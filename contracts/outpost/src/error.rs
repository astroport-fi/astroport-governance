use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// This enum describes bribes contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("You can not send 0 tokens")]
    ZeroAmount {},

    #[error(
        "Proposal {0} is being queried from the Hub, please try again in a few minutes",
        proposal_id
    )]
    PendingVoteExists { proposal_id: u64 },

    #[error(
        "The address has no voting power at the start of the proposal: {0}",
        address
    )]
    NoVotingPower { address: String },

    #[error("The IBC channel to the Hub has not been set")]
    MissingHubChannel {},

    #[error("The user has already voted on this proposal")]
    AlreadyVoted {},

    #[error("Channel already established: {channel_id}")]
    ChannelAlreadyEstablished { channel_id: String },

    #[error("Invalid source port {invalid}. Should be : {valid}")]
    InvalidSourcePort { invalid: String, valid: String },

    #[error("Invalid IBC timeout: {timeout}, must be between {min} and {max} seconds")]
    InvalidIBCTimeout { timeout: u64, min: u64, max: u64 },
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
