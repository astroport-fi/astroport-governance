use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128, Uint64};
use std::fmt::{Display, Formatter, Result};

use crate::assembly::ProposalVoteOption;

// Minimum IBC timeout is 5 seconds
pub const MIN_IBC_TIMEOUT_SECONDS: u64 = 5;
// Maximum IBC timeout is 1 hour
pub const MAX_IBC_TIMEOUT_SECONDS: u64 = 60 * 60;

/// Hub defines the messages that can be sent from an Outpost to the Hub
#[cw_serde]
#[non_exhaustive]
pub enum Hub {
    /// Queries the Assembly for a proposal by ID via the Hub
    QueryProposal {
        /// The ID of the proposal to query
        id: u64,
    },
    /// Cast a vote on an Assembly proposal
    CastAssemblyVote {
        /// The ID of the proposal to vote on
        proposal_id: u64,
        /// The address of the voter
        voter: Addr,
        /// The vote choice
        vote_option: ProposalVoteOption,
        /// The voting power held by the voter, in this case xASTRO holdings
        voting_power: Uint128,
    },
    /// Cast a vote during an emissions voting period
    CastEmissionsVote {
        /// The address of the voter
        voter: Addr,
        /// The voting power held by the voter, in this case vxASTRO  lite holdings
        voting_power: Uint128,
        /// The votes in the format (pool address, percent of voting power)
        votes: Vec<(String, u16)>,
    },
    /// Stake ASTRO tokens for xASTRO
    Stake {},
    /// Unstake xASTRO tokens for ASTRO
    Unstake {
        // The user requesting the unstake and that should receive it
        receiver: String,
        /// The amount of xASTRO to unstake
        amount: Uint128,
    },
    /// Kick an unlocked voter's voting power from the Generator Controller lite
    KickUnlockedVoter {
        /// The address of the voter to kick
        voter: Addr,
    },
    /// Kick a blacklisted voter's voting power from the Generator Controller lite
    KickBlacklistedVoter {
        /// The address of the voter that has been blacklisted
        voter: Addr,
    },
    /// Withdraw stuck funds from the Hub in case of specific IBC failures
    WithdrawFunds {
        /// The address of the user to withdraw funds for
        user: Addr,
    },
}

/// Defines the messages that can be sent from the Hub to an Outpost
#[cw_serde]
pub enum Outpost {
    /// Mint xASTRO tokens for the user
    MintXAstro { receiver: String, amount: Uint128 },
}

/// Defines a minimal proposal that is cached on the Outpost
#[cw_serde]
pub struct ProposalSnapshot {
    /// Unique proposal ID
    pub id: Uint64,
    /// Start time of proposal
    pub start_time: u64,
}

/// Defines the messages that can be returned in response to an IBC Hub or
/// Outpost message
#[cw_serde]
pub enum Response {
    /// The response to a QueryProposal message that includes a minimal Proposal
    QueryProposal(ProposalSnapshot),
    /// A generic response to a Hub/Outpost message, mostly used for indicating success
    /// or error handling
    Result {
        /// The action that was performed, None if no specific action was taken
        action: Option<String>,
        /// The address of the user that took the action, None if the result
        /// isn't specific to an address
        address: Option<String>,
        /// The error message, if None, the action was successful
        error: Option<String>,
    },
}

/// Utility functions for InterchainResponse to ease creation of responses
impl Response {
    /// Create a new success response that sets address and action but leaves
    /// error as None
    pub fn new_success(action: String, address: String) -> Self {
        Response::Result {
            action: Some(action),
            address: Some(address),
            error: None,
        }
    }
    /// Create a new error response that sets address and action to None
    /// while adding the error message
    pub fn new_error(error: String) -> Self {
        Response::Result {
            action: None,
            address: None,
            error: Some(error),
        }
    }
}

/// Implements Display for Hub
impl Display for Hub {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "{}",
            match self {
                Hub::Stake { .. } => "stake",
                Hub::CastAssemblyVote { .. } => "cast_assembly_vote",
                Hub::CastEmissionsVote { .. } => "cast_emissions_vote",
                Hub::QueryProposal { .. } => "query_proposal",
                Hub::Unstake { .. } => "unstake",
                Hub::KickUnlockedVoter { .. } => "kick_unlocked_voter",
                Hub::KickBlacklistedVoter { .. } => "kick_blacklisted_voter",
                Hub::WithdrawFunds { .. } => "withdraw_funds",
            }
        )
    }
}

/// Implements Display for Outpost
impl Display for Outpost {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "{}",
            match self {
                Outpost::MintXAstro { .. } => "MintXAstro",
            }
        )
    }
}

/// Get the address from an IBC port. If the port is prefixed with `wasm.`,
/// strip it out, if not, return the port as is.
pub fn get_contract_from_ibc_port(ibc_port: &str) -> &str {
    match ibc_port.strip_prefix("wasm.") {
        Some(suffix) => suffix, // prints: inj1234
        None => ibc_port,
    }
}
