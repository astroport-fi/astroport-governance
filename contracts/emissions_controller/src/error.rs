use cosmwasm_std::{CheckedFromRatioError, Coin, StdError};
use cw_utils::{ParseReplyError, PaymentError};
use neutron_sdk::NeutronError;
use thiserror::Error;

/// This enum describes contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("{0}")]
    NeutronError(#[from] NeutronError),

    #[error("{0}")]
    ParseReplyError(#[from] ParseReplyError),

    #[error("{0}")]
    CheckedFromRatioError(#[from] CheckedFromRatioError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("You can't vote with zero voting power")]
    ZeroVotingPower {},

    #[error("Next time you can change your vote is at {0}")]
    VoteCooldown(u64),

    #[error("Next tuning will be available at {0}")]
    TuneCooldown(u64),

    #[error("Pool {0} is not whitelisted")]
    PoolIsNotWhitelisted(String),

    #[error("Incorrect whitelist fee. Expected {0}")]
    IncorrectWhitelistFee(Coin),

    #[error("Pool {0} is already whitelisted")]
    PoolAlreadyWhitelisted(String),

    #[error("Invalid total votes weight. Must be 1")]
    InvalidTotalWeight {},

    #[error("Failed to parse reply")]
    FailedToParseReply {},

    #[error("Invalid outpost prefix for {0}")]
    InvalidOutpostPrefix(String),

    #[error("Invalid ASTRO denom for outpost. Must start with ibc/")]
    InvalidOutpostAstroDenom {},

    #[error("Invalid ASTRO denom on the Hub. Must be {0}")]
    InvalidHubAstroDenom(String),

    #[error("Invalid ics20 channel. Must start with channel-")]
    InvalidOutpostIcs20Channel {},

    #[error("Failed to determine outpost for pool {0}")]
    NoOutpostForPool(String),

    #[error("Message contains duplicated pools")]
    DuplicatedVotes {},

    #[error("Astro pool can't be whitelisted")]
    IsAstroPool {},

    #[error("No failed outposts to retry")]
    NoFailedOutpostsToRetry {},

    #[error("Can't set zero emissions for astro pool")]
    ZeroAstroEmissions {},
}
