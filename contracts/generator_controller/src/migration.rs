use astroport_governance::generator_controller::MigrateMsg;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, StdError};
use cw_storage_plus::Item;

/// This structure describes the main control config of generator controller.
#[cw_serde]
pub struct ConfigV100 {
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
}

/// Stores the contract config(V1.0.0) at the given key
pub const CONFIGV100: Item<ConfigV100> = Item::new("config");

/// This structure stores the core parameters for the Generator contract.
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
    /// Max number of blacklisted voters can be removed
    pub blacklisted_voters_limit: Option<u32>,
}

/// Stores the contract config(V1.1.0) at the given key
pub const CONFIGV110: Item<ConfigV110> = Item::new("config");

/// Migrate config to V1.1.0
pub fn migrate_configs_to_v110(deps: &mut DepsMut, msg: &MigrateMsg) -> Result<(), StdError> {
    let cfg_100 = CONFIGV100.load(deps.storage)?;

    let mut cfg = ConfigV110 {
        owner: cfg_100.owner,
        escrow_addr: cfg_100.escrow_addr,
        generator_addr: cfg_100.generator_addr,
        factory_addr: cfg_100.factory_addr,
        pools_limit: cfg_100.pools_limit,
        blacklisted_voters_limit: None,
    };

    if let Some(blacklisted_voters_limit) = msg.blacklisted_voters_limit {
        cfg.blacklisted_voters_limit = Some(blacklisted_voters_limit);
    }

    CONFIGV110.save(deps.storage, &cfg)?;

    Ok(())
}
