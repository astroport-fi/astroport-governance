use crate::marketing_validation::validate_whitelist_links;
use astroport::asset::addr_validate_to_lower;
use cosmwasm_std::{Addr, Decimal, DepsMut, Env, StdError, StdResult};
use cw20::{Cw20QueryMsg, MinterResponse};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::migration::Migration;
use crate::state::{Config, CONFIG};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct ConfigV100 {
    pub owner: Addr,
    pub guardian_addr: Addr,
    pub deposit_token_addr: Addr,
}

pub const CONFIG_V100: Item<ConfigV100> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct ParamsV110 {
    pub max_exit_penalty: Decimal,
    pub slashed_fund_receiver: Option<String>,
    pub logo_urls_whitelist: Vec<String>,
}

pub struct MigrationV110;

impl Migration<ParamsV110> for MigrationV110 {
    fn handle_migration(deps: DepsMut, _env: Env, params: ParamsV110) -> StdResult<()> {
        let configv100 = CONFIG_V100.load(deps.storage)?;
        // Accept values within [0,1] limit.
        if params.max_exit_penalty > Decimal::one() {
            return Err(StdError::generic_err("Max exit penalty should be <= 1"));
        }
        let slashed_fund_receiver = params
            .slashed_fund_receiver
            .map(|addr| addr_validate_to_lower(deps.api, &addr))
            .transpose()?;
        // Initialize early withdraw parameters
        let xastro_minter_resp: MinterResponse = deps
            .querier
            .query_wasm_smart(&configv100.deposit_token_addr, &Cw20QueryMsg::Minter {})?;
        let staking_config: astroport::staking::ConfigResponse = deps.querier.query_wasm_smart(
            &xastro_minter_resp.minter,
            &astroport::staking::QueryMsg::Config {},
        )?;
        // Validate logo urls whitelist
        validate_whitelist_links(&params.logo_urls_whitelist)
            .map_err(|_| StdError::generic_err("Some links are invalid"))?;

        CONFIG.save(
            deps.storage,
            &Config {
                owner: configv100.owner,
                guardian_addr: Some(configv100.guardian_addr),
                deposit_token_addr: configv100.deposit_token_addr,
                max_exit_penalty: params.max_exit_penalty,
                slashed_fund_receiver,
                astro_addr: staking_config.deposit_token_addr,
                xastro_staking_addr: addr_validate_to_lower(deps.api, &xastro_minter_resp.minter)?,
                logo_urls_whitelist: params.logo_urls_whitelist,
            },
        )?;

        Ok(())
    }
}
