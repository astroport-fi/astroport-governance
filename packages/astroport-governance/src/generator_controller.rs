use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Decimal, Uint128};

/// The maximum amount of voters that can be kicked at once from
pub const VOTERS_MAX_LIMIT: u32 = 30;

/// This structure describes the basic settings for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Contract owner
    pub owner: String,
    /// The vxASTRO token contract address
    pub escrow_addr: String,
    /// Generator contract address
    pub generator_addr: String,
    /// Factory contract address
    pub factory_addr: String,
    /// Max number of pools that can receive ASTRO emissions at the same time
    pub pools_limit: u64,
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    KickBlacklistedVoters {
        blacklisted_voters: Vec<String>,
    },
    /// Vote allows a vxASTRO holder to cast votes on which generators should get ASTRO emissions in the next epoch
    Vote {
        votes: Vec<(String, u16)>,
    },
    /// TunePools transforms the latest vote distribution into alloc_points which are then applied to ASTRO generators
    TunePools {},
    UpdateConfig {
        /// The number of voters that can be kicked at once from the pool..
        blacklisted_voters_limit: Option<u32>,
        /// Main pool that will receive a minimum amount of ASTRO emissions
        main_pool: Option<String>,
        /// The minimum percentage of ASTRO emissions that main pool should get every block
        main_pool_min_alloc: Option<Decimal>,
        /// Should the main pool be removed or not? If the variable is omitted then the pool will be kept.
        remove_main_pool: Option<bool>,
    },
    /// ChangePoolsLimit changes the max amount of pools that can be voted at once to receive ASTRO emissions
    ChangePoolsLimit {
        limit: u64,
    },
    /// ProposeNewOwner proposes a new owner for the contract
    ProposeNewOwner {
        /// Newly proposed contract owner
        new_owner: String,
        /// The timestamp when the contract ownership change expires
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the latest contract ownership transfer proposal
    DropOwnershipProposal {},
    /// ClaimOwnership allows the newly proposed owner to claim contract ownership
    ClaimOwnership {},
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// UserInfo returns information about a voter and the generators they voted for
    #[returns(UserInfoResponse)]
    UserInfo { user: String },
    /// TuneInfo returns information about the latest generators that were voted to receive ASTRO emissions
    #[returns(GaugeInfoResponse)]
    TuneInfo {},
    /// Config returns the contract configuration
    #[returns(ConfigResponse)]
    Config {},
    /// PoolInfo returns the latest voting power allocated to a specific pool (generator)
    #[returns(VotedPoolInfoResponse)]
    PoolInfo { pool_addr: String },
    /// PoolInfo returns the voting power allocated to a specific pool (generator) at a specific period
    #[returns(VotedPoolInfoResponse)]
    PoolInfoAtPeriod { pool_addr: String, period: u64 },
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {
    /// Max number of blacklisted voters can be removed
    pub blacklisted_voters_limit: Option<u32>,
}

/// This structure describes the parameters returned when querying for the contract configuration.
#[cw_serde]
pub struct ConfigResponse {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// The vxASTRO token contract address
    pub escrow_addr: Addr,
    /// Generator contract address
    pub generator_addr: Addr,
    /// Factory contract address
    pub factory_addr: Addr,
    /// Max number of pools that can receive ASTRO emissions at the same time
    pub pools_limit: u64,
    /// Max number of blacklisted voters which can be removed
    pub blacklisted_voters_limit: Option<u32>,
    /// Main pool that will receive a minimum amount of ASTRO emissions
    pub main_pool: Option<Addr>,
    /// The minimum percentage of ASTRO emissions that main pool should get every block
    pub main_pool_min_alloc: Decimal,
}

/// This structure describes the response used to return voting information for a specific pool (generator).
#[cw_serde]
#[derive(Default)]
pub struct VotedPoolInfoResponse {
    /// vxASTRO amount that voted for this pool/generator
    pub vxastro_amount: Uint128,
    /// The slope at which the amount of vxASTRO that voted for this pool/generator will decay
    pub slope: Uint128,
}

/// This structure describes the response used to return tuning parameters for all pools/generators.
#[cw_serde]
#[derive(Default)]
pub struct GaugeInfoResponse {
    /// Last timestamp when a tuning vote happened
    pub tune_ts: u64,
    /// Distribution of alloc_points to apply in the Generator contract
    pub pool_alloc_points: Vec<(String, Uint128)>,
}

/// The struct describes a response used to return a staker's vxASTRO lock position.
#[cw_serde]
#[derive(Default)]
pub struct UserInfoResponse {
    /// Last timestamp when the user voted
    pub vote_ts: u64,
    /// The user's vxASTRO voting power
    pub voting_power: Uint128,
    /// The slope at which the user's voting power decays
    pub slope: Uint128,
    /// Timestamp when the user's lock expires
    pub lock_end: u64,
    /// The vote distribution for all the generators/pools the staker picked
    pub votes: Vec<(Addr, u16)>,
}
