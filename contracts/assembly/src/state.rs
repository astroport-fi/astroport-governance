use astroport_governance::assembly::{Config, Proposal, ProposalVote};
use cosmwasm_std::{Addr, Uint64};
use cw_storage_plus::{Item, Map, U64Key};

/// ## Description
/// Stores config of assembly contract
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores global state of assembly contract
pub const PROPOSAL_COUNT: Item<Uint64> = Item::new("proposal_count");

/// ## Description
/// This is a map that contains information about all proposals.
pub const PROPOSALS: Map<U64Key, Proposal> = Map::new("proposals");

/// ## Description
/// This is a map that contains information about all proposal votes.
pub const PROPOSAL_VOTES: Map<(U64Key, &Addr), ProposalVote> = Map::new("proposal_votes");
