use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};
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
pub struct Point {
    bias: Uint128,
    slope: Uint128,
    timestamp: Timestamp,
    block: u64,
}

impl Point {
    pub fn new(timestamp: Timestamp, block: u64) -> Self {
        Self {
            bias: Uint128::zero(),
            slope: Uint128::zero(),
            timestamp,
            block,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Lock {
    pub amount: Uint128,
    pub final_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct History {
    pub main_points: Vec<Point>,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

pub const LOCKED: Map<Addr, Lock> = Map::new("locked");

pub const HISTORY: Item<History> = Item::new("history");
