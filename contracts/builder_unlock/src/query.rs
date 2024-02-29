#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, Env, Order, StdError, StdResult, Uint128};
use cw_storage_plus::Bound;

use astroport_governance::builder_unlock::{
    AllocationParams, AllocationResponse, QueryMsg, SimulateWithdrawResponse, State,
};
use astroport_governance::{DEFAULT_LIMIT, MAX_LIMIT};

use crate::error::ContractError;
use crate::state::{Allocation, CONFIG, PARAMS, STATE, STATUS};

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::Config {}** Return the contract configuration.
///
/// * **QueryMsg::State {}** Return the contract state (number of ASTRO that still need to be withdrawn).
///
/// * **QueryMsg::Allocation {}** Return the allocation details for a specific account.
///
/// * **QueryMsg::UnlockedTokens {}** Return the amount of unlocked ASTRO for a specific account.
///
/// * **QueryMsg::SimulateWithdraw {}** Return the result of a withdrawal simulation.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::State { timestamp } => to_json_binary(&query_state(deps, timestamp)?),
        QueryMsg::Allocation { account, timestamp } => {
            to_json_binary(&query_allocation(deps, account, timestamp)?)
        }
        QueryMsg::UnlockedTokens { account } => to_json_binary(
            &query_tokens_unlocked(deps, env, account)
                .map_err(|err| StdError::generic_err(err.to_string()))?,
        ),
        QueryMsg::SimulateWithdraw { account, timestamp } => to_json_binary(
            &query_simulate_withdraw(deps, env, account, timestamp)
                .map_err(|err| StdError::generic_err(err.to_string()))?,
        ),
        QueryMsg::Allocations { start_after, limit } => {
            to_json_binary(&query_allocations(deps, start_after, limit)?)
        }
    }
}

/// Query either historical or current contract state.
pub fn query_state(deps: Deps, timestamp: Option<u64>) -> StdResult<State> {
    if let Some(timestamp) = timestamp {
        // Loads state at specific timestamp. State changes reflected **after** block has been produced.
        STATE.may_load_at_height(deps.storage, timestamp)
    } else {
        // Loads latest state. Can load allocation state at the current block timestamp.
        STATE.may_load(deps.storage)
    }
    .map(|state| state.unwrap_or_default())
}

/// Return either historical or current information about a specific allocation.
///
/// * **account** account whose allocation we query.
///
/// * **timestamp** timestamp at which we query the allocation. Optional.
pub fn query_allocation(
    deps: Deps,
    account: String,
    timestamp: Option<u64>,
) -> StdResult<AllocationResponse> {
    let receiver = deps.api.addr_validate(&account)?;
    let params = PARAMS
        .may_load(deps.storage, &receiver)?
        .unwrap_or_default();

    let status = if let Some(timestamp) = timestamp {
        // Loads allocation state at specific timestamp. State changes reflected **after** block has been produced.
        STATUS
            .may_load_at_height(deps.storage, &receiver, timestamp)?
            .unwrap_or_default()
    } else {
        // Loads latest allocation state. Can load allocation state at the current block timestamp.
        STATUS
            .may_load(deps.storage, &receiver)?
            .unwrap_or_default()
    };

    Ok(AllocationResponse { params, status })
}

/// Return information about a specific allocation.
///
/// * **start_after** account from which to start querying.
///
/// * **limit** max amount of entries to return.
pub fn query_allocations(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<(Addr, AllocationParams)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let default_start;

    let start = if let Some(start_after) = start_after {
        default_start = deps.api.addr_validate(&start_after)?;
        Some(Bound::exclusive(&default_start))
    } else {
        None
    };

    PARAMS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .collect()
}

/// Return the total amount of unlocked tokens for a specific account.
///
/// * **account** account whose unlocked token amount we query.
pub fn query_tokens_unlocked(
    deps: Deps,
    env: Env,
    account: String,
) -> Result<Uint128, ContractError> {
    let receiver = deps.api.addr_validate(&account)?;
    let block_ts = env.block.time.seconds();
    let allocation = Allocation::must_load(deps.storage, block_ts, &receiver)?;

    Ok(allocation.compute_unlocked_amount(block_ts))
}

/// Simulate a token withdrawal.
///
/// * **account** account for which we simulate a withdrawal.
///
/// * **timestamp** timestamp where we assume the account would withdraw.
pub fn query_simulate_withdraw(
    deps: Deps,
    env: Env,
    account: String,
    timestamp: Option<u64>,
) -> Result<SimulateWithdrawResponse, ContractError> {
    let receiver = deps.api.addr_validate(&account)?;
    let allocation = Allocation::must_load(deps.storage, env.block.time.seconds(), &receiver)?;
    let timestamp = timestamp.unwrap_or_else(|| env.block.time.seconds());

    Ok(allocation.compute_withdraw_amount(timestamp))
}
