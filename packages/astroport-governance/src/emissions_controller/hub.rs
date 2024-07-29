use std::collections::HashMap;

use astroport::asset::validate_native_denom;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{ensure, Addr, Coin, Decimal, StdError, StdResult, Uint128};

use crate::emissions_controller::consts::POOL_NUMBER_LIMIT;
use crate::voting_escrow::UpdateMarketingInfo;

/// This structure describes the basic settings for creating a contract.
#[cw_serde]
pub struct HubInstantiateMsg {
    /// Contract owner
    pub owner: String,
    /// Astroport Assembly contract address
    pub assembly: String,
    /// vxASTRO contract code id
    pub vxastro_code_id: u64,
    /// vxASTRO token marketing info
    pub vxastro_marketing_info: UpdateMarketingInfo,
    /// xASTRO denom
    pub xastro_denom: String,
    /// Astroport Factory contract
    pub factory: String,
    /// ASTRO denom on the Hub
    pub astro_denom: String,
    /// Max number of pools that can receive ASTRO emissions per outpost added.
    /// For example, if there are 3 outposts,
    /// and the pools_limit is 10, then 30 pools can receive ASTRO emissions.
    /// This limit doesn't enforce the exact number of pools per outpost,
    /// but adds flexibility to the contract
    /// to automatically adjust the max number of pools based on the number of outposts.
    pub pools_per_outpost: u64,
    /// Fee required to whitelist a pool
    pub whitelisting_fee: Coin,
    /// Address that receives the whitelisting fee
    pub fee_receiver: String,
    /// Minimal percentage of total voting power required to keep a pool in the whitelist
    pub whitelist_threshold: Decimal,
    /// Controls ASTRO emissions for the next epoch.
    /// If multiple < 1 then protocol emits less ASTRO than it buys back,
    /// otherwise protocol is inflating ASTRO supply.
    pub emissions_multiple: Decimal,
    /// Max ASTRO allowed per epoch. Parameter of the dynamic emissions curve.
    pub max_astro: Uint128,
    /// Defines the number of ASTRO collected to staking contract
    /// from 2-weeks period preceding the current epoch.
    pub collected_astro: Uint128,
    /// EMA of the collected ASTRO from the previous epoch
    pub ema: Uint128,
}

#[cw_serde]
pub enum HubMsg {
    /// TunePools transforms the latest vote distribution into ASTRO emissions
    TunePools {},
    /// Repeats IBC transfer messages with IBC hook for all outposts in Failed state.
    RetryFailedOutposts {},
    /// Update the contract configuration
    UpdateConfig {
        pools_per_outpost: Option<u64>,
        whitelisting_fee: Option<Coin>,
        fee_receiver: Option<String>,
        emissions_multiple: Option<Decimal>,
        max_astro: Option<Uint128>,
    },
    /// Whitelists a pool to receive ASTRO emissions. Requires fee payment
    WhitelistPool { lp_token: String },
    /// Register or update an outpost
    UpdateOutpost {
        /// Bech32 prefix
        prefix: String,
        /// Astro denom on this outpost
        astro_denom: String,
        /// Outpost params contain all necessary information to interact with the remote outpost.
        /// This field also serves as marker whether it is The hub (params: None) or
        /// remote outpost (Some(params))
        outpost_params: Option<OutpostParams>,
        /// A pool that must receive flat ASTRO emissions. Optional.
        astro_pool_config: Option<AstroPoolConfig>,
    },
    /// Remove an outpost
    RemoveOutpost { prefix: String },
    /// Permissionless endpoint to stream proposal info from the Hub to all outposts
    RegisterProposal { proposal_id: u64 },
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// UserInfo returns information about a voter and the pools they voted for.
    /// If timestamp is not provided, the current block time is used.
    #[returns(UserInfoResponse)]
    UserInfo {
        user: String,
        timestamp: Option<u64>,
    },
    /// TuneInfo returns emissions voting outcome at a certain timestamp.
    /// If timestamp is not provided, return the latest tune info.
    #[returns(TuneInfo)]
    TuneInfo { timestamp: Option<u64> },
    /// Config returns the contract configuration
    #[returns(Config)]
    Config {},
    /// VotedPools returns how much voting power a pool received at a certain timestamp.
    #[returns(VotedPoolInfo)]
    VotedPool {
        pool: String,
        timestamp: Option<u64>,
    },
    /// Returns paginated list of all pools that received votes at the current epoch
    #[returns(Vec<(String, VotedPoolInfo)>)]
    VotedPools {
        limit: Option<u8>,
        start_after: Option<String>,
    },
    /// ListOutposts returns all outposts registered in the contract
    #[returns(Vec<(String, OutpostInfo)>)]
    ListOutposts {},
    /// QueryWhitelist returns the list of pools that are allowed to be voted for.
    /// The query is paginated.
    /// If 'start_after' is provided, it yields a list **excluding** 'start_after'.
    #[returns(Vec<String>)]
    QueryWhitelist {
        limit: Option<u8>,
        start_after: Option<String>,
    },
    /// SimulateTune simulates the ASTRO amount that will be emitted in the next epoch per pool
    /// considering if the next epoch starts right now.
    /// This query is useful for the UI to show the expected ASTRO emissions
    /// as well as might be useful for integrator estimations.
    /// It filters out pools which don't belong to any of outposts and invalid Hub-based LP tokens.
    /// Returns TuneResultResponse object which contains
    /// emissions state and next pools grouped by outpost prefix.
    #[returns(SimulateTuneResponse)]
    SimulateTune {},
}

