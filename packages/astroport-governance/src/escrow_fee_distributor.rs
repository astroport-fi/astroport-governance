use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Admin address
    pub owner: String,
    /// Fee token address
    pub astro_token: String,
    /// VotingEscrow contract address
    pub voting_escrow_addr: String,
    /// Address to transfer `token` balance to, if this contract is killed
    pub emergency_return_addr: String,
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
    UpdateConfig {
        max_limit_accounts_of_claim: Option<u64>,
        /// Enables or disables the ability to set a checkpoint token for everyone
        checkpoint_token_enabled: Option<bool>,
    },
    /// Receive receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the
    /// received template.
    Receive(Cw20ReceiveMsg),
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns controls settings that specified in custom [`ConfigResponse`] structure.
    Config {},
    /// Returns a commission amount in the form of Astro for user at timestamp
    FetchUserBalanceByTimestamp { user: String, timestamp: u64 },
    /// Returns the vector that contains voting supply per week
    VotingSupplyPerWeek {
        start_after: Option<u64>,
        limit: Option<u64>,
    },
    /// Returns the vector that contains tokens fee per week
    FeeTokensPerWeek {
        start_after: Option<u64>,
        limit: Option<u64>,
    },
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Admin address
    pub owner: Addr,
    /// Fee token address
    pub astro_token: Addr,
    /// VotingEscrow contract address
    pub voting_escrow_addr: Addr,
    /// Address to transfer `token` balance to, if this contract is killed
    pub emergency_return_addr: Addr,
    /// Period time for fee distribution to start
    pub start_time: u64,
    pub last_token_time: u64,
    pub time_cursor: u64,
    /// makes it possible for everyone to call
    pub checkpoint_token_enabled: bool,
    pub max_limit_accounts_of_claim: u64,
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// ## Description
/// A custom struct for each query response that returns the vector of the recipients for
/// distributed astro per week.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CheckpointToken {
    pub time: u64,
    pub tokens: Uint128,
}

/// ## Description
/// This structure describes custom hooks for the CW20.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Receive tokens into the contract and trigger a token checkpoint.
    Burn {},
}
