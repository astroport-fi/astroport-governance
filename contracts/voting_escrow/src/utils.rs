use crate::error::ContractError;
use astroport::asset::addr_validate_to_lower;
use astroport_governance::utils::{get_periods_count, MAX_LOCK_TIME, WEEK};
use cosmwasm_std::{
    Addr, Decimal, Deps, DepsMut, Fraction, Order, OverflowError, Pair, StdError, StdResult,
    Uint128, Uint256,
};
use cw_storage_plus::{Bound, U64Key};
use std::convert::TryInto;

use crate::state::{Point, BLACKLIST, CONFIG, HISTORY, LAST_SLOPE_CHANGE, SLOPE_CHANGES};

/// # Description
/// Checks that a timestamp is within limits.
pub(crate) fn time_limits_check(time: u64) -> Result<(), ContractError> {
    if !(WEEK..=MAX_LOCK_TIME).contains(&time) {
        Err(ContractError::LockTimeLimitsError {})
    } else {
        Ok(())
    }
}

/// # Description
/// Checks that the sender is the xASTRO token.
pub(crate) fn xastro_token_check(deps: Deps, sender: Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if sender != config.deposit_token_addr {
        Err(ContractError::Unauthorized {})
    } else {
        Ok(())
    }
}

/// # Description
/// Checks if the blacklist contains a specific address.
pub(crate) fn blacklist_check(deps: Deps, addr: &Addr) -> Result<(), ContractError> {
    let blacklist = BLACKLIST.load(deps.storage)?;
    if blacklist.contains(addr) {
        Err(ContractError::AddressBlacklisted(addr.to_string()))
    } else {
        Ok(())
    }
}

/// # Description
/// This trait was implemented to eliminate Decimal rounding problems.
trait DecimalRoundedCheckedMul {
    fn checked_mul(self, other: Uint128) -> Result<Uint128, OverflowError>;
}

impl DecimalRoundedCheckedMul for Decimal {
    fn checked_mul(self, other: Uint128) -> Result<Uint128, OverflowError> {
        if self.is_zero() || other.is_zero() {
            return Ok(Uint128::zero());
        }
        let numerator = other.full_mul(self.numerator());
        let multiply_ratio = numerator / Uint256::from(self.denominator());
        if multiply_ratio > Uint256::from(Uint128::MAX) {
            Err(OverflowError::new(
                cosmwasm_std::OverflowOperation::Mul,
                self,
                other,
            ))
        } else {
            let mut result: Uint128 = multiply_ratio.try_into().unwrap();
            let rem: Uint128 = numerator
                .checked_rem(Uint256::from(self.denominator()))
                .unwrap()
                .try_into()
                .unwrap();
            // 0.5 in Decimal
            if rem.u128() >= 500000000000000000_u128 {
                result += Uint128::from(1_u128);
            }
            Ok(result)
        }
    }
}

/// # Description
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

/// # Description
/// Coefficient calculation where 0 [`WEEK`] is equal to 1 and [`MAX_LOCK_TIME`] is 2.5.
pub(crate) fn calc_coefficient(interval: u64) -> Decimal {
    // coefficient = 1 + 1.5 * (end - start) / MAX_LOCK_TIME
    Decimal::one() + Decimal::from_ratio(15_u64 * interval, get_periods_count(MAX_LOCK_TIME) * 10)
}

/// # Description
/// Fetches the last checkpoint in [`HISTORY`] for the given address.
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
        // We do not need to schedule a slope change in the past
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
/// Helper function for deserialization.
pub(crate) fn deserialize_pair(pair: StdResult<Pair<Decimal>>) -> StdResult<(u64, Decimal)> {
    let (period_serialized, change) = pair?;
    let period_bytes: [u8; 8] = period_serialized
        .try_into()
        .map_err(|_| StdError::generic_err("Deserialization error"))?;
    Ok((u64::from_be_bytes(period_bytes), change))
}

/// # Description
/// Fetches all slope changes between `last_slope_change` and `period`.
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
/// Bulk validation and conversion between [`String`] -> [`Addr`] for an array of addresses.
/// If any address is invalid, the function returns [`StdError`].
pub(crate) fn validate_addresses(deps: Deps, addresses: &[String]) -> StdResult<Vec<Addr>> {
    addresses
        .iter()
        .map(|addr| addr_validate_to_lower(deps.api, addr))
        .collect()
}
