use crate::contract::{MAX_LOCK_TIME, WEEK};
use crate::error::ContractError;
use cosmwasm_std::{Addr, Deps, Env, StdResult, Timestamp, Uint128};
use cw20::{BalanceResponse, Cw20QueryMsg};

use crate::state::CONFIG;

pub(crate) fn get_current_period(env: Env) -> u64 {
    env.block.time.seconds() / WEEK
}

pub(crate) fn get_unlock_period(env: Env, time: &Timestamp) -> Result<u64, ContractError> {
    if time.seconds() < WEEK || time.seconds() > MAX_LOCK_TIME {
        Err(ContractError::LockTimeLimitsError {})
    } else {
        let final_period = get_current_period(env) + time.seconds() / WEEK;
        Ok(final_period)
    }
}

pub(crate) fn xastro_token_check(deps: Deps, sender: Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if sender != config.xastro_token_addr {
        Err(ContractError::Unauthorized {})
    } else {
        Ok(())
    }
}

/// ## Description
/// Returns the total deposit of locked xASTRO tokens.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
pub fn get_total_deposit(deps: Deps, env: Env) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let result: BalanceResponse = deps.querier.query_wasm_smart(
        &config.xastro_token_addr,
        &Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;
    Ok(result.balance)
}
