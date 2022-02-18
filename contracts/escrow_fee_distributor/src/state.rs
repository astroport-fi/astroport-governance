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
    /// Max limit of addresses to claim rewards in single call
    pub max_limit_accounts_of_claim: u64,
    /// Is reward claiming disabled: for emergency
    pub is_claim_disabled: bool,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores config at the given key. Contains information about distributed tokens per week.
pub const TOKENS_PER_WEEK: Map<U64Key, Uint128> = Map::new("tokens_per_week");

/// ## Description
/// Stores config at the given key. Contains information about the last week of commission issuance.
pub const CLAIM_FROM_PERIOD: Map<Addr, u64> = Map::new("claim_from_period");

/// ## Description
/// Contains proposal for change ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
