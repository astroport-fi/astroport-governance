use std::convert::TryInto;

use cosmwasm_std::{Decimal, Fraction, OverflowError, StdError, StdResult, Uint128, Uint256};

/// Seconds in one week. Constant is intended for period number calculation.
pub const WEEK: u64 = 7 * 86400; // lock period is rounded down by week

/// Seconds in 2 years which is maximum lock period.
pub const MAX_LOCK_TIME: u64 = 2 * 365 * 86400; // 2 years (104 weeks)

/// The constant describes the maximum number of accounts for claim.
pub const CLAIM_LIMIT: u64 = 10;

/// Feb 28 2022 00:00 UTC, Monday
pub const EPOCH_START: u64 = 1646006400;

/// ## Description
/// Calculates the period number. Time should be formatted as a timestamp.
pub fn get_period(time: u64) -> StdResult<u64> {
    if time < EPOCH_START {
        Err(StdError::generic_err("Invalid time"))
    } else {
        Ok((time - EPOCH_START) / WEEK)
    }
}

/// ## Description
/// Calculates how many periods are in the specified time interval. The time should be in seconds.
pub fn get_periods_count(interval: u64) -> u64 {
    interval / WEEK
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
pub fn calc_voting_power(
    slope: Uint128,
    old_vp: Uint128,
    start_period: u64,
    end_period: u64,
) -> Uint128 {
    let shift = slope
        .checked_mul(Uint128::from(end_period - start_period))
        .unwrap_or_else(|_| Uint128::zero());
    old_vp.saturating_sub(shift)
}
