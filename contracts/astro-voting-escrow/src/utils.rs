use crate::contract::{MAX_LOCK_TIME, WEEK};
use crate::error::ContractError;
use cosmwasm_std::{
    Addr, Decimal, Deps, Fraction, Order, OverflowError, Pair, StdResult, Uint128, Uint256,
};
use cw_storage_plus::{Bound, U64Key};
use std::convert::TryInto;

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

pub(crate) fn apply_boost(amount: Uint128, interval: u64) -> Uint128 {
    // boost = 2.5 * (end - start) / MAX_LOCK_TIME
    amount * Uint128::from(25_u64 * interval) / Uint128::from(get_period(MAX_LOCK_TIME) * 10)
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
