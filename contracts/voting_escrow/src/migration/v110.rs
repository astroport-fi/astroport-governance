use astroport::asset::addr_validate_to_lower;
use cosmwasm_std::{Addr, Decimal, DepsMut, Env, StdError, StdResult};
use cw20::{Cw20QueryMsg, MinterResponse};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::migration::Migration;
use crate::state::{Config, WithdrawalParams, CONFIG, WITHDRAWAL_PARAMS};

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
}

pub struct MigrationV110;

impl Migration<ParamsV110> for MigrationV110 {
    fn handle_migration(deps: DepsMut, _env: Env, params: ParamsV110) -> StdResult<()> {
        let config = CONFIG_V100.load(deps.storage)?;
        // Accept values within [0,1] limit.
        if params.max_exit_penalty > Decimal::one() {
            return Err(StdError::generic_err("Max exit penalty should be <= 1"));
        }
        let slashed_fund_receiver = params
            .slashed_fund_receiver
            .map(|addr| addr_validate_to_lower(deps.api, &addr))
            .transpose()?;

        CONFIG.save(
            deps.storage,
            &Config {
                owner: config.owner,
                guardian_addr: config.guardian_addr,
                deposit_token_addr: config.deposit_token_addr.clone(),
                max_exit_penalty: params.max_exit_penalty,
                slashed_fund_receiver,
            },
        )?;

        // Initialize early withdraw parameters
        let xastro_minter_resp: MinterResponse = deps
            .querier
            .query_wasm_smart(&config.deposit_token_addr, &Cw20QueryMsg::Minter {})?;
        let staking_config: astroport::staking::ConfigResponse = deps.querier.query_wasm_smart(
            &xastro_minter_resp.minter,
            &astroport::staking::QueryMsg::Config {},
        )?;
        WITHDRAWAL_PARAMS.save(
            deps.storage,
            &WithdrawalParams {
                astro_addr: staking_config.deposit_token_addr,
                staking_addr: Addr::unchecked(xastro_minter_resp.minter),
            },
        )?;

        Ok(())
    }
}
