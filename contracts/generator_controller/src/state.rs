use crate::bps::BasicPoints;

use astroport_governance::generator_controller::{
    ConfigResponse, GaugeInfoResponse, UserInfoResponse, VotedPoolInfoResponse,
};
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type Config = ConfigResponse;
pub type VotedPoolInfo = VotedPoolInfoResponse;
pub type GaugeInfo = GaugeInfoResponse;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfo {
    pub vote_ts: u64,
    pub voting_power: Uint128,
    pub slope: Decimal,
    pub lock_end: u64,
    pub votes: Vec<(Addr, BasicPoints)>,
}

impl UserInfo {
    pub(crate) fn into_response(self) -> UserInfoResponse {
        let votes = self
            .votes
            .iter()
            .map(|(pool_addr, bps)| (pool_addr.clone(), u16::from(*bps)))
            .collect();

        UserInfoResponse {
            vote_ts: self.vote_ts,
            voting_power: self.voting_power,
            slope: self.slope,
            lock_end: self.lock_end,
            votes,
        }
    }
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ( period -> pool_addr )
pub const POOL_VOTES: Map<(U64Key, &Addr), VotedPoolInfo> = Map::new("pool_votes");

/// HashSet with pool addrs based on cw Map
pub const POOLS: Map<&Addr, ()> = Map::new("pools");

/// ( period -> pool_addr )
pub const POOL_PERIODS: Map<(&Addr, U64Key), ()> = Map::new("pool_periods");

/// ( pool_addr -> period )
pub const POOL_SLOPE_CHANGES: Map<(&Addr, U64Key), Decimal> = Map::new("pool_slope_changes");

pub const USER_INFO: Map<&Addr, UserInfo> = Map::new("user_info");

pub const GAUGE_INFO: Item<GaugeInfo> = Item::new("gauge_info");
