use cosmwasm_std::{OverflowError, StdError};
use cw2::VersionError;
use cw_utils::PaymentError;
use thiserror::Error;

use astroport_governance::assembly::ProposalStatus;

/// This enum describes Assembly contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    VersionError(#[from] VersionError),

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

    #[error("Insufficient token deposit!")]
    InsufficientDeposit {},

    #[error("Proposal not passed!")]
    ProposalNotPassed {},

    #[error("Proposal delay not ended!")]
    ProposalDelayNotEnded {},

    #[error("Whitelist cannot be empty!")]
    WhitelistEmpty {},

    #[error("Messages check passed. Nothing was committed to the blockchain")]
    MessagesCheckPassed {},

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

    #[error("{0}")]
    PaymentError(#[from] PaymentError),
}
