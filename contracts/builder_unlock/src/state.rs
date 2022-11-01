use astroport::common::OwnershipProposal;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

use ap_builder_unlock::{AllocationParams, AllocationStatus, Config};

/// This structure stores the total and the remaining amount of ASTRO to be unlocked by all accounts.
#[cw_serde]
#[derive(Default)]
pub struct State {
    /// Amount of ASTRO tokens deposited into the contract
    pub total_astro_deposited: Uint128,
    /// Currently available ASTRO tokens that still need to be unlocked and/or withdrawn
    pub remaining_astro_tokens: Uint128,
    /// Amount of ASTRO tokens deposited into the contract but not assigned to an allocation
    pub unallocated_tokens: Uint128,
}

/// Stores the contract configuration
pub const CONFIG: Item<Config> = Item::new("config");
/// Stores global unlcok state such as the total amount of ASTRO tokens still to be distributed
pub const STATE: Item<State> = Item::new("state");
/// Allocation parameters for each unlock recipient
pub const PARAMS: Map<&Addr, AllocationParams> = Map::new("params");
/// The status of each unlock schedule
pub const STATUS: Map<&Addr, AllocationStatus> = Map::new("status");
/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
