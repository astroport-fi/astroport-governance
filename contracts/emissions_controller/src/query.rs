use std::collections::{HashMap, HashSet};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, Order, StdError, StdResult};
use cw_storage_plus::Bound;
use itertools::Itertools;
use neutron_sdk::bindings::query::NeutronQuery;

use astroport_governance::emissions_controller::consts::MAX_PAGE_LIMIT;
use astroport_governance::emissions_controller::hub::{
    QueryMsg, SimulateTuneResponse, UserInfoResponse,
};

use crate::error::ContractError;
use crate::state::{CONFIG, OUTPOSTS, POOLS_WHITELIST, TUNE_INFO, USER_INFO, VOTED_POOLS};
use crate::utils::{get_epoch_start, simulate_tune};

/// Expose available contract queries.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<NeutronQuery>, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::UserInfo { user, timestamp } => {
            let block_time = env.block.time.seconds();
            let timestamp = timestamp.unwrap_or(block_time);
            let user_info = match timestamp {
                timestamp if timestamp == block_time => USER_INFO.may_load(deps.storage, &user),
                timestamp => USER_INFO.may_load_at_height(deps.storage, &user, timestamp),
            }?
            .unwrap_or_default();

            let applied_votes = user_info
                .votes
                .iter()
                .filter_map(|(pool, weight)| {
                    let data = if timestamp == block_time {
                        VOTED_POOLS.may_load(deps.storage, pool)
                    } else {
                        VOTED_POOLS.may_load_at_height(deps.storage, pool, timestamp)
                    };

                    match data {
                        Ok(Some(pool_info)) if pool_info.init_ts <= user_info.vote_ts => {
                            Some(Ok((pool.clone(), *weight)))
                        }
                        Err(err) => Some(Err(err)),
                        _ => None,
                    }
                })
                .try_collect()?;

            let response = UserInfoResponse {
                vote_ts: user_info.vote_ts,
                voting_power: user_info.voting_power,
                votes: user_info.votes,
                applied_votes,
            };

            Ok(to_json_binary(&response)?)
        }
        QueryMsg::TuneInfo { timestamp } => {
            let block_time = env.block.time.seconds();
            let timestamp = timestamp.unwrap_or(block_time);
            let tune_info = match timestamp {
                timestamp if timestamp == block_time => TUNE_INFO.may_load(deps.storage),
                timestamp => TUNE_INFO.may_load_at_height(deps.storage, timestamp),
            }?
            .ok_or_else(|| StdError::generic_err(format!("Tune info not found at {timestamp}")))?;
            Ok(to_json_binary(&tune_info)?)
        }
        QueryMsg::Config {} => Ok(to_json_binary(&CONFIG.load(deps.storage)?)?),
        QueryMsg::VotedPool { pool, timestamp } => {
            let block_time = env.block.time.seconds();
            let timestamp = timestamp.unwrap_or(block_time);
            let voted_pool = match timestamp {
                timestamp if timestamp == block_time => VOTED_POOLS.may_load(deps.storage, &pool),
                timestamp => VOTED_POOLS.may_load_at_height(deps.storage, &pool, timestamp),
            }?
            .ok_or_else(|| StdError::generic_err(format!("Voted pool not found at {timestamp}")))?;
            Ok(to_json_binary(&voted_pool)?)
        }
        QueryMsg::VotedPoolsList { limit, start_after } => {
            let limit = limit.unwrap_or(MAX_PAGE_LIMIT) as usize;
            let voted_pools = VOTED_POOLS
                .range(
                    deps.storage,
                    start_after.as_ref().map(|s| Bound::exclusive(s.as_str())),
                    None,
                    Order::Ascending,
                )
                .take(limit)
                .collect::<StdResult<Vec<_>>>()?;
            Ok(to_json_binary(&voted_pools)?)
        }
        QueryMsg::ListOutposts {} => {
            let outposts = OUTPOSTS
                .range(deps.storage, None, None, Order::Ascending)
                .collect::<StdResult<Vec<_>>>()?;
            Ok(to_json_binary(&outposts)?)
        }
        QueryMsg::QueryWhitelist {} => Ok(to_json_binary(&POOLS_WHITELIST.load(deps.storage)?)?),
        QueryMsg::SimulateTune {} => {
            let deps = deps.into_empty();

            let voted_pools = VOTED_POOLS
                .keys(deps.storage, None, None, Order::Ascending)
                .collect::<StdResult<HashSet<_>>>()?;
            let outposts = OUTPOSTS
                .range(deps.storage, None, None, Order::Ascending)
                .collect::<StdResult<HashMap<_, _>>>()?;
            let epoch_start = get_epoch_start(env.block.time.seconds());
            let config = CONFIG.load(deps.storage)?;

            let tune_result = simulate_tune(deps, &voted_pools, &outposts, epoch_start, &config)?;
            Ok(to_json_binary(&SimulateTuneResponse {
                new_emissions_state: tune_result.new_emissions_state,
                next_pools_grouped: tune_result.next_pools_grouped,
            })?)
        }
    }
}
