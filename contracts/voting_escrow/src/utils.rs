use crate::error::ContractError;
use astroport_governance::utils::{get_periods_count, MAX_LOCK_TIME, WEEK};

use cosmwasm_std::{Addr, Decimal, Order, StdResult, Storage, Uint128};
use cw_storage_plus::Bound;

use crate::state::{Point, BLACKLIST, CONFIG, HISTORY, LAST_SLOPE_CHANGE, SLOPE_CHANGES};

/// Checks that a timestamp is within limits.
pub(crate) fn time_limits_check(time: u64) -> Result<(), ContractError> {
    if !(WEEK..=MAX_LOCK_TIME).contains(&time) {
        Err(ContractError::LockTimeLimitsError {})
    } else {
        Ok(())
    }
}

/// Checks that the sender is the xASTRO token.
pub(crate) fn xastro_token_check(storage: &dyn Storage, sender: Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(storage)?;
    if sender != config.deposit_token_addr {
        Err(ContractError::Unauthorized {})
    } else {
        Ok(())
    }
}

/// Checks if the blacklist contains a specific address.
pub(crate) fn blacklist_check(storage: &dyn Storage, addr: &Addr) -> Result<(), ContractError> {
    let blacklist = BLACKLIST.load(storage)?;
    if blacklist.contains(addr) {
        Err(ContractError::AddressBlacklisted(addr.to_string()))
    } else {
        Ok(())
    }
}

/// Adjusting voting power according to the slope. The maximum loss is 103/104 * 104 which is
/// 0.000103 vxASTRO.
pub(crate) fn adjust_vp_and_slope(vp: &mut Uint128, dt: u64) -> StdResult<Uint128> {
    let slope = vp.checked_div(Uint128::from(dt))?;
    *vp = slope * Uint128::from(dt);
    Ok(slope)
}

/// Main function used to calculate a user's voting power at a specific period as: previous_power - slope*(x - previous_x).
pub(crate) fn calc_voting_power(point: &Point, period: u64) -> Uint128 {
    let shift = point
        .slope
        .checked_mul(Uint128::from(period - point.start))
        .unwrap_or_else(|_| Uint128::zero());
    point
        .power
        .checked_sub(shift)
        .unwrap_or_else(|_| Uint128::zero())
}

/// Coefficient calculation where 0 [`WEEK`] is equal to 1 and [`MAX_LOCK_TIME`] is 2.5.
pub(crate) fn calc_coefficient(interval: u64) -> Decimal {
    // coefficient = 1 + 1.5 * (end - start) / MAX_LOCK_TIME
    Decimal::one() + Decimal::from_ratio(15_u64 * interval, get_periods_count(MAX_LOCK_TIME) * 10)
}

/// Fetches the last checkpoint in [`HISTORY`] for the given address.
pub(crate) fn fetch_last_checkpoint(
    storage: &dyn Storage,
    addr: &Addr,
    period_key: u64,
) -> StdResult<Option<(u64, Point)>> {
    HISTORY
        .prefix(addr.clone())
        .range(
            storage,
            None,
            Some(Bound::inclusive(period_key)),
            Order::Descending,
        )
        .next()
        .transpose()
}

/// Cancels scheduled slope change of total voting power only if the given period is in future.
/// Removes scheduled slope change if it became zero.
pub(crate) fn cancel_scheduled_slope(
    storage: &mut dyn Storage,
    slope: Uint128,
    period: u64,
) -> StdResult<()> {
    let end_period_key = period;
    let last_slope_change = LAST_SLOPE_CHANGE.may_load(storage)?.unwrap_or(0);
    match SLOPE_CHANGES.may_load(storage, end_period_key)? {
        // We do not need to schedule a slope change in the past
        Some(old_scheduled_change) if period > last_slope_change => {
            let new_slope = old_scheduled_change - slope;
            if !new_slope.is_zero() {
                SLOPE_CHANGES.save(storage, end_period_key, &(old_scheduled_change - slope))
            } else {
                SLOPE_CHANGES.remove(storage, end_period_key);
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

/// Schedules slope change of total voting power in the given period.
pub(crate) fn schedule_slope_change(
    storage: &mut dyn Storage,
    slope: Uint128,
    period: u64,
) -> StdResult<()> {
    if !slope.is_zero() {
        SLOPE_CHANGES
            .update(storage, period, |slope_opt| -> StdResult<Uint128> {
                if let Some(pslope) = slope_opt {
                    Ok(pslope + slope)
                } else {
                    Ok(slope)
                }
            })
            .map(|_| ())
    } else {
        Ok(())
    }
}

/// Fetches all slope changes between `last_slope_change` and `period`.
pub(crate) fn fetch_slope_changes(
    storage: &dyn Storage,
    last_slope_change: u64,
    period: u64,
) -> StdResult<Vec<(u64, Uint128)>> {
    SLOPE_CHANGES
        .range(
            storage,
            Some(Bound::exclusive(last_slope_change)),
            Some(Bound::inclusive(period)),
            Order::Ascending,
        )
        .collect()
}
