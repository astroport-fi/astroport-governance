use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

use astroport::common::OwnershipProposal;
use astroport_governance::{
    assembly::ProposalVoteOption, interchain::ProposalSnapshot, outpost::Config,
};

#[cw_serde]
pub struct PendingVote {
    /// The proposal ID to vote on
    pub proposal_id: u64,
    /// The user voting
    pub voter: Addr,
    /// The choice in vote
    pub vote_option: ProposalVoteOption,
}

/// Store the contract config
pub const CONFIG: Item<Config> = Item::new("config");

/// Store a local cache of proposals to verify votes are allowed
pub const PROPOSALS_CACHE: Map<u64, ProposalSnapshot> = Map::new("proposals_cache");

/// Store the pending votes for a proposal while the information is being
/// retrieved from the Hub
pub const PENDING_VOTES: Map<u64, PendingVote> = Map::new("pending_votes");

/// Record of who has voted on which governance proposal
pub const VOTES: Map<(&Addr, u64), ProposalVoteOption> = Map::new("votes");

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
