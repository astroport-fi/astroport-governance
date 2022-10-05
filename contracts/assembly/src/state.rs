use astroport_governance::assembly::{Config, Proposal};
use cosmwasm_std::Uint64;
use cw_storage_plus::{Item, Map};

/// ## Description
/// Stores the config for the Assembly contract
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores the global state for the Assembly contract
pub const PROPOSAL_COUNT: Item<Uint64> = Item::new("proposal_count");

/// ## Description
/// This is a map that contains information about all proposals
pub const PROPOSALS: Map<u64, Proposal> = Map::new("proposals");
