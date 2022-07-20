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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Token {
    pub bias: Uint128,
    pub slope: Uint128,
    pub start: u64,
    pub expire_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Point {
    pub bias: Uint128,
    pub slope: Uint128,
}

/// ## Description
/// Stores all user lock history
pub const DELEGATED: SnapshotMap<(Addr, String), Token> = SnapshotMap::new(
    "delegated",
    "delegated__checkpoints",
    "delegated__changelog",
    Strategy::EveryBlock,
);

/// ## Description
/// Stores all user lock history
pub const RECEIVED: SnapshotMap<(Addr, String), Token> = SnapshotMap::new(
    "received",
    "received__checkpoints",
    "received__changelog",
    Strategy::EveryBlock,
);

pub const DELEGATION_MAX_PERCENT: Uint128 = Uint128::new(100);
pub const DELEGATION_MIN_PERCENT: Uint128 = Uint128::new(1);
