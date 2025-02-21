use std::ops::RangeInclusive;

use astroport::asset::{Asset, AssetInfo};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, Uint128};

/// Validation limits for rewards number per pool
pub const REWARDS_AMOUNT_LIMITS: RangeInclusive<u8> = 1..=30u8;

/// Validation constraints for max allowed gas limit per one tribute token transfer.
/// Canonical cw20 transfer gas is typically 130-170k.
/// Native coin bank transfer is 80-90k.
/// Token factory token, for example, xASTRO, with bank hook is ~300k.
/// Setting to 600k seems reasonable for most cases.
/// If token transfer hits this gas limit, reward will be considered as claimed while in reality
/// it will be stuck in the contract.
pub const TOKEN_TRANSFER_GAS_LIMIT: RangeInclusive<u64> = 400_000..=1_500_000u64;

#[cw_serde]
pub struct TributeFeeInfo {
    pub fee: Coin,
    pub fee_collector: Addr,
}

#[cw_serde]
pub struct TributeInfo {
    /// Total number of tributes allocated
    pub allocated: Uint128,
    /// Number of tokens yet to be claimed
    pub available: Uint128,
}

#[cw_serde]
pub struct Config {
    /// Contract owner can update config and deregister tributes
    pub owner: Addr,
    /// Emissions controller contract address
    pub emissions_controller: Addr,
    /// Anti-spam fee for adding tributes
    pub tribute_fee_info: TributeFeeInfo,
    /// Maximum number of tributes per pool
    pub rewards_limit: u8,
    /// Initial epoch start timestamp
    pub initial_epoch: u64,
    /// Max allowed gas limit per one tribute token transfer.
    /// If token transfer hits this gas limit, reward will be considered as claimed while in reality
    /// it will be stuck in the contract.
    pub token_transfer_gas_limit: u64,
}

#[cw_serde]
pub struct InstantiateMsg {
    /// Contract owner can update config and deregister tributes
    pub owner: String,
    /// Emissions controller contract address
    pub emissions_controller: String,
    /// Anti-spam fee for adding tributes
    pub tribute_fee_info: TributeFeeInfo,
    /// Maximum number of tributes per pool
    pub rewards_limit: u8,
    /// Token transfer gas limit
    pub token_transfer_gas_limit: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Permissionless endpoint to add tributes to a given LP token.
    /// Caller must pay an anti-spam fee if this reward doesn't exist yet.
    /// If such AssetInfo already exists on a given LP token, it will be extended with additional amount.
    /// Tribute reward can be either a native token or a CW20 token.
    /// The caller must approve CW20 token to pull specified amount.
    /// You can add tribute only for upcoming epoch.
    AddTribute { lp_token: String, asset: Asset },
    /// Claims all tributes for a caller address.
    /// Optional receiver address to send claimed tributes.
    Claim { receiver: Option<String> },
    /// Permissioned to a contract owner. Allows removing tribute from a given LP token only for upcoming epoch.
    DeregisterTribute {
        /// LP token to remove tribute from.
        lp_token: String,
        /// Asset to remove from tributes.
        asset_info: AssetInfo,
        /// Receiver address to send removed tributes.
        receiver: String,
    },
    /// Permissioned to a contract owner. Allows updating tribute contract configuration.
    UpdateConfig {
        /// Anti-spam fee for adding tributes
        tribute_fee_info: Option<TributeFeeInfo>,
        /// Maximum number of tributes per pool
        rewards_limit: Option<u8>,
        /// Token transfer gas limit
        token_transfer_gas_limit: Option<u64>,
    },
    /// ProposeNewOwner proposes a new owner for the contract
    ProposeNewOwner {
        /// Newly proposed contract owner
        new_owner: String,
        /// The timestamp when the contract ownership change expires
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the latest contract ownership transfer proposal
    DropOwnershipProposal {},
    /// ClaimOwnership allows the newly proposed owner to claim contract ownership
    ClaimOwnership {},
    // TODO: handle possible orphaned tributes
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns contract config
    #[returns(Config)]
    Config {},
    /// Returns whether fee is expected for adding tributes
    #[returns(bool)]
    IsFeeExpected {
        lp_token: String,
        asset_info: AssetInfo,
    },
    /// Returns vector of tributes for a given LP token.
    #[returns(Vec<Asset>)]
    QueryPoolTributes {
        /// Epoch timestamp. Enough to provide any timestamp within the epoch.
        /// If None, it will return the current epoch tributes.
        /// NOTE: Tribute epoch matches epoch when rewards started being distributed.
        /// It doesn't match the preceding epoch when rewards were added!.
        epoch_ts: Option<u64>,
        /// LP token to query tributes for.
        lp_token: String,
    },
    /// Returns vector of all tributes for a given epoch. Item value (lp token, tribute asset).
    #[returns(Vec<(String, Asset)>)]
    QueryAllEpochTributes {
        /// Epoch timestamp. Enough to provide any timestamp within the epoch.
        /// If None, it returns the current epoch tributes.
        /// NOTE: Tribute epoch matches epoch when rewards started being distributed.
        /// It doesn't match the preceding epoch when rewards were added!.
        epoch_ts: Option<u64>,
        /// Start after is pagination parameter where value is (lp token, reward asset info).
        start_after: Option<(String, AssetInfo)>,
        /// Limits the number of returned results.
        limit: Option<u32>,
    },
    /// Returns vector of claimable tributes for a given address.
    #[returns(Vec<Asset>)]
    SimulateClaim {
        /// Address to simulate claim for.
        address: String,
    },
}
