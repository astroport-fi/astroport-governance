use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::common::OwnershipProposal;

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map, U64Key};

/// ## Description
/// This structure describes the main control config of distributor contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Owner address
    pub owner: Addr,
    /// Fee token address
    pub astro_token: Addr,
    /// VotingEscrow contract address
    pub voting_escrow_addr: Addr,
    /// Max limit of addresses to claim rewards in single call
    pub claim_many_limit: u64,
    /// Is reward claiming disabled: for emergency
    pub is_claim_disabled: bool,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Contains information about distributed rewards per week.
pub const REWARDS_PER_WEEK: Map<U64Key, Uint128> = Map::new("rewards_per_week");

/// ## Description
/// Contains information about the last week of commission issuance.
pub const LAST_CLAIM_PERIOD: Map<Addr, u64> = Map::new("last_claim_period");

/// ## Description
/// Contains proposal for change ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
