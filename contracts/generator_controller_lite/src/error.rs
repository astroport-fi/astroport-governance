use cosmwasm_std::StdError;
use thiserror::Error;

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

    #[error("You can't vote with zero voting power")]
    ZeroVotingPower {},

    #[error("{0} is the main pool. Voting or whitelisting the main pool is prohibited.")]
    MainPoolVoteOrWhitelistingProhibited(String),

    #[error("main_pool_min_alloc should be more than 0 and less than 1")]
    MainPoolMinAllocFailed {},

    #[error("You can only run this action once in a voting period")]
    CooldownError {},

    #[error("Invalid lp token address: {0}")]
    InvalidLPTokenAddress(String),

    #[error("Votes contain duplicated pool addresses")]
    DuplicatedPools {},

    #[error("There are no pools to tune")]
    TuneNoPools {},

    #[error("Invalid pool number: {0}. Must be within [2, 100] range")]
    InvalidPoolNumber(u64),

    #[error("The vector contains duplicated addresses")]
    DuplicatedVoters {},

    #[error("Exceeded voters limit for kick blacklisted/unlocked voters operation!")]
    KickVotersLimitExceeded {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Whitelist cannot be empty!")]
    WhitelistEmpty {},

    #[error("The pair aren't registered: {0}-{1}")]
    PairNotRegistered(String, String),

    #[error("Pool is already whitelisted: {0}")]
    PoolIsWhitelisted(String),

    #[error("Pool is not whitelisted: {0}")]
    PoolIsNotWhitelisted(String),

    #[error("Address is still locked: {0}")]
    AddressIsLocked(String),

    #[error("Sender is not the Hub installed")]
    InvalidHub {},
}
