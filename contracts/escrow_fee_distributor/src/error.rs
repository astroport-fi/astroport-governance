use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// ## Description
/// This enum describes staking contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Checkpoint token is not available!")]
    CheckpointTokenIsNotAvailable {},

    #[error("Amount is not available!")]
    AmountIsNotAvailable {},

    #[error("Token address is wrong!")]
    TokenAddressIsWrong {},

    #[error("Contract is killed!")]
    ContractIsKilled {},

    #[error("Exceeded account limit for claim operation!")]
    ExceededAccountLimitOfClaim {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
