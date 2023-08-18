use astroport_governance::assembly::ProposalStatus;
use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// This enum describes Assembly contract errors
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Proposal not active!")]
    ProposalNotActive {},

    #[error("Voting period ended!")]
    VotingPeriodEnded {},

    #[error("User already voted!")]
    UserAlreadyVoted {},

    #[error("You don't have any voting power!")]
    NoVotingPower {},

    #[error("Voting period not ended yet!")]
    VotingPeriodNotEnded {},

    #[error("Proposal expired!")]
    ExecuteProposalExpired {},

    #[error("Insufficient token deposit!")]
    InsufficientDeposit {},

    #[error("Proposal not passed!")]
    ProposalNotPassed {},

    #[error("Proposal not completed!")]
    ProposalNotCompleted {},

    #[error("Proposal delay not ended!")]
    ProposalDelayNotEnded {},

    #[error("Proposal not in delay period!")]
    ProposalNotInDelayPeriod {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Whitelist cannot be empty!")]
    WhitelistEmpty {},

    #[error("Messages check passed. Nothing was committed to the blockchain")]
    MessagesCheckPassed {},

    #[error("IBC controller does not have channel {0}")]
    InvalidChannel(String),

    #[error("IBC controller is not set")]
    MissingIBCController {},

    #[error(
        "Failed to process callback from IBC controller as proposal {0} is not in \"{}\" state",
        ProposalStatus::InProgress
    )]
    WrongIbcProposalStatus(String),

    #[error("The IBC controller reports an invalid proposal status: {0}. Valid statuses: failed or executed ")]
    InvalidRemoteIbcProposalStatus(String),

    #[error("Sender is not an IBC controller installed in the assembly")]
    InvalidIBCController {},

    #[error("Sender is not the Generator controller installed in the assembly")]
    InvalidGeneratorController {},

    #[error("Sender is not the Hub installed in the assembly")]
    InvalidHub {},

    #[error("The proposal has no messages to execute")]
    InvalidProposalMessages {},

    #[error("Voting power exceeds maximum Outpost power")]
    InvalidVotingPower {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
