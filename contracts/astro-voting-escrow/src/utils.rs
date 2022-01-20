use crate::contract::{MAX_LOCK_TIME, WEEK};
use crate::error::ContractError;
use cosmwasm_std::{Deps, DepsMut, Env, StdError, StdResult, Timestamp, Uint128};

use crate::state::{Config, CONFIG};

pub(crate) enum ChangeBalanceOp {
    Add,
    Sub,
}

pub(crate) fn change_balance(deps: DepsMut, op: ChangeBalanceOp, amount: Uint128) -> StdResult<()> {
    let mut config = CONFIG.load(deps.storage)?;
    config.balance = match op {
        ChangeBalanceOp::Add => config.balance.checked_add(amount)?,
        ChangeBalanceOp::Sub => config.balance.checked_sub(amount)?,
    };
    CONFIG.save(deps.storage, &config)
}

pub(crate) fn time_limits_check(time: &Timestamp) -> Result<(), ContractError> {
    if time.seconds() < WEEK || time.seconds() > MAX_LOCK_TIME {
        Err(ContractError::LockTimeLimitsError {})
    } else {
        Ok(())
    }
}
