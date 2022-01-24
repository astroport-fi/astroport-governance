use crate::contract::{MAX_LOCK_TIME, WEEK};
use crate::error::ContractError;
use cosmwasm_std::{Addr, Deps, Env, StdResult, Timestamp, Uint128};
use cw20::{BalanceResponse, Cw20QueryMsg};

use crate::state::CONFIG;

pub(crate) fn time_limits_check(time: u64) -> Result<(), ContractError> {
    if !(WEEK..=MAX_LOCK_TIME).contains(&time) {
        Err(ContractError::LockTimeLimitsError {})
    } else {
        Ok(())
    }
}

pub(crate) fn cur_period(time: Timestamp) -> u64 {
    time.seconds() / WEEK
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
