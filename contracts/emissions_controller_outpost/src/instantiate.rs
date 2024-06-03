use astroport::asset::validate_native_denom;
use astroport_governance::emissions_controller::outpost::OutpostInstantiateMsg;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Addr, DepsMut, Env, MessageInfo, Reply, Response, StdError, SubMsg,
    SubMsgResponse, SubMsgResult, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::parse_instantiate_response_data;

use astroport_governance::emissions_controller::outpost::Config;
use astroport_governance::voting_escrow;

use crate::error::ContractError;
use crate::state::CONFIG;

/// Contract name that is used for migration.
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// ID for the vxastro contract instantiate reply
pub const INSTANTIATE_VXASTRO_REPLY_ID: u64 = 1;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: OutpostInstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    validate_native_denom(&msg.astro_denom)?;

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        vxastro: Addr::unchecked(""),
        astro_denom: msg.astro_denom,
        factory: deps.api.addr_validate(&msg.factory)?,
        // Contract owner is responsible for setting a channel via UpdateConfig
        voting_ibc_channel: "".to_string(),
        hub_emissions_controller: msg.hub_emissions_controller,
        ics20_channel: msg.ics20_channel,
    };

    CONFIG.save(deps.storage, &config)?;

    // Instantiate vxASTRO contract
    validate_native_denom(&msg.vxastro_deposit_denom)?;
    let init_vxastro_msg = WasmMsg::Instantiate {
        admin: Some(msg.owner),
        code_id: msg.vxastro_code_id,
        msg: to_json_binary(&voting_escrow::InstantiateMsg {
            deposit_denom: msg.vxastro_deposit_denom.to_string(),
            emissions_controller: env.contract.address.to_string(),
            marketing: msg.vxastro_marketing_info,
        })?,
        funds: vec![],
        label: "Vote Escrowed xASTRO".to_string(),
    };

    Ok(Response::default()
        .add_attribute("action", "instantiate_emissions_controller")
        .add_submessage(SubMsg::reply_on_success(
            init_vxastro_msg,
            INSTANTIATE_VXASTRO_REPLY_ID,
        )))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg {
        Reply {
            id: INSTANTIATE_VXASTRO_REPLY_ID,
            result:
                SubMsgResult::Ok(SubMsgResponse {
                    data: Some(data), ..
                }),
        } => {
            let vxastro_contract = parse_instantiate_response_data(&data)?.contract_address;

            CONFIG.update::<_, StdError>(deps.storage, |mut config| {
                ensure!(
                    config.vxastro == Addr::unchecked(""),
                    StdError::generic_err("vxASTRO contract is already set")
                );

                config.vxastro = Addr::unchecked(&vxastro_contract);
                Ok(config)
            })?;

            Ok(Response::new().add_attribute("vxastro", vxastro_contract))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}
