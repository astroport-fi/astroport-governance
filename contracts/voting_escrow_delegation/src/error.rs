use cosmwasm_std::StdError;
use cw_utils::ParseReplyError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("{0}")]
    ParseReplyError(#[from] ParseReplyError),

    #[error("You can't delegate with zero voting power")]
    ZeroVotingPower {},

    #[error("You have already delegated all the voting power.")]
    AllVotingPowerIsDelegated {},

    #[error("The delegation period must be at least a week and not more than a user lock period.")]
    DelegationPeriodError {},

    #[error("New expiration date must be greater than previously set and less than or equal to user's end of voting power lock.")]
    DelegationExtendPeriodError {},

    #[error("Not enough voting power to proceed")]
    NotEnoughVotingPower {},

    #[error("The percentage range must be from 0 to 100.")]
    PercentageError {},

    #[error("A delegation with a token {0} already exists.")]
    DelegationTokenAlreadyExists(String),

    #[error("New delegated voting power can not be less than it was previously.")]
    DelegationExtendVotingPowerError {},
}
