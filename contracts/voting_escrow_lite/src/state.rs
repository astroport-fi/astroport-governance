use crate::astroport::common::OwnershipProposal;
use astroport_governance::voting_escrow_lite::Config;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map, SnapshotMap, Strategy};

/// This structure stores data about the lockup position for a specific vxASTRO staker.
#[cw_serde]
pub struct Lock {
    /// The total amount of xASTRO tokens that were deposited in the vxASTRO position
    pub amount: Uint128,
    /// The timestamp when a lock will be unlocked. None for positions in Locked state
    pub end: Option<u64>,
}

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores all user lock history by timestamp
pub const LOCKED: SnapshotMap<Addr, Lock> = SnapshotMap::new(
    "locked_timestamp",
    "locked_timestamp__checkpoints",
    "locked_timestamp__changelog",
    Strategy::EveryBlock,
);

/// Stores the voting power history for every staker (addr => timestamp)
/// Total voting power checkpoints are stored using a (contract_addr => timestamp) key
pub const VOTING_POWER_HISTORY: Map<(Addr, u64), Uint128> = Map::new("voting_power_history");

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Contains blacklisted staker addresses
pub const BLACKLIST: Item<Vec<Addr>> = Item::new("blacklist");