/// General contract configuration
#[cw_serde]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// Astroport Assembly contract address
    pub assembly: Addr,
    /// vxASTRO contract address
    pub vxastro: Addr,
    /// Astroport Factory contract
    pub factory: Addr,
    /// ASTRO denom on the Hub
    pub astro_denom: String,
    /// xASTRO denom
    pub xastro_denom: String,
    /// Staking contract
    pub staking: Addr,
    /// The Astroport Incentives contract
    pub incentives_addr: Addr,
    /// Max number of pools that can receive ASTRO emissions per outpost added.
    /// For example, if there are 3 outposts,
    /// and the pools_limit is 10, then 30 pools can receive ASTRO emissions.
    /// This limit doesn't enforce the exact number of pools per outpost,
    /// but adds flexibility to the contract
    /// to automatically adjust the max number of pools based on the number of outposts.
    pub pools_per_outpost: u64,
    /// Fee required to whitelist a pool
    pub whitelisting_fee: Coin,
    /// Address that receives the whitelisting fee
    pub fee_receiver: Addr,
    /// Minimal percentage of total voting power required to keep a pool in the whitelist
    pub whitelist_threshold: Decimal,
    /// Controls the number of ASTRO emissions for the next epoch
    /// where next amount = two epoch EMA * emissions_multiple.
    /// If multiple < 1 then protocol emits less ASTRO than it buys back,
    /// otherwise protocol is inflating ASTRO supply.
    pub emissions_multiple: Decimal,
    /// Max ASTRO allowed per epoch. Parameter of the dynamic emissions curve.
    pub max_astro: Uint128,
}

impl Config {
    pub fn validate(&self) -> StdResult<()> {
        ensure!(
            POOL_NUMBER_LIMIT.contains(&self.pools_per_outpost),
            StdError::generic_err(format!(
                "Invalid pools_limit_per_outpost. Must be within [{}, {}] range",
                POOL_NUMBER_LIMIT.start(),
                POOL_NUMBER_LIMIT.end()
            ))
        );
        validate_native_denom(&self.whitelisting_fee.denom)?;
        validate_native_denom(&self.astro_denom)?;

        ensure!(
            self.whitelist_threshold > Decimal::zero() && self.whitelist_threshold < Decimal::one(),
            StdError::generic_err("whitelist_threshold must be within (0, 1) range")
        );

        ensure!(
            !self.emissions_multiple.is_zero(),
            StdError::generic_err("emissions_multiple must be greater than 0")
        );

        ensure!(
            !self.max_astro.is_zero(),
            StdError::generic_err("max_astro must be greater than 0")
        );

        Ok(())
    }
}

#[cw_serde]
pub struct OutpostParams {
    /// Emissions controller on a given outpost
    pub emissions_controller: String,
    /// wasm<>wasm IBC channel for voting
    pub voting_channel: String,
    /// General IBC channel for fungible token transfers
    pub ics20_channel: String,
}

/// Each outpost may have one pool that receives flat ASTRO emissions.
/// This pools doesn't participate in the voting process.
#[cw_serde]
pub struct AstroPoolConfig {
    /// Pool with ASTRO which needs to receive flat emissions
    pub astro_pool: String,
    /// Amount of ASTRO per epoch
    pub constant_emissions: Uint128,
}

#[cw_serde]
pub struct OutpostInfo {
    /// Outpost params contain all necessary information to interact with the remote outpost.
    /// This field also serves as marker whether it is The hub (params: None) or
    /// remote outpost (Some(params))
    pub params: Option<OutpostParams>,
    /// ASTRO token denom
    pub astro_denom: String,
    /// A pool that must receive flat ASTRO emissions. Optional.
    pub astro_pool_config: Option<AstroPoolConfig>,
}

#[cw_serde]
#[derive(Default)]
pub struct UserInfo {
    /// Last time when a user voted
    pub vote_ts: u64,
    /// Voting power used for the vote
    pub voting_power: Uint128,
    /// Vote distribution for all the pools a user picked
    pub votes: HashMap<String, Decimal>,
}

