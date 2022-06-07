use crate::error::ContractError;
use cosmwasm_std::{Decimal, Fraction, StdError, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::ops::Mul;

/// ## Description
/// BasicPoints struct implementation. BasicPoints value is within [0, 10000] interval.
/// Technically BasicPoints is wrapper over [`u16`] with additional limit checks and
/// several implementations of math functions so BasicPoints object
/// can be used in formulas along with [`Uint128`] and [`Decimal`].
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
            .checked_multiply_ratio(Self::MAX, denominator)
            .map_err(|_| StdError::generic_err("Checked multiple ratio error!"))?
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
            rhs.numerator() * Uint128::from(self.0),
            rhs.denominator() * Uint128::from(Self::MAX),
        )
    }
}
