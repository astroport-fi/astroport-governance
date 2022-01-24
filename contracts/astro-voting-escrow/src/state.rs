use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of maker.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub period: u64,
    /// the xASTRO token contract address
    pub xastro_token_addr: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Lock {
    pub amount: Uint128,
    pub start: u64,
    pub end: u64,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

pub const LOCKED: Map<Addr, Lock> = Map::new("locked");

pub const HISTORY: Map<(Addr, U64Key), Lock> = Map::new("history");
