use std::collections::HashMap;

use crate::emissions_controller::consts::{
    LIQUIDITY_PERCENT_MAX, LIQUIDITY_PERCENT_MIN, POOL_NUMBER_LIMIT, SPREAD_PER_STEP_MAX,
    SPREAD_PER_STEP_MIN,
};
use crate::emissions_controller::router::RouteStepVerbose;
use crate::voting_escrow::UpdateMarketingInfo;
use astroport::asset::validate_native_denom;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{ensure, Addr, Coin, Decimal, Empty, StdError, StdResult, Uint128};

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
    /// Whitelist eligibility check requires a pool to have a valid swap route to ASTRO.
    /// This parameter defines what percentage of the whitelisted pool's total liquidity to be
    /// used in swap simulations.
    pub liquidity_percent: Decimal,
    /// When adding a new pool to the whitelist, the contract checks that the spread
    /// is within acceptable limits by simulating a swap along the provided route.
    /// This parameter defines the maximum allowed spread per swap step in the route.
    /// For example, if the route has 3 steps, and the allowed_spread_per_step is 1%,
    /// then the total allowed spread for the entire route is approximately 3%.
    /// This parameter protects against whitelisting pools which don't generate fees for protocol.
    pub allowed_spread_per_step: Decimal,
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
        liquidity_percent: Option<Decimal>,
        allowed_spread_per_step: Option<Decimal>,
        enable_unwhitelisting: Option<bool>,
    },
    /// Permissionless endpoint.
    /// Whitelists a pool to receive ASTRO emissions.
    /// Requires fee payment.
    /// Runs eligibility checks for the pool.
    /// A pool is eligible if:
    /// 1. It is a valid Astroport pool
    /// 2. It has a valid swap route to ASTRO
    /// If the pool belongs to an outpost,
    /// this endpoint launches an IBC message to validate the pool on the outpost.
    /// Outpost lp token is added to the whitelist only if outpost confirms in IBC callback that the pool is valid.
    WhitelistPool { lp_token: String },
    /// Permissionless endpoint.
    /// Checks that a pool is still eligible for whitelisting.
    /// If a pool doesn't meet the criteria, it will be removed from the whitelist.
    /// If a pool still meets the criteria, nothing happens.
    /// Outpost pool is unwhitelisted only if outpost confirms in IBC callback that the pool is no longer eligible.
    /// Note that unwhitelisting a pool doesn't mean blacklisting.
    /// A pool can be whitelisted again by calling WhitelistPool endpoint.
    UnwhitelistIneligiblePool { lp_token: String },
    /// Permissioned to the contract owner.
    /// Pins or unpins a pool in the whitelist.
    /// Pinned pools can't be unwhitelisted by permissionless endpoint for being ineligible.
    /// However, pinned pools can be unwhitelisted naturally through vxASTRO voting process.
    TogglePinnedPool { lp_token: String, pin: bool },
    /// Manages pool blacklist.
    /// Blacklisting prevents voting for it.
    /// If the pool is whitelisted, it will be removed from the whitelist.
    /// All its votes will be forfeited immediately.
    /// Users will be able to apply their votes to other pools at the next epoch (if they already voted).
    /// Removing a pool from the blacklist will not restore the votes
    /// and will not add it to the whitelist automatically.
    /// Only contract owner can call this endpoint.
    UpdateBlacklist {
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
    },
    /// Register or update an outpost
    UpdateOutpost {
        /// Bech32 prefix
        prefix: String,
        /// Astro denom on this outpost
        astro_denom: String,
        /// Outpost params contain all necessary information to interact with the remote outpost.
        /// This field also serves as marker whether it is The hub (params: None) or
        /// remote outpost (Some(params))
        outpost_params: Option<InputOutpostParams>,
        /// A pool that must receive flat ASTRO emissions. Optional.
        astro_pool_config: Option<AstroPoolConfig>,
    },
    /// Jail an outpost.
    /// Jailed outposts can't participate in the voting process but still allow
    /// outpost users to unlock their vxASTRO.
    JailOutpost { prefix: String },
    /// Unjail an outpost.
    /// Unjailed outposts retain all previous configurations but will need to whitelist pools and
    /// start a voting process from scratch.
    UnjailOutpost { prefix: String },
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
    /// QueryBlacklist returns the list of pools that are not allowed to be voted for.
    /// The query is paginated.
    /// If 'start_after' is provided, it yields a list **excluding** 'start_after'.
    #[returns(Vec<String>)]
    QueryBlacklist {
        limit: Option<u8>,
        start_after: Option<String>,
    },
    /// CheckWhitelist checks all the pools in the list and returns whether they are whitelisted.
    /// Returns array of tuples (LP token, is_whitelisted).
    #[returns(Vec<(String, bool)>)]
    CheckWhitelist { lp_tokens: Vec<String> },
    /// SimulateTune simulates the ASTRO amount that will be emitted in the next epoch per pool
    /// considering if the next epoch starts right now.
    /// This query is useful for the UI to show the expected ASTRO emissions
    /// as well as might be useful for integrator estimations.
    /// It filters out pools which don't belong to any of outposts and invalid Hub-based LP tokens.
    /// Returns TuneResultResponse object which contains
    /// emissions state and next pools grouped by outpost prefix.
    #[returns(SimulateTuneResponse)]
    SimulateTune {},
    /// Checks if a whitelisted pool is still eligible for whitelisting.
    /// Runs the same checks as when whitelisting a pool.
    /// If a pool doesn't meet the criteria, this query throws an error.
    /// If a pool still meets the criteria, it returns an empty response.
    /// This query can only check a pool belonging to the Hub.
    /// Outpost pools can only be checked by calling this query on the respective outpost.
    /// A pool is eligible if:
    /// 1. It is a valid Astroport pool
    /// 2. It has a valid swap route to ASTRO
    #[returns(Empty)]
    CheckWhitelistEligibility {
        lp_token: String,
        liquidity_percent: Decimal,
        allowed_spread_per_step: Decimal,
    },
    #[returns(Vec<RouteStepVerbose>)]
    WhitelistingRoutes {
        start_after: Option<String>,
        limit: Option<u32>,
    },
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
    /// Whitelist eligibility check requires a pool to have a valid swap route to ASTRO.
    /// This parameter defines what percentage of the whitelisted pool's total liquidity to be
    /// used in swap simulations.
    pub liquidity_percent: Decimal,
    /// When adding a new pool to the whitelist, the contract checks that the spread
    /// is within acceptable limits by simulating a swap along the provided route.
    /// This parameter defines the maximum allowed spread per swap step in the route.
    /// For example, if the route has 3 steps, and the allowed_spread_per_step is 1%,
    /// then the total allowed spread for the entire route is approximately 3%.
    /// This parameter protects against whitelisting pools which don't generate fees for protocol.
    pub allowed_spread_per_step: Decimal,
    /// Enables or disables the permissionless unwhitelisting of ineligible pools.
    /// Can be used to temporarily disable the feature in case of a certain pool's
    /// liquidity is being moved to another pool.
    pub unwhitelisting_enabled: bool,
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

        ensure!(
            self.liquidity_percent >= LIQUIDITY_PERCENT_MIN
                && self.liquidity_percent <= LIQUIDITY_PERCENT_MAX,
            StdError::generic_err(format!(
                "liquidity_percent must be within [{LIQUIDITY_PERCENT_MIN}, {LIQUIDITY_PERCENT_MAX}] range"
            ))
        );

        ensure!(
            self.allowed_spread_per_step >= SPREAD_PER_STEP_MIN
                && self.allowed_spread_per_step <= SPREAD_PER_STEP_MAX,
            StdError::generic_err(format!(
                "allowed_spread_per_step must be within [{SPREAD_PER_STEP_MIN}, {SPREAD_PER_STEP_MAX}] range"
            ))
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
    /// ICS20 transfer escrow address on Neutron. Calculated automatically based on channel id
    pub escrow_address: Addr,
}

