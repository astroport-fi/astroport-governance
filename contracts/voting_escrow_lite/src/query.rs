#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Addr, Binary, Deps, Env, StdError, StdResult, Uint128, Uint64};

use cw20::{BalanceResponse, TokenInfoResponse};
use cw20_base::contract::{query_download_logo, query_marketing_info};
use cw20_base::state::TOKEN_INFO;

use astroport_governance::voting_escrow_lite::{
    BlacklistedVotersResponse, LockInfoResponse, QueryMsg, VotingPowerResponse, DEFAULT_LIMIT,
    MAX_LIMIT,
};

use crate::state::{BLACKLIST, CONFIG, LOCKED};
use crate::utils::fetch_last_checkpoint;

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::CheckVotersAreBlacklisted { voters }** Check if the provided voters are blacklisted.
///
/// * **QueryMsg::BlacklistedVoters { start_after, limit }** Fetch all blacklisted voters.
///
/// * **QueryMsg::TotalVotingPower {}** Fetch the total voting power (vxASTRO supply) at the current block. Always returns 0 in this version.
///
/// * **QueryMsg::TotalVotingPowerAt { .. }** Fetch the total voting power (vxASTRO supply) at a specified timestamp. Always returns 0 in this version.
///
/// * **QueryMsg::TotalVotingPowerAtPeriod { .. }** Fetch the total voting power (vxASTRO supply) at a specified period. Always returns 0 in this version.
///
/// * **QueryMsg::UserVotingPower{ .. }** Fetch the user's voting power (vxASTRO balance) at the current block. Always returns 0 in this version.
///
/// * **QueryMsg::UserVotingPowerAt { .. }** Fetch the user's voting power (vxASTRO balance) at a specified timestamp. Always returns 0 in this version.
///
/// * **QueryMsg::UserVotingPowerAtPeriod { .. }** Fetch the user's voting power (vxASTRO balance) at a specified period. Always returns 0 in this version.
///
/// * **QueryMsg::TotalEmissionsVotingPower {}** Fetch the total emissions voting power at the current block.
///
/// * **QueryMsg::TotalEmissionsVotingPowerAt { time }** Fetch the total emissions voting power at a specified timestamp.
///
/// * **QueryMsg::UserEmissionsVotingPower { user }** Fetch a user's emissions voting power at the current block.
///
/// * **QueryMsg::UserEmissionsVotingPowerAt { user, time }** Fetch a user's emissions voting power at a specified timestamp.
///
/// * **QueryMsg::LockInfo { user }** Fetch a user's lock information.
///
/// * **QueryMsg::UserDepositAt { user, timestamp }** Fetch a user's deposit at a specified timestamp.
///
/// * **QueryMsg::Config {}** Fetch the contract's config.
///
/// * **QueryMsg::Balance { address: _ }** Fetch the user's balance. Always returns 0 in this version.
///
/// * **QueryMsg::TokenInfo {}** Fetch the token's information.
///
/// * **QueryMsg::MarketingInfo {}** Fetch the token's marketing information.
///
/// * **QueryMsg::DownloadLogo {}** Fetch the token's logo.
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::CheckVotersAreBlacklisted { voters } => {
            to_binary(&check_voters_are_blacklisted(deps, voters)?)
        }
        QueryMsg::BlacklistedVoters { start_after, limit } => {
            to_binary(&get_blacklisted_voters(deps, start_after, limit)?)
        }
        QueryMsg::TotalVotingPower {} => to_binary(&VotingPowerResponse {
            voting_power: Uint128::zero(),
        }),
        QueryMsg::TotalVotingPowerAt { .. } => to_binary(&VotingPowerResponse {
            voting_power: Uint128::zero(),
        }),
        QueryMsg::TotalVotingPowerAtPeriod { .. } => to_binary(&VotingPowerResponse {
            voting_power: Uint128::zero(),
        }),
        QueryMsg::UserVotingPower { .. } => to_binary(&VotingPowerResponse {
            voting_power: Uint128::zero(),
        }),
        QueryMsg::UserVotingPowerAt { .. } => to_binary(&VotingPowerResponse {
            voting_power: Uint128::zero(),
        }),
        QueryMsg::UserVotingPowerAtPeriod { .. } => to_binary(&VotingPowerResponse {
            voting_power: Uint128::zero(),
        }),
        QueryMsg::TotalEmissionsVotingPower {} => {
            to_binary(&get_total_emissions_voting_power(deps, env, None)?)
        }
        QueryMsg::TotalEmissionsVotingPowerAt { time } => {
            to_binary(&get_total_emissions_voting_power(deps, env, Some(time))?)
        }
        QueryMsg::UserEmissionsVotingPower { user } => {
            to_binary(&get_user_emissions_voting_power(deps, env, user, None)?)
        }
        QueryMsg::UserEmissionsVotingPowerAt { user, time } => to_binary(
            &get_user_emissions_voting_power(deps, env, user, Some(time))?,
        ),
        QueryMsg::LockInfo { user } => to_binary(&get_user_lock_info(deps, env, user)?),
        QueryMsg::UserDepositAt { user, timestamp } => {
            to_binary(&get_user_deposit_at_time(deps, user, timestamp)?)
        }
        QueryMsg::Config {} => {
            let config = CONFIG.load(deps.storage)?;
            to_binary(&config)
        }
        QueryMsg::Balance { address } => to_binary(&get_user_balance(deps, env, address)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps, env)?),
        QueryMsg::MarketingInfo {} => to_binary(&query_marketing_info(deps)?),
        QueryMsg::DownloadLogo {} => to_binary(&query_download_logo(deps)?),
    }
}

