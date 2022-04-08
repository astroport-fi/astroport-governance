use crate::voting_escrow::QueryMsg::{
    LockInfo, TotalVotingPower, TotalVotingPowerAt, UserVotingPower, UserVotingPowerAt,
};
use cosmwasm_std::{Addr, Binary, Decimal, QuerierWrapper, StdResult, Uint128};
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
    /// The maximum % of staked xASTRO that is confiscated upon an early exit
    pub max_exit_penalty: Option<Decimal>,
    /// The address that receives slashed ASTRO (slashed xASTRO is burned in order to claim ASTRO)
    pub slashed_fund_receiver: Option<String>,
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
    /// Early withdrawal with slashing penalty
    WithdrawEarly {},
    ConfigureEarlyWithdrawal {
        /// The maximum penalty that can be applied to a user
        max_penalty: Option<Decimal>,
        /// The address that will receive the slashed funds
        slashed_fund_receiver: Option<String>,
    },
    /// A callback after early withdrawal to send slashed ASTRO to the slashed funds receiver
    EarlyWithdrawCallback {
        /// Contracts' ASTRO balance before callback
        preupgrade_astro: Uint128,
        /// Slashed funds receiver
        slashed_funds_receiver: Addr,
    },
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
    /// Return the amount of xASTRO that the staker can withdraw right now after the penalty is applied
    /// for early withdrawal
    EarlyWithdrawAmount { user: String },
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
    pub slope: Uint128,
}

/// This structure stores the parameters returned when querying for a contract's configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
    pub deposit_token_addr: String,
}

/// This structure describes a Migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub params: Binary,
}

/// ## Description
/// Queries current user's voting power from the voting escrow contract.
pub fn get_voting_power(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &UserVotingPower {
            user: user.to_string(),
        },
    )?;
    Ok(vp.voting_power)
}

/// ## Description
/// Queries current user's voting power from the voting escrow contract by timestamp.
pub fn get_voting_power_at(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
    timestamp: u64,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &UserVotingPowerAt {
            user: user.to_string(),
            time: timestamp,
        },
    )?;

    Ok(vp.voting_power)
}

/// ## Description
/// Queries current total voting power from the voting escrow contract.
pub fn get_total_voting_power(querier: QuerierWrapper, escrow_addr: &Addr) -> StdResult<Uint128> {
    let vp: VotingPowerResponse =
        querier.query_wasm_smart(escrow_addr.clone(), &TotalVotingPower {})?;

    Ok(vp.voting_power)
}

/// ## Description
/// Queries total voting power from the voting escrow contract by timestamp.
pub fn get_total_voting_power_at(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    timestamp: u64,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse =
        querier.query_wasm_smart(escrow_addr.clone(), &TotalVotingPowerAt { time: timestamp })?;

    Ok(vp.voting_power)
}

/// ## Description
/// Queries user's lockup information from the voting escrow contract.
pub fn get_lock_info(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
) -> StdResult<LockInfoResponse> {
    let lock_info: LockInfoResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &LockInfo {
            user: user.to_string(),
        },
    )?;
    Ok(lock_info)
}
