use crate::error::ContractError;
use astroport_governance::utils::{get_periods_count, MAX_LOCK_TIME, WEEK};
use cosmwasm_std::{Addr, Decimal, Order, Pair, StdError, StdResult, Storage, Uint128};
use cw_storage_plus::{Bound, U64Key};
use std::cmp::min;
use std::convert::TryInto;

use crate::state::{Point, BLACKLIST, CONFIG, HISTORY, LAST_SLOPE_CHANGE, SLOPE_CHANGES};

/// Checks that a timestamp is within limits.
pub(crate) fn time_limits_check(time: u64) -> Result<(), ContractError> {
    if !(WEEK..=MAX_LOCK_TIME).contains(&time) {
        Err(ContractError::LockTimeLimitsError {})
    } else {
        Ok(())
    }
}

/// ## Description
/// Checks that the sender is the xASTRO token.
pub(crate) fn xastro_token_check(storage: &dyn Storage, sender: Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(storage)?;
    if sender != config.deposit_token_addr {
        Err(ContractError::Unauthorized {})
    } else {
        Ok(())
    }
}

/// ## Description
/// Checks if the blacklist contains a specific address.
pub(crate) fn blacklist_check(storage: &dyn Storage, addr: &Addr) -> Result<(), ContractError> {
    let blacklist = BLACKLIST.load(storage)?;
    if blacklist.contains(addr) {
        Err(ContractError::AddressBlacklisted(addr.to_string()))
    } else {
        Ok(())
    }
}

/// ## Description
/// Adjusting voting power according to the slope. The maximum loss is 103/104 * 104 which is
/// 0.000103 vxASTRO.
pub(crate) fn adjust_vp_and_slope(vp: &mut Uint128, dt: u64) -> StdResult<Uint128> {
    let slope = vp.checked_div(Uint128::from(dt))?;
    *vp = slope * Uint128::from(dt);
    Ok(slope)
}

/// ## Description
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

/// ## Description
/// Coefficient calculation where 0 [`WEEK`] is equal to 1 and [`MAX_LOCK_TIME`] is 2.5.
pub(crate) fn calc_coefficient(interval: u64) -> Decimal {
    // coefficient = 1 + 1.5 * (end - start) / MAX_LOCK_TIME
    Decimal::one() + Decimal::from_ratio(15_u64 * interval, get_periods_count(MAX_LOCK_TIME) * 10)
}

/// ## Description
/// Fetches the last checkpoint in [`HISTORY`] for the given address.
pub(crate) fn fetch_last_checkpoint(
    storage: &dyn Storage,
    addr: &Addr,
    period_key: &U64Key,
) -> StdResult<Option<Pair<Point>>> {
    HISTORY
        .prefix(addr.clone())
        .range(
            storage,
            None,
            Some(Bound::Inclusive(period_key.wrapped.clone())),
            Order::Descending,
        )
        .next()
        .transpose()
}

/// ## Description
/// Cancels scheduled slope change of total voting power only if the given period is in future.
/// Removes scheduled slope change if it became zero.
pub(crate) fn cancel_scheduled_slope(
    storage: &mut dyn Storage,
    slope: Uint128,
    period: u64,
) -> StdResult<()> {
    let end_period_key = U64Key::new(period);
    let last_slope_change = LAST_SLOPE_CHANGE.may_load(storage)?.unwrap_or(0);
    match SLOPE_CHANGES.may_load(storage, end_period_key.clone())? {
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

/// ## Description
/// Schedules slope change of total voting power in the given period.
pub(crate) fn schedule_slope_change(
    storage: &mut dyn Storage,
    slope: Uint128,
    period: u64,
) -> StdResult<()> {
    if !slope.is_zero() {
        SLOPE_CHANGES
            .update(
                storage,
                U64Key::new(period),
                |slope_opt| -> StdResult<Uint128> {
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

/// ## Description
/// Helper function for deserialization.
pub(crate) fn deserialize_pair(pair: StdResult<Pair<Uint128>>) -> StdResult<(u64, Uint128)> {
    let (period_serialized, change) = pair?;
    let period_bytes: [u8; 8] = period_serialized
        .try_into()
        .map_err(|_| StdError::generic_err("Deserialization error"))?;
    Ok((u64::from_be_bytes(period_bytes), change))
}

/// ## Description
/// Fetches all slope changes between `last_slope_change` and `period`.
pub(crate) fn fetch_slope_changes(
    storage: &dyn Storage,
    last_slope_change: u64,
    period: u64,
) -> StdResult<Vec<(u64, Uint128)>> {
    SLOPE_CHANGES
        .range(
            storage,
            Some(Bound::Exclusive(U64Key::new(last_slope_change).wrapped)),
            Some(Bound::Inclusive(U64Key::new(period).wrapped)),
            Order::Ascending,
        )
        .map(deserialize_pair)
        .collect()
}

/// ## Description
/// Calculate slashed and return amount based on a given parameters.
/// The penalty is calculated as min(max_exit_penalty, time_left_until_unlock / MAX_LOCK_TIME).
pub(crate) fn calc_early_withdraw_amount(
    max_exit_penalty: Decimal,
    periods_upon_unlock: u64,
    xastro_amount: Uint128,
) -> (Uint128, Uint128) {
    let user_penalty = Decimal::from_ratio(periods_upon_unlock, get_periods_count(MAX_LOCK_TIME));
    let exact_penalty = min(max_exit_penalty, user_penalty);
    let slashed_amount = xastro_amount * exact_penalty;
    let return_amount = xastro_amount.saturating_sub(slashed_amount);

    (slashed_amount, return_amount)
}
