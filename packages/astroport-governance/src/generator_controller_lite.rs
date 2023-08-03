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
    /// Assembly contract address
    pub assembly_addr: String,
    /// Hub contract address
    pub hub_addr: Option<String>,
    /// Max number of pools that can receive ASTRO emissions at the same time
    pub pools_limit: u64,
    /// The list of pools which are eligible to receive votes
    pub whitelisted_pools: Vec<String>,
}

/// This structure describes the execute messages available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Removes all votes applied by blacklisted voters
    KickBlacklistedVoters { blacklisted_voters: Vec<String> },
    /// Removes all votes applied by voters that have unlocked
    KickUnlockedVoters { unlocked_voters: Vec<String> },
    /// Removes all votes applied by a voter that have unlocked on an Outpost
    KickUnlockedOutpostVoter { unlocked_voter: String },
    /// Vote allows a vxASTRO holder to cast votes on which generators should get ASTRO emissions in the next epoch
    Vote { votes: Vec<(String, u16)> },
    /// OutpostVote allows a vxASTRO holder on an Outpost to cast votes on which generators should get ASTRO emissions in the next epoch
    OutpostVote {
        voter: String,
        voting_power: Uint128,
        votes: Vec<(String, u16)>,
    },
    /// TunePools transforms the latest vote distribution into alloc_points which are then applied to ASTRO generators
    TunePools {},
    UpdateConfig {
        // Assembly contract address
        assembly_addr: Option<String>,
        /// The number of voters that can be kicked at once from the pool..
        kick_voters_limit: Option<u32>,
        /// Main pool that will receive a minimum amount of ASTRO emissions
        main_pool: Option<String>,
        /// The minimum percentage of ASTRO emissions that main pool should get every block
        main_pool_min_alloc: Option<Decimal>,
        /// Should the main pool be removed or not? If the variable is omitted then the pool will be kept.
        remove_main_pool: Option<bool>,
        // Hub contract address
        hub_addr: Option<String>,
    },
    /// ChangePoolsLimit changes the max amount of pools that can be voted at once to receive ASTRO emissions
    ChangePoolsLimit { limit: u64 },
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
    /// Adds or removes the pools which are eligible to receive votes
    UpdateWhitelist {
        add: Option<Vec<String>>,
        remove: Option<Vec<String>>,
    },
    // Update network config for IBC
    UpdateNetworks {
        // Adding requires a list of (network, address prefix, IBC governance channel)
        add: Option<Vec<NetworkInfo>>,
        remove: Option<Vec<String>>,
    },
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
pub struct MigrateMsg {}

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
    /// Assembly contract address
    pub assembly_addr: Addr,
    /// Hub contract address
    pub hub_addr: Option<Addr>,
    /// Max number of pools that can receive ASTRO emissions at the same time
    pub pools_limit: u64,
    /// Max number of voters which can be kicked at a time
    pub kick_voters_limit: Option<u32>,
    /// Main pool that will receive a minimum amount of ASTRO emissions
    pub main_pool: Option<Addr>,
    /// The minimum percentage of ASTRO emissions that main pool should get every block
    pub main_pool_min_alloc: Decimal,
    /// The list of pools which are eligible to receive votes
    pub whitelisted_pools: Vec<String>,
    /// The list of pools which are eligible to receive votes
    pub whitelisted_networks: Vec<NetworkInfo>,
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
    /// Last period when a tuning was applied
    pub tune_period: u64,
    /// Distribution of alloc_points to apply in the Generator contract
    pub pool_alloc_points: Vec<(String, Uint128)>,
}

/// The struct describes a response used to return a staker's vxASTRO lock position.
#[cw_serde]
#[derive(Default)]
pub struct UserInfoResponse {
    /// The period when the user voted last time, None if they've never voted
    pub vote_period: Option<u64>,
    /// The user's vxASTRO voting power
    pub voting_power: Uint128,
    /// The vote distribution for all the generators/pools the staker picked
    pub votes: Vec<(String, u16)>,
}

#[cw_serde]
#[derive(Eq, Hash)]
pub struct NetworkInfo {
    /// The address prefix for the network, e.g. "terra". This is determined
    /// by the contract and will be overwritten in update_networks
    pub address_prefix: String,
    /// The address of the generator contract on the Outpost
    pub generator_address: Addr,
    /// The IBC channel used for governance
    pub ibc_channel: Option<String>,
}
