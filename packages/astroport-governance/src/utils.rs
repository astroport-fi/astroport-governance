use std::convert::TryInto;

use crate::voting_escrow::QueryMsg::{
    LockInfo, TotalVotingPower, TotalVotingPowerAt, UserVotingPower, UserVotingPowerAt,
};
use crate::voting_escrow::{LockInfoResponse, VotingPowerResponse};
use cosmwasm_std::{
    Addr, Decimal, Fraction, OverflowError, QuerierWrapper, StdError, StdResult, Uint128, Uint256,
};

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
    slope: Decimal,
    old_vp: Uint128,
    start_period: u64,
    end_period: u64,
) -> Uint128 {
    let shift = slope
        .checked_mul(Uint128::from(end_period - start_period))
        .unwrap_or_else(|_| Uint128::zero());
    old_vp.saturating_sub(shift)
}

/// ## Description
/// Queries current user's voting power from the voting escrow contract.
pub fn get_voting_power(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &UserVotingPower {
            user: user.to_string(),
        },
    )?;
    Ok(vp.voting_power)
}

/// ## Description
/// Queries current user's voting power from the voting escrow contract by timestamp.
pub fn get_voting_power_at(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
    timestamp: u64,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &UserVotingPowerAt {
            user: user.to_string(),
            time: timestamp,
        },
    )?;

    Ok(vp.voting_power)
}

/// ## Description
/// Queries current total voting power from the voting escrow contract.
pub fn get_total_voting(querier: QuerierWrapper, escrow_addr: &Addr) -> StdResult<Uint128> {
    let vp: VotingPowerResponse =
        querier.query_wasm_smart(escrow_addr.clone(), &TotalVotingPower {})?;

    Ok(vp.voting_power)
}

/// ## Description
/// Queries total voting power from the voting escrow contract by timestamp.
pub fn get_total_voting_at(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    timestamp: u64,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse =
        querier.query_wasm_smart(escrow_addr.clone(), &TotalVotingPowerAt { time: timestamp })?;

    Ok(vp.voting_power)
}

/// ## Description
/// Queries user's lockup information from the voting escrow contract.
pub fn get_lock_info(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
) -> StdResult<LockInfoResponse> {
    let lock_info: LockInfoResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &LockInfo {
            user: user.to_string(),
        },
    )?;
    Ok(lock_info)
}
