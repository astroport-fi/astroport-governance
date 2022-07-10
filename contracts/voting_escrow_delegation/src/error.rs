use cosmwasm_std::StdError;
use cw_utils::ParseReplyError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Custom Error val: {val:?}")]
    CustomError { val: String },

    #[error("{0}")]
    ParseReplyError(#[from] ParseReplyError),

    #[error("You can't delegate with zero voting power")]
    ZeroVotingPower {},

    #[error("NFT delegation already exists")]
    NFTDelegationAlreadyExists {},

    #[error("The delegation period must be at least a week and not more than a user lock period.")]
    DelegationPeriodError {},

    #[error("The percentage range must be from 0 to 100.")]
    PercentageError {},

    #[error("Cancel time cannot be greater then expire time.")]
    CancelTimeWrong {},
}
