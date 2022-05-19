use crate::astroport::common::OwnershipProposal;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

use astroport_governance::builder_unlock::{AllocationParams, AllocationStatus, Config, State};

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
