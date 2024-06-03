use cosmwasm_std::{OverflowError, StdError, Uint128};
use cw_utils::{ParseReplyError, PaymentError};
use thiserror::Error;

use astroport_governance::emissions_controller::consts::MAX_POOLS_TO_VOTE;

/// This enum describes contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    ParseReplyError(#[from] ParseReplyError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("You can't vote with zero voting power")]
    ZeroVotingPower {},

    #[error("Invalid total votes weight. Must be 1.")]
    InvalidTotalWeight {},

    #[error("Failed to parse reply")]
    FailedToParseReply {},

    #[error("You can vote maximum for {MAX_POOLS_TO_VOTE} pools")]
    ExceededMaxPoolsToVote {},

    #[error("User {0} has pending IBC transaction. Wait until it is resolved by relayer")]
    PendingUser(String),

    #[error("Message contains duplicated pools")]
    DuplicatedVotes {},

    #[error("Invalid astro amount. Expected: {expected}, actual: {actual}")]
    InvalidAstroAmount { expected: Uint128, actual: Uint128 },

    #[error("No valid schedules found")]
    NoValidSchedules {},
}
