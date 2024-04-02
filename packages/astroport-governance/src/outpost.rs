use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use cw20::Cw20ReceiveMsg;

use crate::assembly::ProposalVoteOption;

/// Holds the parameters used for creating an Outpost contract
#[cw_serde]
pub struct InstantiateMsg {
    /// The contract owner
    pub owner: String,
    /// The address of the xASTRO token contract on the Outpost
    pub xastro_token_addr: String,
    /// The address of the vxASTRO lite contract on the Outpost
    pub vxastro_token_addr: String,
    /// The address of the Hub contract on the Hub chain
    pub hub_addr: String,
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
    /// Update parameters in the Outpost contract. Only the owner is allowed to
    /// update the config
    UpdateConfig {
        /// The new Hub address
        hub_addr: Option<String>,
        /// The new Hub IBC channel
        hub_channel: Option<String>,
        /// The timeout in seconds for IBC packets
        ibc_timeout_seconds: Option<u64>,
    },
    /// Cast a vote on an Assembly proposal from an Outpost
    CastAssemblyVote {
        /// The ID of the proposal to vote on
        proposal_id: u64,
        /// The vote choice
        vote: ProposalVoteOption,
    },
    /// Cast a vote during an emissions voting period
    CastEmissionsVote {
        /// The votes in the format (pool address, percent of voting power)
        votes: Vec<(String, u16)>,
    },
    /// Kick an unlocked voter's voting power from the Generator Controller lite
    KickUnlocked {
        /// The address of the user to kick
        user: Addr,
    },
    /// Kick a blacklisted voter's voting power from the Generator Controller lite
    KickBlacklisted {
        /// The address of the user that has been blacklisted
        user: Addr,
    },
    /// Withdraw stuck funds from the Hub in case of specific IBC failures
    WithdrawHubFunds {},
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
    /// Unstake xASTRO from the Hub and return the ASTRO to the sender
    Unstake {},
}

/// Describes the query messages available in the contract
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the config of the Outpost
    #[returns(Config)]
    Config {},
    #[returns(ProposalVoteOption)]
    ProposalVoted { proposal_id: u64, user: String },
}

/// The config of the Outpost
#[cw_serde]
pub struct Config {
    /// The owner of the contract
    pub owner: Addr,
    /// The address of the Hub contract on the Hub chain    
    pub hub_addr: String,
    /// The channel used to communicate with the Hub
    pub hub_channel: Option<String>,
    /// The address of the xASTRO token contract on the Outpost
    pub xastro_token_addr: Addr,
    /// The address of the vxASTRO lite contract on the Outpost
    pub vxastro_token_addr: Addr,
    /// The timeout in seconds for IBC packets
    pub ibc_timeout_seconds: u64,
}
