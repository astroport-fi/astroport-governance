use crate::error::ContractError;
use cosmwasm_std::{StdError, Uint128};
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
        let value = (numerator.u128() * Self::MAX as u128)
            .checked_div(denominator.u128())
            .ok_or_else(|| StdError::generic_err("Division by zero"))?;
        value.try_into()
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
