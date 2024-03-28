#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, Env, Order, StdResult, Uint128};
use cw_storage_plus::Bound;

use crate::state::{channel_balance_at, total_balance_at, CONFIG, OUTPOSTS, USER_FUNDS};
use astroport_governance::{
    hub::{HubBalance, OutpostConfig, QueryMsg},
    DEFAULT_LIMIT, MAX_LIMIT,
};

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns core contract settings stored in the [`Config`] structure.
///
/// * **QueryMsg::UserFunds { }** Returns a [`HubBalance`] containing the amount of ASTRO this address has held on the Hub due to IBC failures
///
/// * **QueryMsg::Outposts { }** Returns a [`Vec<OutpostResponse>`] containing the active Outposts
///
/// * **QueryMsg::ChannelBalanceAt { channel, timestamp }** Returns a [`HubBalance`] containing the amount of xASTRO minted on the specified  channel at the specified timestamp
///
/// * **QueryMsg::TotalChannelBalancesAt { }** Returns a [`HubBalance`] containing the total amount of xASTRO minted across all channels at a specified time
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::UserFunds { user } => query_user_funds(deps, user),
        QueryMsg::Outposts { start_after, limit } => query_outposts(deps, start_after, limit),
        QueryMsg::ChannelBalanceAt { channel, timestamp } => to_json_binary(&HubBalance {
            balance: channel_balance_at(deps.storage, &channel, timestamp.u64())?,
        }),
        QueryMsg::TotalChannelBalancesAt { timestamp } => to_json_binary(&HubBalance {
            balance: total_balance_at(deps.storage, timestamp.u64())?,
        }),
    }
}

/// Return a list of Outpost in the format of `OutpostConfig`
/// Paged by address and will only return limit at a time
fn query_outposts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.as_deref().map(Bound::exclusive);

    let outposts: Vec<OutpostConfig> = OUTPOSTS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (key, value) = item.unwrap();
            OutpostConfig {
                address: key,
                channel: value.outpost,
                cw20_ics20_channel: value.cw20_ics20,
            }
        })
        .collect();
    to_json_binary(&outposts)
}

/// Return the amount of ASTRO this address has held on the Hub due to IBC
/// failures
fn query_user_funds(deps: Deps, user: Addr) -> StdResult<Binary> {
    let funds = USER_FUNDS
        .load(deps.storage, &user)
        .unwrap_or(Uint128::zero());

    to_json_binary(&HubBalance { balance: funds })
}
