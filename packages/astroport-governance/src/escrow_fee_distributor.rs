use cosmwasm_std::Addr;
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Admin address
    pub owner: String,
    /// Fee token address
    pub astro_token: String,
    /// VotingEscrow contract address
    pub voting_escrow_addr: String,
    /// Max limit of addresses to claim rewards in single call
    pub claim_many_limit: Option<u64>,
    /// Is reward claiming disabled: for emergency
    pub is_claim_disabled: Option<bool>,
}

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
    /// Claim single address in single call
    Claim {
        recipient: Option<String>,
        max_periods: Option<u64>,
    },
    /// Claim multiple addresses in single call
    ClaimMany { receivers: Vec<String> },
    UpdateConfig {
        /// Max limit of addresses to claim rewards in single call
        claim_many_limit: Option<u64>,
        /// Is reward claiming disabled: for emergency
        is_claim_disabled: Option<bool>,
    },
    /// Receive receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the
    /// received template.
    Receive(Cw20ReceiveMsg),
}

/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns controls settings that specified in custom [`ConfigResponse`] structure.
    Config {},
    /// Returns the reward amount in the form of Astro for the user by timestamp
    UserReward { user: String, timestamp: u64 },
    /// Returns the vector that contains the total reward amount per week
    AvailableRewardPerWeek {
        start_after: Option<u64>,
        limit: Option<u64>,
    },
}

/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Admin address
    pub owner: Addr,
    /// Fee token address
    pub astro_token: Addr,
    /// VotingEscrow contract address
    pub voting_escrow_addr: Addr,
    /// Max limit of addresses to claim rewards in single call
    pub claim_many_limit: u64,
    /// Is reward claiming disabled: for emergency
    pub is_claim_disabled: bool,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// This structure describes custom hooks for the CW20.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Receive tokens into the contract and trigger a token checkpoint.
    ReceiveTokens {},
}
