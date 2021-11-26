use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

use astroport_governance::astro_vesting::{AllocationParams, AllocationStatus, Config, State};

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");
pub const PARAMS: Map<&Addr, AllocationParams> = Map::new("params");
pub const STATUS: Map<&Addr, AllocationStatus> = Map::new("status");
