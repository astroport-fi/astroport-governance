use crate::marketing_validation::validate_whitelist_links;

use cosmwasm_std::{Addr, Decimal, DepsMut, Env, StdError, StdResult};

use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::migration::Migration;
use crate::state::{Config, CONFIG};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigV120 {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// Address that can only blacklist vxASTRO stakers and remove their governance power
    pub guardian_addr: Option<Addr>,
    /// The xASTRO token contract address
    pub deposit_token_addr: Addr,
    /// The maximum % of staked xASTRO that is confiscated upon an early exit
    pub max_exit_penalty: Decimal,
    /// The address that receives slashed ASTRO (slashed xASTRO is burned in order to claim ASTRO)
    pub slashed_fund_receiver: Option<Addr>,
    /// The address of $ASTRO
    pub astro_addr: Addr,
    /// The address of $xASTRO staking contract
    pub xastro_staking_addr: Addr,
}

pub const CONFIG_V120: Item<ConfigV120> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct ParamsV130 {
    pub logo_urls_whitelist: Vec<String>,
}

pub struct MigrationV130;

impl Migration<ParamsV130> for MigrationV130 {
    fn handle_migration(deps: DepsMut, _: Env, params: ParamsV130) -> StdResult<()> {
        let configv120 = CONFIG_V120.load(deps.storage)?;
        // Validate logo urls whitelist
        validate_whitelist_links(&params.logo_urls_whitelist)
            .map_err(|_err| StdError::generic_err("Some links are invalid"))?;

        CONFIG.save(
            deps.storage,
            &Config {
                owner: configv120.owner,
                guardian_addr: configv120.guardian_addr,
                deposit_token_addr: configv120.deposit_token_addr,
                max_exit_penalty: configv120.max_exit_penalty,
                slashed_fund_receiver: configv120.slashed_fund_receiver,
                astro_addr: configv120.astro_addr,
                xastro_staking_addr: configv120.xastro_staking_addr,
                logo_urls_whitelist: params.logo_urls_whitelist,
            },
        )?;

        Ok(())
    }
}
