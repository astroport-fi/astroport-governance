use cosmwasm_std::{OverflowError, StdError};
use cw20_base::ContractError as cw20baseError;
use cw_utils::PaymentError;
use thiserror::Error;

/// This enum describes vxASTRO contract errors
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Cw20Base(#[from] cw20baseError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Lock already exists, either unlock and withdraw or extend_lock to add to the lock")]
    LockAlreadyExists {},

    #[error("Lock does not exist")]
    LockDoesNotExist {},

    #[error("The lock time has not yet expired")]
    LockHasNotExpired {},

    #[error("The lock expired. Withdraw and create new lock")]
    LockExpired {},

    #[error("The {0} address is blacklisted")]
    AddressBlacklisted(String),

    #[error("Marketing info validation error: {0}")]
    MarketingInfoValidationError(String),

    #[error("Already unlocking")]
    Unlocking {},

    #[error("The lock has not been unlocked, call unlock first")]
    NotUnlocked {},
}
