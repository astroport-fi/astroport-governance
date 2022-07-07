use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, SnapshotMap, Strategy};

/// This structure stores the main parameters for the voting escrow delegation contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// Astroport NFT contract address
    pub nft_token_addr: Addr,
    /// vxASTRO contract address
    pub voting_escrow_addr: Addr,
}

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// This structure stores points along the checkpoint history for every vxASTRO staker.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Point {
    /// The staker's vxASTRO voting power
    pub power: Uint128,
    /// The start period when the staker's voting power start to decrease
    pub start: u64,
    /// The period when the lock should expire
    pub end: u64,
    /// Weekly voting power decay
    pub slope: Uint128,
}

/// The struct describes last user's votes parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Default)]
pub struct DelegateInfo {
    pub nft_token_code_id: u64,
    pub voting_power: Uint128,
}

/// ## Description
/// Stores all user lock history
pub const LOCKED: SnapshotMap<Addr, Point> = SnapshotMap::new(
    "locked",
    "locked__checkpoints",
    "locked__changelog",
    Strategy::EveryBlock,
);
