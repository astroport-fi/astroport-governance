#![cfg(not(tarpaulin_include))]

use astroport_governance::emissions_controller::hub::{
    AstroPoolConfig, OutpostInfo, OutpostParams,
};
use astroport_governance::utils::determine_ics20_escrow_address;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{entry_point, DepsMut, Empty, Env, Order, Response, StdResult};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::Map;

use crate::error::ContractError;
use crate::instantiate::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::state::OUTPOSTS;

#[cw_serde]
struct OldOutpostParams {
    pub emissions_controller: String,
    pub voting_channel: String,
    pub ics20_channel: String,
}

#[cw_serde]
struct OldOutpostInfo {
    pub params: Option<OldOutpostParams>,
    pub astro_denom: String,
    pub astro_pool_config: Option<AstroPoolConfig>,
}

const OLD_OUTPOSTS: Map<&str, OldOutpostInfo> = Map::new("outposts");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        CONTRACT_NAME => match contract_version.version.as_ref() {
            "1.0.0" | "1.0.1" => {
                let old_outposts = OLD_OUTPOSTS
                    .range(deps.storage, None, None, Order::Ascending)
                    .collect::<StdResult<Vec<_>>>()?;

                for (prefix, old_outpost) in old_outposts {
                    let params = old_outpost
                        .params
                        .map(|params| -> StdResult<_> {
                            Ok(OutpostParams {
                                emissions_controller: params.emissions_controller,
                                voting_channel: params.voting_channel,
                                escrow_address: determine_ics20_escrow_address(
                                    deps.api,
                                    "transfer",
                                    &params.ics20_channel,
                                )?,
                                ics20_channel: params.ics20_channel,
                            })
                        })
                        .transpose()?;

                    OUTPOSTS.save(
                        deps.storage,
                        &prefix,
                        &OutpostInfo {
                            params,
                            astro_denom: old_outpost.astro_denom,
                            astro_pool_config: old_outpost.astro_pool_config,
                            jailed: false,
                        },
                    )?;
                }

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
