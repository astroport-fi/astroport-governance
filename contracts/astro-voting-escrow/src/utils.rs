use crate::contract::{MAX_LOCK_TIME, WEEK};
use crate::error::ContractError;
use cosmwasm_std::{Addr, Deps, Uint128};

use crate::state::{Lock, CONFIG};

pub(crate) fn time_limits_check(time: u64) -> Result<(), ContractError> {
    if !(WEEK..=MAX_LOCK_TIME).contains(&time) {
        Err(ContractError::LockTimeLimitsError {})
    } else {
        Ok(())
    }
}

pub(crate) fn get_period(time: u64) -> u64 {
    time / WEEK
}

pub(crate) fn xastro_token_check(deps: Deps, sender: Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if sender != config.xastro_token_addr {
        Err(ContractError::Unauthorized {})
    } else {
        Ok(())
    }
}

pub(crate) fn calc_voting_power(lock: Lock, cur_period: u64) -> Uint128 {
    let power = lock.power.u128() as f32;
    let slope = power / (lock.end - lock.start) as f32;
    let voting_power = power - slope * (cur_period - lock.start) as f32;
    // if it goes below zero then u128 will adjust it to 0
    Uint128::from(voting_power.round() as u128)
}
