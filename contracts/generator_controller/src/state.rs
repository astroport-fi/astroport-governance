use crate::astroport::common::OwnershipProposal;
use crate::bps::BasicPoints;

use astroport_governance::generator_controller::{
    ConfigResponse, GaugeInfoResponse, UserInfoResponse, VotedPoolInfoResponse,
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

/// This structure describes the main control config of generator controller contract.
pub type Config = ConfigResponse;
/// This structure describes voting parameters for a specific pool.
pub type VotedPoolInfo = VotedPoolInfoResponse;
/// This structure describes last tuning parameters.
pub type TuneInfo = GaugeInfoResponse;

/// The struct describes last user's votes parameters.
#[cw_serde]
#[derive(Default)]
pub struct UserInfo {
    pub vote_ts: u64,
    pub voting_power: Uint128,
    pub slope: Uint128,
    pub lock_end: u64,
    pub votes: Vec<(Addr, BasicPoints)>,
}

impl UserInfo {
    /// The function converts [`UserInfo`] object into [`UserInfoResponse`].
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

/// Stores config at the given key.
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores voting parameters per pool at a specific period by key ( period -> pool_addr ).
pub const POOL_VOTES: Map<(u64, &Addr), VotedPoolInfo> = Map::new("pool_votes");

/// HashSet based on [`Map`]. It contains all pool addresses whose voting power > 0.
pub const POOLS: Map<&Addr, ()> = Map::new("pools");

/// Hashset based on [`Map`]. It stores null object by key ( pool_addr -> period ).
/// This hashset contains all periods which have saved result in [`POOL_VOTES`] for a specific pool address.
pub const POOL_PERIODS: Map<(&Addr, u64), ()> = Map::new("pool_periods");

/// Slope changes for a specific pool address by key ( pool_addr -> period ).
pub const POOL_SLOPE_CHANGES: Map<(&Addr, u64), Uint128> = Map::new("pool_slope_changes");

/// User's voting information.
pub const USER_INFO: Map<&Addr, UserInfo> = Map::new("user_info");

/// Last tuning information.
pub const TUNE_INFO: Item<TuneInfo> = Item::new("tune_info");

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
