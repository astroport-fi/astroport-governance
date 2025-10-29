#![cfg(not(tarpaulin_include))]

use crate::error::ContractError;
use crate::instantiate::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::state::{CONFIG, POOLS_WHITELIST};
use astroport_governance::emissions_controller::hub::{Config, MigrateMsg};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Coin, Decimal, DepsMut, Env, Response, Uint128};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::Item;

#[cw_serde]
pub struct OldConfig {
    pub owner: Addr,
    pub assembly: Addr,
    pub vxastro: Addr,
    pub factory: Addr,
    pub astro_denom: String,
    pub xastro_denom: String,
    pub staking: Addr,
    pub incentives_addr: Addr,
    pub pools_per_outpost: u64,
    pub whitelisting_fee: Coin,
    pub fee_receiver: Addr,
    pub whitelist_threshold: Decimal,
    pub emissions_multiple: Decimal,
    pub max_astro: Uint128,
}

#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        CONTRACT_NAME => match contract_version.version.as_ref() {
            "1.1.0" | "1.1.1" | "1.2.0" | "1.2.1" | "1.3.0" => {
                let old_wl_iface = Item::<Vec<String>>::new("pools_whitelist");
                let old_whitelist: Vec<String> = old_wl_iface.load(deps.storage)?;
                old_wl_iface.remove(deps.storage);

                for pool in &old_whitelist {
                    POOLS_WHITELIST.save(deps.storage, pool, &false)?;
                }

                let old_config: OldConfig = Item::new("config").load(deps.storage)?;
                CONFIG.save(
                    deps.storage,
                    &Config {
                        owner: old_config.owner,
                        assembly: old_config.assembly,
                        vxastro: old_config.vxastro,
                        factory: old_config.factory,
                        astro_denom: old_config.astro_denom,
                        xastro_denom: old_config.xastro_denom,
                        staking: old_config.staking,
                        incentives_addr: old_config.incentives_addr,
                        pools_per_outpost: old_config.pools_per_outpost,
                        whitelisting_fee: old_config.whitelisting_fee,
                        fee_receiver: old_config.fee_receiver,
                        whitelist_threshold: old_config.whitelist_threshold,
                        emissions_multiple: old_config.emissions_multiple,
                        max_astro: old_config.max_astro,
                        liquidity_percent: msg.liquidity_percent,
                        allowed_spread_per_step: msg.allowed_spread_per_step,
                        unwhitelisting_enabled: false,
                    },
                )?;

                Ok(())
            }
            _ => Err(ContractError::MigrationError {}),
        },
        _ => Err(ContractError::MigrationError {}),
    }?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
