use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::common::OwnershipProposal;

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map, U64Key};

/// ## Description
/// This structure describes the main control config of distributor contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// address of ownership
    pub owner: Addr,
    /// Fee token address
    pub astro_token: Addr,
    /// VotingEscrow contract address
    pub voting_escrow_addr: Addr,
    /// Address to transfer `token` balance to, if this contract is killed
    pub emergency_return_addr: Addr,
    /// Period time for fee distribution to start
    pub start_time: u64,
    pub last_token_time: u64,
    pub time_cursor: u64,
    /// Flag which defines whether checkpoint token is enabled or not for everyone.
    pub checkpoint_token_enabled: bool,
    pub max_limit_accounts_of_claim: u64,
    pub token_last_balance: Uint128,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores config at the given key. Contains information about the amount of commission accrued
/// to the user in Astro tokens.
pub const CHECKPOINT_TOKEN: Map<U64Key, Uint128> = Map::new("checkpoint_token");

/// ## Description
/// Stores config at the given key. Contains information about distributed tokens per week.
pub const TOKENS_PER_WEEK: Map<U64Key, Uint128> = Map::new("tokens_per_week");

/// ## Description
/// Stores config at the given key. Contains information about the last week of commission issuance.
pub const TIME_CURSOR_OF: Map<Addr, u64> = Map::new("time_cursor_of");

/// ## Description
/// Contains proposal for change ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
