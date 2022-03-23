use cosmwasm_std::{Decimal, Uint128};
use cw20::{Cw20ReceiveMsg, Logo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure stores marketing information for vxASTRO.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct InstantiateMarketingInfo {
    /// Project URL
    pub project: Option<String>,
    /// Token description
    pub description: Option<String>,
    /// Token marketing information
    pub marketing: Option<String>,
    /// Token logo
    pub logo: Option<Logo>,
}

/// This structure stores general parameters for the vxASTRO contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// The vxASTRO contract owner
    pub owner: String,
    /// Address that's allowed to black or whitelist contracts
    pub guardian_addr: String,
    /// xASTRO token address
    pub deposit_token_addr: String,
    /// Marketing info for vxASTRO
    pub marketing: Option<InstantiateMarketingInfo>,
}

/// This structure describes the execute functions in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Extend the lockup time for your staked xASTRO
    ExtendLockTime { time: u64 },
    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received
    /// template.
    Receive(Cw20ReceiveMsg),
    /// Withdraw xASTRO from the vxASTRO contract
    Withdraw {},
    /// Propose a new owner for the contract
    ProposeNewOwner { new_owner: String, expires_in: u64 },
    /// Remove the ownership transfer proposal
    DropOwnershipProposal {},
    /// Claim contract ownership
    ClaimOwnership {},
    /// Add or remove accounts from the blacklist
    UpdateBlacklist {
        append_addrs: Option<Vec<String>>,
        remove_addrs: Option<Vec<String>>,
    },
    /// Update the marketing info for the vxASTRO contract
    UpdateMarketing {
        /// A URL pointing to the project behind this token
        project: Option<String>,
        /// A longer description of the token and its utility. Designed for tooltips or such
        description: Option<String>,
        /// The address (if any) that can update this data structure
        marketing: Option<String>,
    },
    /// Upload a logo for vxASTRO
    UploadLogo(Logo),
}

/// This structure describes a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Create a vxASTRO position and lock xASTRO for `time` amount of time
    CreateLock { time: u64 },
    /// Deposit xASTRO in another user's vxASTRO position
    DepositFor { user: String },
    /// Add more xASTRO to your vxASTRO position
    ExtendLockAmount {},
}

/// This structure describes the query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return the user's vxASTRO balance
    Balance { address: String },
    /// Fetch the vxASTRO token information
    TokenInfo {},
    /// Fetch vxASTRO's marketing information
    MarketingInfo {},
    /// Download the vxASTRO logo
    DownloadLogo {},
    /// Return the current total amount of vxASTRO
    TotalVotingPower {},
    /// Return the total amount of vxASTRO at some point in the past
    TotalVotingPowerAt { time: u64 },
    /// Return the total voting power at a specific period
    TotalVotingPowerAtPeriod { period: u64 },
    /// Return the user's current voting power (vxASTRO balance)
    UserVotingPower { user: String },
    /// Return the user's vxASTRO balance at some point in the past
    UserVotingPowerAt { user: String, time: u64 },
    /// Return the user's voting power at a specific period
    UserVotingPowerAtPeriod { user: String, period: u64 },
    /// Return information about a user's lock position
    LockInfo { user: String },
    /// Return the  vxASTRO contract configuration
    Config {},
}

/// This structure is used to return a user's amount of vxASTRO.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VotingPowerResponse {
    /// The vxASTRO balance
    pub voting_power: Uint128,
}

/// This structure is used to return the lock information for a vxASTRO position.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LockInfoResponse {
    /// The amount of xASTRO locked in the position
    pub amount: Uint128,
    /// This is the initial boost for the lock position
    pub coefficient: Decimal,
    /// Start time for the vxASTRO position decay
    pub start: u64,
    /// End time for the vxASTRO position decay
    pub end: u64,
    pub slope: Decimal,
}

/// This structure stores the parameters returned when querying for a contract's configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
    pub deposit_token_addr: String,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