/// Checks if specified addresses are blacklisted.
///
/// * **voters** addresses to check if they are blacklisted.
pub fn check_voters_are_blacklisted(
    deps: Deps,
    voters: Vec<String>,
) -> StdResult<BlacklistedVotersResponse> {
    let black_list = BLACKLIST.load(deps.storage)?;

    for voter in voters {
        let voter_addr = deps.api.addr_validate(voter.as_str())?;
        if !black_list.contains(&voter_addr) {
            return Ok(BlacklistedVotersResponse::VotersNotBlacklisted { voter });
        }
    }

    Ok(BlacklistedVotersResponse::VotersBlacklisted {})
}

/// Returns a list of blacklisted voters.
///
/// * **start_after** is an optional field that specifies whether the function should return
/// a list of voters starting from a specific address onward.
///
/// * **limit** max amount of voters addresses to return.
pub fn get_blacklisted_voters(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<Addr>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let mut black_list = BLACKLIST.load(deps.storage)?;

    if black_list.is_empty() {
        return Ok(vec![]);
    }

    black_list.sort();

    let mut start_index = Default::default();
    if let Some(start_after) = start_after {
        let start_addr = deps.api.addr_validate(start_after.as_str())?;
        start_index = black_list
            .iter()
            .position(|addr| *addr == start_addr)
            .ok_or_else(|| {
                StdError::generic_err(format!(
                    "The {} address is not blacklisted",
                    start_addr.as_str()
                ))
            })?
            + 1; // start from the next element of the slice
    }

    // validate end index of the slice
    let end_index = (start_index + limit).min(black_list.len());

    Ok(black_list[start_index..end_index].to_vec())
}

/// Return a user's lock information.
///
/// * **user** user for which we return lock information.
fn get_user_lock_info(deps: Deps, _env: Env, user: String) -> StdResult<LockInfoResponse> {
    let addr = deps.api.addr_validate(&user)?;
    if let Some(lock) = LOCKED.may_load(deps.storage, addr)? {
        let resp = LockInfoResponse {
            amount: lock.amount,
            end: lock.end,
        };
        Ok(resp)
    } else {
        Err(StdError::generic_err("User is not found"))
    }
}

/// Fetches a user's emissions voting power at the current block and uses that
/// as the balance
///
/// * **user** user/staker for which we fetch the current voting power (vxASTRO balance).
fn get_user_balance(deps: Deps, env: Env, user: String) -> StdResult<BalanceResponse> {
    let vp_response = get_user_emissions_voting_power(deps, env, user, None)?;
    Ok(BalanceResponse {
        balance: vp_response.voting_power,
    })
}

/// Return a user's staked xASTRO amount at a given timestamp.
///
/// * **user** user for which we return lock information.
///
/// * **timestamp** timestamp at which we return the staked xASTRO amount.
fn get_user_deposit_at_time(deps: Deps, user: String, timestamp: Uint64) -> StdResult<Uint128> {
    let addr = deps.api.addr_validate(&user)?;
    let locked_opt = LOCKED.may_load_at_height(deps.storage, addr, timestamp.u64())?;
    if let Some(lock) = locked_opt {
        Ok(lock.amount)
    } else {
        Ok(Uint128::zero())
    }
}

/// Fetch a user's emissions voting power at the current block if no time
/// is specified, else uses the given time. If a user is blacklisted, this will
/// return 0
///
/// * **user** user/staker for which we fetch the current emissions voting power.
///
/// * **time** optional time at which to fetch the user's emissions voting power.
fn get_user_emissions_voting_power(
    deps: Deps,
    env: Env,
    user: String,
    time: Option<u64>,
) -> StdResult<VotingPowerResponse> {
    let user = deps.api.addr_validate(&user)?;
    let timestamp = time.unwrap_or_else(|| env.block.time.seconds());
    let last_checkpoint = fetch_last_checkpoint(deps.storage, &user, timestamp)?;

    if let Some(emissions_power) = last_checkpoint.map(|(_, emissions_power)| emissions_power) {
        // The voting power point at the specified `time` was found
        Ok(VotingPowerResponse {
            voting_power: emissions_power,
        })
    } else {
        // User not found
        Ok(VotingPowerResponse {
            voting_power: Uint128::zero(),
        })
    }
}

/// Fetch the total emissions voting power at the current block if no time
/// is specified, else uses the given time.
///
/// * **time** optional time at which to fetch the user's emissions voting power.
fn get_total_emissions_voting_power(
    deps: Deps,
    env: Env,
    time: Option<u64>,
) -> StdResult<VotingPowerResponse> {
    let timestamp = time.unwrap_or_else(|| env.block.time.seconds());
    let last_checkpoint = fetch_last_checkpoint(deps.storage, &env.contract.address, timestamp)?;

    let emissions_power =
        last_checkpoint.map_or(Uint128::zero(), |(_, emissions_power)| emissions_power);
    Ok(VotingPowerResponse {
        voting_power: emissions_power,
    })
}

/// Fetch the vxASTRO token information, such as the token name, symbol, decimals and total supply (total voting power).
fn query_token_info(deps: Deps, _env: Env) -> StdResult<TokenInfoResponse> {
    let info = TOKEN_INFO.load(deps.storage)?;
    let res = TokenInfoResponse {
        name: info.name,
        symbol: info.symbol,
        decimals: info.decimals,
        total_supply: Uint128::zero(),
    };
    Ok(res)
}
