use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport_governance::U64Key;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map, SnapshotMap, Strategy};

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
pub struct DelegateVP {
    pub delegated: Uint128,
    pub received: Uint128,
    pub delegated_expire_time: u64,
    pub received_expire_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Token {
    pub bias: Uint128,
    pub slope: Uint128,
    pub percentage: Uint128,
    pub expire_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Point {
    pub bias: Uint128,
    pub slope: Uint128,
}

pub const TOKENS: Map<String, Token> = Map::new("tokens");

/// ## Description
/// Stores all user lock history
pub const DELEGATED: SnapshotMap<Addr, DelegateVP> = SnapshotMap::new(
    "delegate",
    "delegate__checkpoints",
    "delegate__changelog",
    Strategy::EveryBlock,
);
