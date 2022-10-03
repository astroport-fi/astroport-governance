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
    /// Voting escrow contract address
    pub voting_escrow_addr: String,
    /// Max limit of addresses to claim rewards for in a single call
    pub claim_many_limit: Option<u64>,
    /// Whether reward claiming is disabled
    pub is_claim_disabled: Option<bool>,
}

/// This structure describes the execute messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// ProposeNewOwner creates a request to change contract ownership
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the offer to change the contract owner
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the request to change contract ownership
    DropOwnershipProposal {},
    /// ClaimOwnership claims contract ownership
    ClaimOwnership {},
    /// Claim claims staking rewards for a single staker and sends them to the specified recipient
    Claim {
        recipient: Option<String>,
        max_periods: Option<u64>,
    },
    /// ClaimMany claims staking rewards for multiple addresses in a single call
    ClaimMany { receivers: Vec<String> },
    /// UpdateConfig updates the contract configuration
    UpdateConfig {
        /// Max limit of addresses to claim rewards for in a single call
        claim_many_limit: Option<u64>,
        /// Whether reward claiming is disabled
        is_claim_disabled: Option<bool>,
    },
    /// Receive receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template
    Receive(Cw20ReceiveMsg),
}

/// This structure describes query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Config returns control settings using a custom [`ConfigResponse`] structure
    Config {},
    /// UserReward returns the reward amount that can be claimed by a staker in the form of ASTRO at a specified timestamp
    UserReward { user: String, timestamp: u64 },
    /// AvailableRewardPerWeek returns a vector that contains the total reward amount per week distributed to vxASTRO stakers
    AvailableRewardPerWeek {
        start_after: Option<u64>,
        limit: Option<u64>,
    },
}

/// This structure describes the parameters returned when querying for the contract configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// Fee token address (ASTRO token)
    pub astro_token: Addr,
    /// Voting escrow contract address
    pub voting_escrow_addr: Addr,
    /// Max limit of addresses to claim rewards for in a single call
    pub claim_many_limit: u64,
    /// Wthether reward claiming is disabled
    pub is_claim_disabled: bool,
}

/// This structure describes a migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// This structure describes custom hooks for a CW20.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// ReceiveTokens receives tokens into the contract and triggers a vxASTRO checkpoint.
    ReceiveTokens {},
}
