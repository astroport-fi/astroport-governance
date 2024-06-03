use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use cw20::{BalanceResponse, Logo, MarketingInfoResponse, TokenInfoResponse};

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
    /// Astroport Emissions Controller contract
    pub emissions_controller: String,
    /// Marketing info for vxASTRO
    pub marketing: Option<UpdateMarketingInfo>,
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
    /// Permissioned to the Emissions Controller contract.
    /// Confirms unlocking for a specific user.
    /// Unconfirmed unlocks can't be withdrawn.
    ConfirmUnlock { user: String },
    /// Permissioned to the Emissions Controller contract.
    /// Cancel unlocking for a specific user.
    /// This is used on IBC failures/timeouts.
    /// Allows users to retry unlocking.
    ForceRelock { user: String },
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
    /// Astroport Emissions Controller contract
    pub emissions_controller: Addr,
}

#[derive(Copy)]
#[cw_serde]
pub struct UnlockStatus {
    /// The timestamp when a lock will be unlocked.
    pub end: u64,
    /// Whether The Hub confirmed unlocking
    pub hub_confirmed: bool,
}

#[cw_serde]
pub struct LockInfoResponse {
    /// The total number of xASTRO tokens that were deposited in the vxASTRO position
    pub amount: Uint128,
    /// Unlocking status. None for positions in Locked state
    pub unlock_status: Option<UnlockStatus>,
}
