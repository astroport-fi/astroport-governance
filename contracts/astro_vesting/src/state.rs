use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

use astroport_governance::astro_vesting::{AllocationParams, AllocationStatus, Config};

pub const CONFIG: Item<Config<Addr>> = Item::new("config");
pub const PARAMS: Map<&Addr, AllocationParams> = Map::new("params");
pub const STATUS: Map<&Addr, AllocationStatus> = Map::new("status");
