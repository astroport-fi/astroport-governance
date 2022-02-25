use crate::error::ContractError;
use astroport::asset::addr_validate_to_lower;
use astroport_governance::utils::{get_period, MAX_LOCK_TIME, WEEK};
use cosmwasm_std::{Addr, Decimal, Deps, DepsMut, Order, Pair, StdError, StdResult};
use cw_storage_plus::{Bound, U64Key};
use std::convert::TryInto;

use crate::state::{Point, BLACKLIST, CONFIG, HISTORY, LAST_SLOPE_CHANGE, SLOPE_CHANGES};

/// # Description
/// Checks the time is within limits
pub(crate) fn time_limits_check(time: u64) -> Result<(), ContractError> {
    if !(WEEK..=MAX_LOCK_TIME).contains(&time) {
        Err(ContractError::LockTimeLimitsError {})
    } else {
        Ok(())
    }
}

/// # Description
/// Checks the sender is xASTRO token
pub(crate) fn xastro_token_check(deps: Deps, sender: Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if sender != config.deposit_token_addr {
        Err(ContractError::Unauthorized {})
    } else {
        Ok(())
    }
}

pub(crate) fn blacklist_check(deps: Deps, addr: &Addr) -> Result<(), ContractError> {
    let blacklist = BLACKLIST.load(deps.storage)?;
    if blacklist.contains(addr) {
        Err(ContractError::AddressBlacklisted(addr.to_string()))
    } else {
        Ok(())
    }
}

/// # Description
/// Coefficient calculation where 0 [`WEEK`] equals to 1 and [`MAX_LOCK_TIME`] equals to 2.5.
pub(crate) fn calc_coefficient(interval: u64) -> Decimal {
    // coefficient = 1 + 1.5 * (end - start) / MAX_LOCK_TIME
    Decimal::one() + Decimal::from_ratio(15_u64 * interval, get_period(MAX_LOCK_TIME) * 10)
}

/// # Description
/// Fetches last checkpoint in [`HISTORY`] for given address.
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
            Order::Descending,
        )
        .next()
        .transpose()
}

pub(crate) fn cancel_scheduled_slope(deps: DepsMut, slope: Decimal, period: u64) -> StdResult<()> {
    let end_period_key = U64Key::new(period);
    let last_slope_change = LAST_SLOPE_CHANGE
        .may_load(deps.as_ref().storage)?
        .unwrap_or(0);
    match SLOPE_CHANGES.may_load(deps.as_ref().storage, end_period_key.clone())? {
        // we do not need to schedule slope change in the past
        Some(old_scheduled_change) if period > last_slope_change => {
            let new_slope = old_scheduled_change - slope;
            if !new_slope.is_zero() {
                SLOPE_CHANGES.save(
                    deps.storage,
                    end_period_key,
                    &(old_scheduled_change - slope),
                )
            } else {
                SLOPE_CHANGES.remove(deps.storage, end_period_key);
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

pub(crate) fn schedule_slope_change(deps: DepsMut, slope: Decimal, period: u64) -> StdResult<()> {
    if !slope.is_zero() {
        SLOPE_CHANGES
            .update(
                deps.storage,
                U64Key::new(period),
                |slope_opt| -> StdResult<Decimal> {
                    if let Some(pslope) = slope_opt {
                        Ok(pslope + slope)
                    } else {
                        Ok(slope)
                    }
                },
            )
            .map(|_| ())
    } else {
        Ok(())
    }
}

/// # Description
/// Helper function for deserialization
pub(crate) fn deserialize_pair(pair: StdResult<Pair<Decimal>>) -> StdResult<(u64, Decimal)> {
    let (period_serialized, change) = pair?;
    let period_bytes: [u8; 8] = period_serialized
        .try_into()
        .map_err(|_| StdError::generic_err("Deserialization error"))?;
    Ok((u64::from_be_bytes(period_bytes), change))
}

/// # Description
/// Fetches all slope changes between last_slope_change and period.
pub(crate) fn fetch_slope_changes(
    deps: Deps,
    last_slope_change: u64,
    period: u64,
) -> StdResult<Vec<(u64, Decimal)>> {
    SLOPE_CHANGES
        .range(
            deps.storage,
            Some(Bound::Exclusive(U64Key::new(last_slope_change).wrapped)),
            Some(Bound::Inclusive(U64Key::new(period).wrapped)),
            Order::Ascending,
        )
        .map(deserialize_pair)
        .collect()
}

/// # Description
/// Bulk validation and converting [`String`] -> [`Addr`] of array with addresses.
/// If any address is invalid returns [`StdError`].
pub(crate) fn validate_addresses(deps: Deps, addresses: &[String]) -> StdResult<Vec<Addr>> {
    addresses
        .iter()
        .map(|addr| addr_validate_to_lower(deps.api, addr))
        .collect()
}
