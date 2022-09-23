use crate::voting_escrow::QueryMsg::{
    LockInfo, TotalVotingPower, TotalVotingPowerAt, UserVotingPower, UserVotingPowerAt,
};
use cosmwasm_std::{Addr, Binary, Decimal, QuerierWrapper, StdResult, Uint128};
use cw20::{Cw20ReceiveMsg, Logo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

/// ## Pagination settings
/// The maximum amount of items that can be read at once from
pub const MAX_LIMIT: u32 = 30;

/// The default amount of items to read from
pub const DEFAULT_LIMIT: u32 = 10;

pub const DEFAULT_PERIODS_LIMIT: u64 = 20;

/// This structure stores marketing information for vxASTRO.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct UpdateMarketingInfo {
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
    pub guardian_addr: Option<String>,
    /// xASTRO token address
    pub deposit_token_addr: String,
    /// Marketing info for vxASTRO
    pub marketing: Option<UpdateMarketingInfo>,
    /// The list of whitelisted logo urls prefixes
    pub logo_urls_whitelist: Vec<String>,
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
    /// Update config
    UpdateConfig { new_guardian: Option<String> },
    /// Set whitelisted logo urls
    SetLogoUrlsWhitelist { whitelist: Vec<String> },
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

/// This enum describes voters status.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlacklistedVotersResponse {
    /// Voters are blacklisted
    VotersBlacklisted {},
    /// Returns a voter that is not blacklisted.
    VotersNotBlacklisted { voter: String },
}

impl fmt::Display for BlacklistedVotersResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlacklistedVotersResponse::VotersBlacklisted {} => write!(f, "Voters are blacklisted!"),
            BlacklistedVotersResponse::VotersNotBlacklisted { voter } => {
                write!(f, "Voter is not blacklisted: {}", voter)
            }
        }
    }
}

/// This structure describes the query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Checks if specified addresses are blacklisted
    CheckVotersAreBlacklisted { voters: Vec<String> },
    /// Return the blacklisted voters
    BlacklistedVoters {
        start_after: Option<String>,
        limit: Option<u32>,
    },
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
    /// Return user's locked xASTRO balance at the given block height
    UserDepositAtHeight { user: String, height: u64 },
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
    /// Slope at which a staker's vxASTRO balance decreases over time
    pub slope: Uint128,
}

/// This structure stores the parameters returned when querying for a contract's configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Address that's allowed to change contract parameters
    pub owner: String,
    /// Address that can only blacklist vxASTRO stakers and remove their governance power
    pub guardian_addr: Option<Addr>,
    /// The xASTRO token contract address
    pub deposit_token_addr: String,
    /// The address of $ASTRO
    pub astro_addr: String,
    /// The address of $xASTRO staking contract
    pub xastro_staking_addr: String,
    /// The list of whitelisted logo urls prefixes
    pub logo_urls_whitelist: Vec<String>,
}

/// This structure describes a Migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub params: Binary,
}

/// Queries current user's voting power from the voting escrow contract.
///
/// * **user** staker for which we calculate the latest vxASTRO voting power.
pub fn get_voting_power(
    querier: &QuerierWrapper,
    escrow_addr: impl Into<String>,
    user: impl Into<String>,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse =
        querier.query_wasm_smart(escrow_addr, &UserVotingPower { user: user.into() })?;
    Ok(vp.voting_power)
}

/// Queries current user's voting power from the voting escrow contract by timestamp.
///
/// * **user** staker for which we calculate the voting power at a specific time.
///
/// * **timestamp** timestamp at which we calculate the staker's voting power.
pub fn get_voting_power_at(
    querier: &QuerierWrapper,
    escrow_addr: impl Into<String>,
    user: impl Into<String>,
    timestamp: u64,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse = querier.query_wasm_smart(
        escrow_addr,
        &UserVotingPowerAt {
            user: user.into(),
            time: timestamp,
        },
    )?;

    Ok(vp.voting_power)
}

/// Queries current total voting power from the voting escrow contract.
pub fn get_total_voting_power(
    querier: &QuerierWrapper,
    escrow_addr: impl Into<String>,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse = querier.query_wasm_smart(escrow_addr, &TotalVotingPower {})?;

    Ok(vp.voting_power)
}

/// Queries total voting power from the voting escrow contract by timestamp.
///
/// * **timestamp** time at which we fetch the total voting power.
pub fn get_total_voting_power_at(
    querier: &QuerierWrapper,
    escrow_addr: impl Into<String>,
    timestamp: u64,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse =
        querier.query_wasm_smart(escrow_addr, &TotalVotingPowerAt { time: timestamp })?;

    Ok(vp.voting_power)
}

/// Queries user's lockup information from the voting escrow contract.
///
/// * **user** staker for which we return lock position information.
pub fn get_lock_info(
    querier: &QuerierWrapper,
    escrow_addr: impl Into<String>,
    user: impl Into<String>,
) -> StdResult<LockInfoResponse> {
    let lock_info: LockInfoResponse =
        querier.query_wasm_smart(escrow_addr, &LockInfo { user: user.into() })?;
    Ok(lock_info)
}
