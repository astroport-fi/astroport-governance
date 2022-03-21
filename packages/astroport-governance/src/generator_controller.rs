use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// contract owner
    pub owner: String,
    /// the vxASTRO token contract address
    pub escrow_addr: String,
    /// generator contract address
    pub generator_addr: String,
    /// factory contract address
    pub factory_addr: String,
    /// max number of pools that can receive an ASTRO allocation
    pub pools_limit: u64,
}

/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Vote {
        votes: Vec<(String, u16)>,
    },
    GaugePools {},
    ChangePoolLimit {
        limit: u64,
    },
    /// Propose a new owner for the contract
    ProposeNewOwner {
        new_owner: String,
        expires_in: u64,
    },
    /// Remove the ownership transfer proposal
    DropOwnershipProposal {},
    /// Claim contract ownership
    ClaimOwnership {},
}

/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    UserInfo { user: String },
    GaugeInfo {},
    Config {},
    PoolInfo { pool_addr: String },
    PoolInfoAtPeriod { pool_addr: String, period: u64 },
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// This structure describes response with the main control config of generator controller contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// contract address that used for settings control
    pub owner: Addr,
    /// The vxASTRO token contract address
    pub escrow_addr: Addr,
    /// Generator contract address
    pub generator_addr: Addr,
    /// Factory contract address
    pub factory_addr: Addr,
    /// Max number of pools that can receive an ASTRO allocation
    pub pools_limit: u64,
}

/// This structure describes response with voting parameters for a specific pool.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct VotedPoolInfoResponse {
    pub vxastro_amount: Uint128,
    pub slope: Decimal,
}

/// This structure describes response with last gauge parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct GaugeInfoResponse {
    pub gauge_ts: u64,
    pub pool_alloc_points: Vec<(String, Uint128)>,
}

/// The struct describes response with last user's votes parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfoResponse {
    pub vote_ts: u64,
    pub voting_power: Uint128,
    pub slope: Decimal,
    pub lock_end: u64,
    pub votes: Vec<(Addr, u16)>,
}
