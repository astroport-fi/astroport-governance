use crate::bps::BasicPoints;

use cosmwasm_std::{Addr, Decimal, Uint128, Uint64};
use cw_storage_plus::{Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of voting escrow contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// contract address that used for settings control
    pub owner: Addr,
    /// the vxASTRO token contract address
    pub escrow_addr: Addr,
    /// generator contract address
    pub generator_addr: Addr,
    /// a max number of generators that can receive an ASTRO allocation
    pub pools_limit: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct VotedPoolInfo {
    pub vxastro_amount: Uint128,
    pub slope: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfo {
    pub vote_ts: u64,
    pub voting_power: Uint128,
    pub slope: Decimal,
    pub votes: Vec<(Addr, BasicPoints)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct GaugeInfo {
    pub gauge_ts: u64,
    pub pool_alloc_points: Vec<(Addr, Uint64)>,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ( period -> pool_addr )
pub const POOL_VOTES: Map<(U64Key, &Addr), VotedPoolInfo> = Map::new("pool_votes");

pub const USER_INFO: Map<&Addr, UserInfo> = Map::new("user_info");

pub const GAUGE_INFO: Item<GaugeInfo> = Item::new("gauge_info");
