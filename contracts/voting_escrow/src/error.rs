use cosmwasm_std::{OverflowError, StdError};
use cw20_base::ContractError as CW20Error;
use cw_utils::PaymentError;
use thiserror::Error;

/// This enum describes vxASTRO contract errors
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    Cw20Base(#[from] CW20Error),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Marketing info validation error: {0}")]
    MarketingInfoValidationError(String),

    #[error("No withdrawal balance available")]
    ZeroBalance {},

    #[error("Unlock period not expired. Expected: at {0}")]
    UnlockPeriodNotExpired(u64),

    #[error("Position is not in unlocking state")]
    NotInUnlockingState {},

    #[error("Position is already unlocking. Consider relocking to lock more tokens")]
    PositionUnlocking {},
}
