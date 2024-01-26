use std::fmt;

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, QuerierWrapper, StdResult, Uint128, Uint64};
use cw20::{BalanceResponse, DownloadLogoResponse, Logo, MarketingInfoResponse, TokenInfoResponse};

use crate::voting_escrow_lite::QueryMsg::{
    LockInfo, TotalVotingPower, TotalVotingPowerAt, UserDepositAt, UserEmissionsVotingPower,
    UserVotingPower, UserVotingPowerAt,
};

/// ## Pagination settings
/// The maximum amount of items that can be read at once from
pub const MAX_LIMIT: u32 = 30;

/// The default amount of items to read from
pub const DEFAULT_LIMIT: u32 = 10;

pub const DEFAULT_PERIODS_LIMIT: u64 = 20;

/// This structure stores marketing information for vxASTRO.
#[cw_serde]
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
#[cw_serde]
pub struct InstantiateMsg {
    /// The vxASTRO contract owner
    pub owner: String,
    /// Address that's allowed to black or whitelist contracts
    pub guardian_addr: Option<String>,
    /// xASTRO token address
    pub deposit_denom: String,
    /// Marketing info for vxASTRO
    pub marketing: Option<UpdateMarketingInfo>,
    /// The list of whitelisted logo urls prefixes
    pub logo_urls_whitelist: Vec<String>,
    /// Address of the Generator controller to kick unlocked users
    pub generator_controller_addr: Option<String>,
    /// Address of the Outpost to handle unlock remotely
    pub outpost_addr: Option<String>,
}

/// This structure describes the execute functions in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Create a vxASTRO position and lock xASTRO for `time` amount of time
    CreateLock {},
    /// Deposit xASTRO in another user's vxASTRO position
    DepositFor { user: String },
    /// Add more xASTRO to your vxASTRO position
    ExtendLockAmount {},
    /// Unlock xASTRO from the vxASTRO contract
    Unlock {},
    /// Relock all xASTRO from an unlocking position if the Hub could not be notified
    Relock { user: String },
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
        #[serde(default)]
        append_addrs: Vec<String>,
        #[serde(default)]
        remove_addrs: Vec<String>,
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
    UpdateConfig {
        new_guardian: Option<String>,
        generator_controller: Option<String>,
        outpost: Option<String>,
    },
    /// Set whitelisted logo urls
    SetLogoUrlsWhitelist { whitelist: Vec<String> },
}

/// This enum describes voters status.
#[cw_serde]
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
                write!(f, "Voter is not blacklisted: {voter}")
            }
        }
    }
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Checks if specified addresses are blacklisted
    #[returns(BlacklistedVotersResponse)]
    CheckVotersAreBlacklisted { voters: Vec<String> },
    /// Return the blacklisted voters
    #[returns(Vec<Addr>)]
    BlacklistedVoters {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Return the user's vxASTRO balance
    #[returns(BalanceResponse)]
    Balance { address: String },
    /// Fetch the vxASTRO token information
    #[returns(TokenInfoResponse)]
    TokenInfo {},
    /// Fetch vxASTRO's marketing information
    #[returns(MarketingInfoResponse)]
    MarketingInfo {},
    /// Download the vxASTRO logo
    #[returns(DownloadLogoResponse)]
    DownloadLogo {},
    /// Return the current total amount of vxASTRO
    #[returns(VotingPowerResponse)]
    TotalVotingPower {},
    /// Return the total amount of vxASTRO at some point in the past
    #[returns(VotingPowerResponse)]
    TotalVotingPowerAt { time: u64 },
    /// Return the total voting power at a specific period
    #[returns(VotingPowerResponse)]
    TotalVotingPowerAtPeriod { period: u64 },
    /// Return the user's current voting power (vxASTRO balance)
    #[returns(VotingPowerResponse)]
    UserVotingPower { user: String },
    /// Return the user's vxASTRO balance at some point in the past
    #[returns(VotingPowerResponse)]
    UserVotingPowerAt { user: String, time: u64 },
    /// Return the user's voting power at a specific period
    #[returns(VotingPowerResponse)]
    UserVotingPowerAtPeriod { user: String, period: u64 },

    #[returns(VotingPowerResponse)]
    TotalEmissionsVotingPower {},
    /// Return the total amount of vxASTRO at some point in the past
    #[returns(VotingPowerResponse)]
    TotalEmissionsVotingPowerAt { time: u64 },
    /// Return the user's current emission voting power
    #[returns(VotingPowerResponse)]
    UserEmissionsVotingPower { user: String },
    /// Return the user's emission voting power  at some point in the past
    #[returns(VotingPowerResponse)]
    UserEmissionsVotingPowerAt { user: String, time: u64 },

    #[returns(LockInfoResponse)]
    LockInfo { user: String },
    /// Return user's locked xASTRO balance at the given timestamp
    #[returns(Uint128)]
    UserDepositAt { user: String, timestamp: Uint64 },
    /// Return the  vxASTRO contract configuration
    #[returns(Config)]
    Config {},
}

/// This structure is used to return a user's amount of vxASTRO.
#[cw_serde]
pub struct VotingPowerResponse {
    /// The vxASTRO balance
    pub voting_power: Uint128,
}

/// This structure is used to return the lock information for a vxASTRO position.
#[cw_serde]
pub struct LockInfoResponse {
    /// The amount of xASTRO locked in the position
    pub amount: Uint128,
    /// Indicates the end of a lock period, if None the position is locked
    pub end: Option<u64>,
}

/// This structure stores the main parameters for the voting escrow contract.
#[cw_serde]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// Address that can only blacklist vxASTRO stakers and remove their governance power
    pub guardian_addr: Option<Addr>,
    /// The xASTRO token contract address
    pub deposit_denom: String,
    /// The list of whitelisted logo urls prefixes
    pub logo_urls_whitelist: Vec<String>,
    /// Minimum unlock wait time in seconds
    pub unlock_period: u64,
    /// Address of the Generator controller to kick unlocked users
    pub generator_controller_addr: Option<Addr>,
    /// Address of the Outpost to handle unlock remotely
    pub outpost_addr: Option<Addr>,
}

/// This structure describes a Migration message.
#[cw_serde]
pub struct MigrateMsg {
    pub params: Binary,
}

/// Queries current user's deposit from the voting escrow contract.
///
/// * **user** staker for which we fetch the latest xASTRO deposits.
///
/// * **timestamp** timestamp to fetch deposits at.
pub fn get_user_deposit_at_time(
    querier: &QuerierWrapper,
    escrow_addr: impl Into<String>,
    user: impl Into<String>,
    timestamp: u64,
) -> StdResult<Uint128> {
    let balance = querier.query_wasm_smart(
        escrow_addr,
        &UserDepositAt {
            user: user.into(),
            timestamp: Uint64::from(timestamp),
        },
    )?;
    Ok(balance)
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

/// Queries current user's emissions voting power from the voting escrow contract.
///
/// * **user** staker for which we calculate the latest vxASTRO voting power.
pub fn get_emissions_voting_power(
    querier: &QuerierWrapper,
    escrow_addr: impl Into<String>,
    user: impl Into<String>,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse =
        querier.query_wasm_smart(escrow_addr, &UserEmissionsVotingPower { user: user.into() })?;
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
