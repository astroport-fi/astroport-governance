use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;

/// Holds the parameters used for creating a Hub contract
#[cw_serde]
pub struct InstantiateMsg {
    /// The contract owner
    pub owner: String,
    /// The address of the Assembly contract on the Hub
    pub assembly_addr: String,
    /// The address of the CW20-ICS20 contract on the Hub that supports
    /// memo handling
    pub cw20_ics20_addr: String,
    /// The address of the xASTRO staking contract on the Hub
    pub staking_addr: String,
    /// The address of the generator controller contract on the Hub
    pub generator_controller_addr: String,
    /// The timeout in seconds for IBC packets
    pub ibc_timeout_seconds: u64,
}

/// The contract migration message
/// We currently take no arguments for migrations
#[cw_serde]
pub struct MigrateMsg {}

/// Describes the execute messages available in the contract
#[cw_serde]
pub enum ExecuteMsg {
    /// Receive a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// Update parameters in the Hub contract. Only the owner is allowed to
    /// update the config
    UpdateConfig {
        /// The timeout in seconds for IBC packets
        ibc_timeout_seconds: Option<u64>,
    },
    /// Add a new Outpost to the Hub. Only allowed Outposts can send IBC messages
    AddOutpost {
        /// The remote contract address of the Outpost to add
        outpost_addr: String,
        /// The channel connecting us to the Outpost
        outpost_channel: String,
        /// The channel to use for CW20-ICS20 IBC transfers
        cw20_ics20_channel: String,
    },
    /// Remove an Outpost from the Hub
    RemoveOutpost {
        /// The remote contract address of the Outpost to remove
        outpost_addr: String,
    },
    /// Propose a new owner for the contract
    ProposeNewOwner { new_owner: String, expires_in: u64 },
    /// Remove the ownership transfer proposal
    DropOwnershipProposal {},
    /// Claim contract ownership
    ClaimOwnership {},
}

/// Messages handled via CW20 transfers
#[cw_serde]
pub enum Cw20HookMsg {
    /// Handles instructions received via an IBC transfer memo in the
    /// CW20-ICS20 contract
    OutpostMemo {
        /// The channel the memo was received on
        channel: String,
        /// The original sender of the packet on the outpost
        sender: String,
        /// The original intended receiver of the packet on the Hub
        receiver: String,
        /// The memo containing the JSON to handle
        memo: String,
    },
    /// Handle failed CW20 IBC transfers
    TransferFailure {
        // The original sender where the funds should be returned to
        receiver: String,
    },
}

/// Describes the query messages available in the contract
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the config of the Hub
    #[returns(Config)]
    Config {},
    /// Returns the balance of funds held for a user
    #[returns(HubBalance)]
    UserFunds { user: Addr },
    /// Returns the list of the current Outposts on the Hub
    #[returns(Vec<OutpostConfig>)]
    Outposts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    // TODO: change to track by nano seconds
    /// Returns the current balance of xASTRO minted via a specific Outpost channel
    #[returns(HubBalance)]
    ChannelBalanceAt { channel: String, timestamp: Uint64 },
    /// Returns the total balance of all xASTRO minted via Outposts
    #[returns(HubBalance)]
    TotalChannelBalancesAt { timestamp: Uint64 },
}

/// The config of the Hub
#[cw_serde]
pub struct Config {
    /// The owner of the contract
    pub owner: Addr,
    /// The address of the Assembly contract on the Hub
    pub assembly_addr: Addr,
    /// The address of the CW20-ICS20 contract on the Hub that supports memo
    /// handling
    pub cw20_ics20_addr: Addr,
    /// The address of the ASTRO token contract on the Hub
    pub token_addr: Addr,
    /// The address of the xASTRO token contract on the Hub
    pub xtoken_addr: Addr,
    /// The address of the staking contract on the Hub
    pub staking_addr: Addr,
    /// The address of the generator controller contract on the Hub
    pub generator_controller_addr: Addr,
    /// The timeout in seconds for IBC packets
    pub ibc_timeout_seconds: u64,
}

/// A response containing the Outpost address and channels
#[cw_serde]
pub struct OutpostConfig {
    /// The address of the Outpost contract on another chain
    pub address: String,
    /// The channel connecting the Hub contract with that Outpost contract
    pub channel: String,
    /// The CS20-ICS20 channel ASTRO is transferred through
    pub cw20_ics20_channel: String,
}

/// A response containing the balance of a channel or user on the Hub
#[cw_serde]
pub struct HubBalance {
    /// The balance of the user or channel
    pub balance: Uint128,
}
