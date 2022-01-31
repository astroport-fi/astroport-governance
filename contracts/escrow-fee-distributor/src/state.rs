use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use astroport::common::OwnershipProposal;

use astroport_governance::escrow_fee_distributor::Claimed;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map, U64Key};

/// ## Description
/// This structure describes the main control config of distributor contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// address of ownership
    pub owner: Addr,
    /// Fee token address
    pub token: Addr,
    /// VotingEscrow contract address
    pub voting_escrow: Addr,
    /// Address to transfer `token` balance to, if this contract is killed
    pub emergency_return: Addr,
    /// Epoch time for fee distribution to start
    pub start_time: u64,
    pub last_token_time: u64,
    pub time_cursor: u64,
    /// makes it possible for everyone to call
    pub can_checkpoint_token: bool,
    pub is_killed: bool,
    pub max_limit_accounts_of_claim: u64,
}

/// ## Description
/// This structure describes the information of distributor contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct DistributorInfo {
    pub token_last_balance: Uint128,
    pub time_cursor_of: HashMap<Addr, u64>,
    pub user_period_of: HashMap<Addr, u64>,
    pub tokens_per_week: HashMap<u64, Uint128>,
    pub voting_supply_per_week: HashMap<u64, Uint128>,
}

/// ## Description
/// Stores config at the given key
pub const DISTRIBUTOR_INFO: Item<DistributorInfo> = Item::new("distributor_info");

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores config at the given key
pub const CHECKPOINT_TOKEN: Map<U64Key, Uint128> = Map::new("checkpoint_token");

/// ## Description
/// Stores config at the given key
pub const CLAIMED: Item<Vec<Claimed>> = Item::new("claimed");

/// ## Description
/// Contains proposal for change ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
