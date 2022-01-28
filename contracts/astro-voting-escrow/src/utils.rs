use crate::contract::{MAX_LOCK_TIME, WEEK};
use crate::error::ContractError;
use cosmwasm_std::{Addr, Api, Deps, Order, Pair, StdError, StdResult, Uint128};
use cw_storage_plus::{Bound, U64Key};

use crate::state::{Point, CONFIG, HISTORY};

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

pub(crate) fn calc_voting_power(point: &Point, period: u64) -> Uint128 {
    let power = point.power.u128() as f32;
    let voting_power = power - f32::from(point.slope.clone()) * (period - point.start) as f32;
    // if it goes below zero then u128 will adjust it to 0
    Uint128::from(voting_power.round() as u128)
}

pub(crate) fn fetch_last_checkpoint(
    deps: Deps,
    addr: &Addr,
    period_key: &U64Key,
) -> StdResult<Option<Pair<Point>>> {
    HISTORY
        .prefix(addr.clone())
        .range(
            deps.storage,
            None,
            Some(Bound::Inclusive(period_key.wrapped.clone())),
            Order::Ascending,
        )
        .last()
        .transpose()
}

/// ## Description
/// Returns the validated address in lowercase on success. Otherwise returns [`Err`]
/// ## Params
/// * **api** is a object of type [`Api`]
///
/// * **addr** is the object of type [`Addr`]
pub(crate) fn addr_validate_to_lower(api: &dyn Api, addr: &str) -> StdResult<Addr> {
    if addr.to_lowercase() != addr {
        return Err(StdError::generic_err(format!(
            "Address {} should be lowercase",
            addr
        )));
    }
    api.addr_validate(addr)
}
