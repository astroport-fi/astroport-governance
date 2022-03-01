use cosmwasm_std::StdError;
use thiserror::Error;

/// ## Description
/// This enum describes contract errors
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Basic points conversion error. {0} > 10000")]
    BPSConverstionError(u128),

    #[error("Basic points sum exceeds limit")]
    BPSLimitError {},

    #[error("Pool not found")]
    PoolNotFound {},

    #[error("You can't vote with zero voting power")]
    ZeroVotingPower {},

    #[error("You can only run this action every {0} days")]
    CooldownError(u64),

    #[error("Votes contain duplicated pool addresses")]
    DuplicatedPools {},

    #[error("Your lock will expire in less than a week")]
    LockExpiresSoon {},
}