#[cw_serde]
pub struct InputOutpostParams {
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
    /// This field also serves as a marker whether it is The hub (params: None) or
    /// remote outpost (Some(params))
    pub params: Option<OutpostParams>,
    /// ASTRO token denom
    pub astro_denom: String,
    /// A pool that must receive flat ASTRO emissions. Optional.
    pub astro_pool_config: Option<AstroPoolConfig>,
    /// Defines whether outpost is jailed. Jailed outposts can't participate in the voting process,
    /// but they still allow remote users to unstake their vxASTRO.
    pub jailed: bool,
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
            liquidity_percent: Decimal::percent(10),
            allowed_spread_per_step: Decimal::percent(5),
            unwhitelisting_enabled: false,
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

        config.liquidity_percent = Decimal::zero();
        assert_eq!(
            config.validate().unwrap_err(),
            StdError::generic_err("liquidity_percent must be within [0.01, 0.5] range")
        );

        config.liquidity_percent = Decimal::percent(10);
        config.allowed_spread_per_step = Decimal::zero();

        assert_eq!(
            config.validate().unwrap_err(),
            StdError::generic_err("allowed_spread_per_step must be within [0.01, 0.5] range")
        );

        config.allowed_spread_per_step = Decimal::percent(5);

        config.validate().unwrap();
    }
}
