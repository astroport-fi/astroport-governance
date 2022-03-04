use crate::error::ContractError;
use cosmwasm_std::{Decimal, Fraction, StdError, StdResult, Uint128, Uint256};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::ops::Mul;

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct BasicPoints(u16);

impl BasicPoints {
    pub const MAX: u16 = 10000;

    pub fn checked_add(self, rhs: Self) -> Result<Self, ContractError> {
        let next_value = self.0 + rhs.0;
        if next_value > Self::MAX {
            Err(ContractError::BPSLimitError {})
        } else {
            Ok(Self(next_value))
        }
    }

    pub fn from_ratio(numerator: Uint128, denominator: Uint128) -> Result<Self, ContractError> {
        numerator
            .checked_multiply_ratio(Self::MAX, denominator)?
            .u128()
            .try_into()
    }
}

impl TryFrom<u16> for BasicPoints {
    type Error = ContractError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err(ContractError::BPSConverstionError(value as u128))
        }
    }
}

impl TryFrom<u128> for BasicPoints {
    type Error = ContractError;

    fn try_from(value: u128) -> Result<Self, Self::Error> {
        if value <= Self::MAX as u128 {
            Ok(Self(value as u16))
        } else {
            Err(ContractError::BPSConverstionError(value))
        }
    }
}

impl From<BasicPoints> for u16 {
    fn from(value: BasicPoints) -> Self {
        value.0
    }
}

impl From<BasicPoints> for Uint128 {
    fn from(value: BasicPoints) -> Self {
        Uint128::from(u16::from(value))
    }
}

impl Mul<Uint128> for BasicPoints {
    type Output = Uint128;

    fn mul(self, rhs: Uint128) -> Self::Output {
        rhs.multiply_ratio(self.0, Self::MAX)
    }
}

impl Mul<Decimal> for BasicPoints {
    type Output = Decimal;

    fn mul(self, rhs: Decimal) -> Self::Output {
        Decimal::from_ratio(
            self.0 as u128 * rhs.numerator(),
            rhs.denominator() * Self::MAX as u128,
        )
    }
}

pub(crate) trait CheckedMulRatio {
    fn checked_multiply_ratio(
        self,
        numerator: impl Into<u128>,
        denominator: impl Into<Uint256>,
    ) -> StdResult<Uint128>;
}

impl CheckedMulRatio for Uint128 {
    fn checked_multiply_ratio(
        self,
        numerator: impl Into<u128>,
        denominator: impl Into<Uint256>,
    ) -> StdResult<Uint128> {
        let numerator = self.full_mul(numerator);
        let denominator = denominator.into();
        let mut result = numerator / denominator;
        let rem = numerator
            .checked_rem(denominator)
            .map_err(|_| StdError::generic_err("Division by zero"))?;
        // Rounding up if residual is more than 50% of denominator
        if rem.ge(&(denominator / Uint256::from(2u8))) {
            result += Uint256::from(1u128);
        }
        result
            .try_into()
            .map_err(|_| StdError::generic_err("Uint256 -> Uint128 conversion error"))
    }
}
