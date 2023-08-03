use crate::astroport::common::OwnershipProposal;
use crate::bps::BasicPoints;

use astroport_governance::generator_controller_lite::{
    ConfigResponse, GaugeInfoResponse, UserInfoResponse, VotedPoolInfoResponse,
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
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
    /// The period when the user voted last time, None if they've never voted
    pub vote_period: Option<u64>,
    /// The user's vxASTRO voting power
    pub voting_power: Uint128,
    /// The vote distribution for all the generators/pools the staker picked
    pub votes: Vec<(String, BasicPoints)>,
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
            vote_period: self.vote_period,
            voting_power: self.voting_power,
            votes,
        }
    }
}

/// Stores config at the given key.
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores voting parameters per pool at a specific period by key ( period -> pool_addr ).
pub const POOL_VOTES: Map<(u64, &str), VotedPoolInfo> = Map::new("pool_votes");

/// HashSet based on [`Map`]. It contains all pool addresses whose voting power > 0.
pub const POOLS: Map<&str, ()> = Map::new("pools");

/// Hashset based on [`Map`]. It stores null object by key ( pool_addr -> period ).
/// This hashset contains all periods which have saved result in [`POOL_VOTES`] for a specific pool address.
pub const POOL_PERIODS: Map<(&str, u64), ()> = Map::new("pool_periods");

/// User's voting information.
pub const USER_INFO: Map<&str, UserInfo> = Map::new("user_info");

/// Last tuning information.
pub const TUNE_INFO: Item<TuneInfo> = Item::new("tune_info");

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
