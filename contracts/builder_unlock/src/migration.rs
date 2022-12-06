use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes a migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub max_allocations_amount: Uint128,
}

/// This structure stores the total and the remaining amount of ASTRO to be unlocked by all accounts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateV100 {
    /// Amount of ASTRO tokens deposited into the contract
    pub total_astro_deposited: Uint128,
    /// Currently available ASTRO tokens that still need to be unlocked and/or withdrawn
    pub remaining_astro_tokens: Uint128,
}

pub const STATEV100: Item<StateV100> = Item::new("state");

/// This structure stores the parameters used to describe the status of an allocation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AllocationStatusV100 {
    /// Amount of ASTRO already withdrawn
    pub astro_withdrawn: Uint128,
}

pub const STATUSV100: Map<&Addr, AllocationStatusV100> = Map::new("status");

/// This structure stores general parameters for the builder unlock contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigV100 {
    /// Account that can create new unlock schedules
    pub owner: Addr,
    /// Address of ASTRO token
    pub astro_token: Addr,
}

/// Stores the contract configuration
pub const CONFIGV100: Item<ConfigV100> = Item::new("config");