#[cw_serde]
pub struct UserInfoResponse {
    /// Last time when a user voted
    pub vote_ts: u64,
    /// Voting power used for the vote
    pub voting_power: Uint128,
    /// Vote distribution for all the pools a user picked
    pub votes: HashMap<String, Decimal>,
    /// Actual applied votes. This list excludes non-whitelisted pools
    pub applied_votes: HashMap<String, Decimal>,
}

#[cw_serde]
pub struct VotedPoolInfo {
    /// Time when the pool was whitelisted
    pub init_ts: u64,
    /// Voting power the pool received
    pub voting_power: Uint128,
}

impl VotedPoolInfo {
    /// Consume self and return a new instance with added voting power
    pub fn with_add_vp(self, vp: Uint128) -> Self {
        Self {
            voting_power: self.voting_power + vp,
            ..self
        }
    }

    /// Consume self and return a new instance with subtracted voting power
    pub fn with_sub_vp(self, vp: Uint128) -> Self {
        Self {
            voting_power: self.voting_power.saturating_sub(vp),
            ..self
        }
    }
}

#[cw_serde]
#[derive(Copy)]
pub enum OutpostStatus {
    InProgress,
    Failed,
    Done,
}

#[cw_serde]
pub struct TuneInfo {
    /// Last time when the tune was executed.
    /// Matches epoch start i.e., Monday 00:00 UTC every 2 weeks
    pub tune_ts: u64,
    /// Map of outpost prefix -> array of pools with their emissions
    pub pools_grouped: HashMap<String, Vec<(String, Uint128)>>,
    /// Map of outpost prefix -> IBC status. Hub should never enter this map.
    pub outpost_emissions_statuses: HashMap<String, OutpostStatus>,
    /// State of the dynamic emissions curve
    pub emissions_state: EmissionsState,
}

#[cw_serde]
pub struct SimulateTuneResponse {
    pub new_emissions_state: EmissionsState,
    pub next_pools_grouped: HashMap<String, Vec<(String, Uint128)>>,
}

#[cw_serde]
pub struct EmissionsState {
    /// xASTRO to ASTRO staking rate from the previous epoch
    pub xastro_rate: Decimal,
    /// Collected ASTRO from previous epoch.
    pub collected_astro: Uint128,
    /// EMA of the collected ASTRO from the previous epoch
    pub ema: Uint128,
    /// Amount of ASTRO to be emitted in the current epoch
    pub emissions_amount: Uint128,
}

#[cfg(test)]
mod unit_tests {
    use cosmwasm_std::coin;

    use super::*;

    #[test]
    fn test_validate_config() {
        let mut config = Config {
            owner: Addr::unchecked(""),
            assembly: Addr::unchecked(""),
            vxastro: Addr::unchecked(""),
            factory: Addr::unchecked(""),
            astro_denom: "uastro".to_string(),
            xastro_denom: "".to_string(),
            staking: Addr::unchecked(""),
            incentives_addr: Addr::unchecked(""),
            pools_per_outpost: 0,
            whitelisting_fee: coin(100, "uastro"),
            fee_receiver: Addr::unchecked(""),
            whitelist_threshold: Decimal::percent(10),
            emissions_multiple: Decimal::percent(80),
            max_astro: 1_400_000_000_000u128.into(),
        };
        assert_eq!(
            config.validate().unwrap_err(),
            StdError::generic_err("Invalid pools_limit_per_outpost. Must be within [1, 10] range")
        );

        config.pools_per_outpost = 5;
        config.whitelist_threshold = Decimal::zero();

        assert_eq!(
            config.validate().unwrap_err(),
            StdError::generic_err("whitelist_threshold must be within (0, 1) range")
        );

        config.whitelist_threshold = Decimal::percent(10);
        config.whitelisting_fee.denom = "u".to_string();

        assert_eq!(
            config.validate().unwrap_err(),
            StdError::generic_err("Invalid denom length [3,128]: u")
        );

        config.whitelisting_fee.denom = "uastro".to_string();
        config.astro_denom = "u".to_string();

        assert_eq!(
            config.validate().unwrap_err(),
            StdError::generic_err("Invalid denom length [3,128]: u")
        );

        config.astro_denom = "uastro".to_string();
        config.emissions_multiple = Decimal::zero();

        assert_eq!(
            config.validate().unwrap_err(),
            StdError::generic_err("emissions_multiple must be greater than 0")
        );

        config.emissions_multiple = Decimal::percent(80);
        config.max_astro = Uint128::zero();

        assert_eq!(
            config.validate().unwrap_err(),
            StdError::generic_err("max_astro must be greater than 0")
        );

        config.max_astro = 1_400_000_000_000u128.into();

        config.validate().unwrap();
    }
}
