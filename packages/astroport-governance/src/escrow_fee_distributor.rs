use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Admin address
    pub owner: String,
    /// Fee token address
    pub token: String,
    /// VotingEscrow contract address
    pub voting_escrow: String,
    /// Address to transfer `token` balance to, if this contract is killed
    pub emergency_return: String,
    /// Epoch time for fee distribution to start
    pub start_time: u64,
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Creates a request to change ownership.
    ProposeNewOwner {
        /// a new owner
        owner: String,
        /// the validity period of the offer to change the owner
        expires_in: u64,
    },
    /// Removes a request to change ownership.
    DropOwnershipProposal {},
    /// Approves ownership.
    ClaimOwnership {},
    /// Calculates the total number of tokens to be distributed in a given week.
    CheckpointToken {},
    /// Claim
    Claim {
        recipient: Option<String>,
    },
    ClaimMany {
        receivers: Vec<String>,
    },
    ToggleAllowCheckpointToken {},
    RecoverBalance {
        token_address: String,
    },
    KillMe {},
    Burn {
        token_address: String,
    },
    CheckpointTotalSupply {},
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns controls settings that specified in custom [`ConfigResponse`] structure.
    Config {},
    /// Returns information about who gets ASTRO fees every week
    AstroRecipientsPerWeek {},
    /// Returns the vxAstro balance for user at timestamp
    FetchUserBalanceByTimestamp { user: String, timestamp: u64 },
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Admin address
    pub owner: Addr,
    /// Fee token address
    pub token: Addr,
    /// VotingEscrow contract address
    pub voting_escrow: Addr,
    /// Address to transfer `token` balance to, if this contract is killed
    pub emergency_return: Addr,
    /// Epoch time for fee distribution to start
    pub start_time: u64,
    pub last_token_time: u64,
    pub time_cursor: u64,
    /// makes it possible for everyone to call
    pub can_checkpoint_token: bool,
    pub is_killed: bool,
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// ## Description
/// A custom struct for each query response that returns the vector of the recipients for distributed astro per week.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RecipientsPerWeekResponse {
    pub recipients: Vec<Addr>,
}

/// ## Description
/// A custom struct for each query response that returns the vector of the recipients for
/// distributed astro per week.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CheckpointToken {
    pub time: u64,
    pub tokens: Uint128,
}

/// ## Description
/// A custom struct for each query response that returns the vector of the recipients
/// who claimed astro.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Claimed {
    pub recipient: Addr,
    pub amount: Uint128,
    pub claim_period: u64,
    pub max_period: u64,
}

/// ## Description
/// A custom struct for each query response.
#[derive(Serialize, Default, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Point {
    pub bias: i128,
    pub slope: i128,
    pub ts: u64,
    pub blk: u64,
}
