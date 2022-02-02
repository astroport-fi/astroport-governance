use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of maker.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// the xASTRO token contract address
    pub xastro_token_addr: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Point {
    pub power: Uint128,
    pub start: u64,
    pub end: u64,
    pub slope: Decimal,
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

pub const HISTORY: Map<(Addr, U64Key), Point> = Map::new("history");

pub const SLOPE_CHANGES: Map<U64Key, Decimal> = Map::new("slope_changes");
