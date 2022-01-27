use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::{Add, Sub};

/// ## Description
/// This structure describes the main control config of maker.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// the xASTRO token contract address
    pub xastro_token_addr: Addr,
}

const MULTIPLIER: f32 = 1000000_f32;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Slope(pub u64);

impl From<Slope> for f32 {
    fn from(Slope(value): Slope) -> Self {
        value as f32 / MULTIPLIER as f32
    }
}

impl From<f32> for Slope {
    fn from(value: f32) -> Self {
        let converted = value * MULTIPLIER as f32;
        Self(converted as u64)
    }
}

impl Sub for Slope {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Slope(self.0.sub(rhs.0))
    }
}

impl Add for Slope {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Slope(self.0.add(rhs.0))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Point {
    pub power: Uint128,
    pub start: u64,
    pub end: u64,
    pub slope: Slope,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Lock {
    pub power: Uint128,
    pub end: u64,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

pub const LOCKED: Map<Addr, Lock> = Map::new("locked");

pub const HISTORY: Map<(Addr, U64Key), Point> = Map::new("history");

pub const SLOPE_CHANGES: Map<U64Key, Slope> = Map::new("slope_changes");
