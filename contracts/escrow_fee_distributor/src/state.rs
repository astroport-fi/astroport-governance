use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::astroport::common::OwnershipProposal;

use astroport_governance::U64Key;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

/// This structure stores the main parameters for the distributor contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// ASTRO token address
    pub astro_token: Addr,
    /// vxASTRO contract address
    pub voting_escrow_addr: Addr,
    /// Max limit of addresses that can claim rewards in a single call
    pub claim_many_limit: u64,
    /// Whether reward claiming is disabled
    pub is_claim_disabled: bool,
}

/// Stores the contract config at the given key.
pub const CONFIG: Item<Config> = Item::new("config");
/// Contains information about weekly distributed rewards.
pub const REWARDS_PER_WEEK: Map<U64Key, Uint128> = Map::new("rewards_per_week");
/// Contains information about the last week of reward issuance.
pub const LAST_CLAIM_PERIOD: Map<Addr, u64> = Map::new("last_claim_period");
/// Contains the proposal to change contract ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
