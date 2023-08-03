use cosmwasm_std::{OverflowError, StdError};
use cw20_base::ContractError as cw20baseError;
use thiserror::Error;

/// This enum describes vxASTRO contract errors
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Cw20Base(#[from] cw20baseError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Lock already exists, either unlock and withdraw or extend_lock to add to the lock")]
    LockAlreadyExists {},

    #[error("Lock does not exist")]
    LockDoesNotExist {},

    #[error("Lock time must be within limits (week <= lock time < 2 years)")]
    LockTimeLimitsError {},

    #[error("The lock time has not yet expired")]
    LockHasNotExpired {},

    #[error("The lock expired. Withdraw and create new lock")]
    LockExpired {},

    #[error("The {0} address is blacklisted")]
    AddressBlacklisted(String),

    #[error("Marketing info validation error: {0}")]
    MarketingInfoValidationError(String),

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Already unlocking")]
    Unlocking {},

    #[error("The lock has not been unlocked, call unlock first")]
    NotUnlocked,
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
