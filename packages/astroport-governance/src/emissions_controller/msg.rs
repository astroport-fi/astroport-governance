use std::collections::HashMap;
use std::fmt::Display;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Binary, Decimal, Uint128};

use crate::assembly::ProposalVoteOption;

#[cw_serde]
pub enum ExecuteMsg<T> {
    /// Vote allows a vxASTRO holders
    /// to cast votes on which pools should get ASTRO emissions in the next epoch
    Vote { votes: Vec<(String, Decimal)> },
    /// Only vxASTRO contract can call this endpoint.
    /// Updates user votes according to the current voting power.
    UpdateUserVotes { user: String, is_unlock: bool },
    /// Permissionless endpoint which allows user to update their
    /// voting power contribution in case of IBC failures or pool has been re-added to whitelist.
    RefreshUserVotes {},
    /// ProposeNewOwner proposes a new owner for the contract
    ProposeNewOwner {
        /// Newly proposed contract owner
        new_owner: String,
        /// The timestamp when the contract ownership change expires
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the latest contract ownership transfer proposal
    DropOwnershipProposal {},
    /// ClaimOwnership allows the newly proposed owner to claim contract ownership
    ClaimOwnership {},
    /// Set of endpoints specific for Hub/Outpost
    Custom(T),
}

/// This is a generic ICS acknowledgement format.
/// Proto defined here:
/// https://github.com/cosmos/cosmos-sdk/blob/v0.42.0/proto/ibc/core/channel/v1/channel.proto#L141-L147
/// This is compatible with the JSON serialization.
#[cw_serde]
pub enum IbcAckResult {
    Ok(Binary),
    Error(String),
}

/// Create a serialized error message
pub fn ack_fail(err: impl Display) -> Binary {
    to_json_binary(&IbcAckResult::Error(err.to_string())).unwrap()
}

/// Create a serialized success message
pub fn ack_ok() -> Binary {
    to_json_binary(&IbcAckResult::Ok(b"ok".into())).unwrap()
}

/// Internal IBC messages for hub and outposts interactions
#[cw_serde]
pub enum VxAstroIbcMsg {
    /// Sender: Outpost
    EmissionsVote {
        voter: String,
        /// Actual voting power reported from outpost
        voting_power: Uint128,
        /// Voting power distribution
        votes: HashMap<String, Decimal>,
    },
    /// Sender: Outpost
    UpdateUserVotes {
        voter: String,
        /// Actual voting power reported from outpost
        voting_power: Uint128,
        /// Marker defines whether this packet was sent from vxASTRO unlock context
        is_unlock: bool,
    },
    /// Sender: Hub
    RegisterProposal { proposal_id: u64, start_time: u64 },
    /// Sender: Outpost
    GovernanceVote {
        voter: String,
        /// Actual voting power reported from outpost
        voting_power: Uint128,
        /// Proposal id
        proposal_id: u64,
        /// Vote option
        vote: ProposalVoteOption,
    },
}
