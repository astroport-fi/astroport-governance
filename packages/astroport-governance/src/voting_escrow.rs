use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::{BalanceResponse, DownloadLogoResponse, Logo, MarketingInfoResponse, TokenInfoResponse};

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
    /// xASTRO denom
    pub deposit_denom: String,
    /// Marketing info for vxASTRO
    pub marketing: Option<UpdateMarketingInfo>,
    /// The list of whitelisted logo urls prefixes
    pub logo_urls_whitelist: Vec<String>,
}

/// This structure describes the execute functions in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Create a vxASTRO position and lock xASTRO
    Lock { receiver: Option<String> },
    /// Unlock xASTRO from the vxASTRO contract
    Unlock {},
    /// Cancel unlocking
    Relock {},
    /// Withdraw xASTRO from the vxASTRO contract
    Withdraw {},
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
    /// Set whitelisted logo urls
    SetLogoUrlsWhitelist { whitelist: Vec<String> },
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
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
    #[returns(Uint128)]
    TotalVotingPower { time: Option<u64> },
    /// Return the user's current voting power (vxASTRO balance)
    #[returns(Uint128)]
    UserVotingPower { user: String, time: Option<u64> },
    /// Fetch a user's lock information
    #[returns(LockInfoResponse)]
    LockInfo { user: String },
    /// Return the  vxASTRO contract configuration
    #[returns(Config)]
    Config {},
}

/// This structure stores the main parameters for the voting escrow contract.
#[cw_serde]
pub struct Config {
    /// The xASTRO denom
    pub deposit_denom: String,
    /// The list of whitelisted logo urls prefixes
    pub logo_urls_whitelist: Vec<String>,
}

#[cw_serde]
pub struct LockInfoResponse {
    /// The total amount of xASTRO tokens that were deposited in the vxASTRO position
    pub amount: Uint128,
    /// The timestamp when a lock will be unlocked. None for positions in Locked state
    pub end: Option<u64>,
}
