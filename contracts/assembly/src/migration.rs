#![cfg(not(tarpaulin_include))]

use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Addr, CosmosMsg, DepsMut, Env, Order, Response, StdResult, Uint128, Uint64};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::Map;

use astroport_governance::assembly::{Config, Proposal, ProposalStatus};
use astroport_governance::voting_escrow;

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::error::ContractError;
use crate::state::{CONFIG, PROPOSALS};

#[cw_serde]
pub struct MigrateMsg {
    pub vxastro_contract: String,
}

#[cw_serde]
pub struct OldProposal {
    pub proposal_id: Uint64,
    pub submitter: Addr,
    pub status: ProposalStatus,
    pub for_power: Uint128,
    pub outpost_for_power: Uint128,
    pub against_power: Uint128,
    pub outpost_against_power: Uint128,
    pub start_block: u64,
    pub start_time: u64,
    pub end_block: u64,
    pub delayed_end_block: u64,
    pub expiration_block: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub messages: Vec<CosmosMsg>,
    pub deposit_amount: Uint128,
    pub ibc_channel: Option<String>,
    pub total_voting_power: Uint128,
}

const OLD_PROPOSALS: Map<u64, OldProposal> = Map::new("proposals");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        CONTRACT_NAME => match contract_version.version.as_ref() {
            "2.0.0" => {
                // vxastro and emissions_controller are optional fields,
                // thus old config can be deserialized to new config
                let config = CONFIG.load(deps.storage)?;

                let emissions_controller = deps
                    .querier
                    .query_wasm_smart::<voting_escrow::Config>(
                        &msg.vxastro_contract,
                        &voting_escrow::QueryMsg::Config {},
                    )?
                    .emissions_controller;

                CONFIG.save(
                    deps.storage,
                    &Config {
                        vxastro_contract: Some(Addr::unchecked(msg.vxastro_contract)),
                        emissions_controller: Some(emissions_controller),
                        ..config
                    },
                )?;

                let proposals = OLD_PROPOSALS
                    .range(deps.storage, None, None, Order::Ascending)
                    .collect::<StdResult<Vec<_>>>()?;

                proposals.into_iter().try_for_each(|(id, old_proposal)| {
                    let proposal = Proposal {
                        proposal_id: old_proposal.proposal_id,
                        submitter: old_proposal.submitter,
                        status: old_proposal.status,
                        for_power: old_proposal.for_power,
                        against_power: old_proposal.against_power,
                        start_block: old_proposal.start_block,
                        start_time: old_proposal.start_time,
                        end_block: old_proposal.end_block,
                        delayed_end_block: old_proposal.delayed_end_block,
                        expiration_block: old_proposal.expiration_block,
                        title: old_proposal.title,
                        description: old_proposal.description,
                        link: old_proposal.link,
                        messages: old_proposal.messages,
                        deposit_amount: old_proposal.deposit_amount,
                        ibc_channel: old_proposal.ibc_channel,
                        total_voting_power: old_proposal.total_voting_power,
                    };
                    PROPOSALS
                        .save(deps.storage, id, &proposal)
                        .map_err(ContractError::Std)
                })
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
