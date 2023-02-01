use crate::state::CONFIG;
use astroport_governance::generator_controller::ConfigResponse;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, DepsMut, StdError};
use cw_storage_plus::Item;

/// This structure describes the main control config of generator controller.
#[cw_serde]
pub struct ConfigV110 {
    /// contract address that used for settings control
    pub owner: Addr,
    /// The vxASTRO token contract address
    pub escrow_addr: Addr,
    /// Generator contract address
    pub generator_addr: Addr,
    /// Factory contract address
    pub factory_addr: Addr,
    /// Max number of pools that can receive an ASTRO allocation
    pub pools_limit: u64,
    /// Max number of blacklisted voters which can be removed
    pub blacklisted_voters_limit: Option<u32>,
    /// Main pool that will receive a minimum amount of ASTRO emissions
    pub main_pool: Option<Addr>,
    /// The minimum percentage of ASTRO emissions that main pool should get every block
    pub main_pool_min_alloc: Decimal,
}

/// Stores the contract config(V1.1.0) at the given key
pub const CONFIG_V110: Item<ConfigV110> = Item::new("config");

/// Migrate config to V1.2.0
pub fn migrate_configs_to_v120(deps: &mut DepsMut) -> Result<(), StdError> {
    let cfg_110 = CONFIG_V110.load(deps.storage)?;

    let cfg = ConfigResponse {
        owner: cfg_110.owner,
        escrow_addr: cfg_110.escrow_addr,
        generator_addr: cfg_110.generator_addr,
        factory_addr: cfg_110.factory_addr,
        pools_limit: cfg_110.pools_limit,
        blacklisted_voters_limit: cfg_110.blacklisted_voters_limit,
        main_pool: cfg_110.main_pool,
        main_pool_min_alloc: cfg_110.main_pool_min_alloc,
        whitelisted_pools: vec![],
    };

    CONFIG.save(deps.storage, &cfg)?;

    Ok(())
}
