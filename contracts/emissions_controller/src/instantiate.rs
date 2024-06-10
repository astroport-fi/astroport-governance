use astroport::asset::validate_native_denom;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Addr, DepsMut, Env, MessageInfo, Reply, Response, StdError, SubMsg,
    SubMsgResponse, SubMsgResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::parse_instantiate_response_data;
use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::query::NeutronQuery;

use astroport_governance::emissions_controller::hub::{Config, TuneInfo};
use astroport_governance::emissions_controller::hub::{EmissionsState, HubInstantiateMsg};
use astroport_governance::emissions_controller::utils::query_incentives_addr;
use astroport_governance::voting_escrow;

use crate::error::ContractError;
use crate::state::{CONFIG, POOLS_WHITELIST, TUNE_INFO};
use crate::utils::{get_epoch_start, get_xastro_rate_and_share};

/// Contract name that is used for migration.
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// ID for the vxastro contract instantiate reply
pub const INSTANTIATE_VXASTRO_REPLY_ID: u64 = 1;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    _info: MessageInfo,
    msg: HubInstantiateMsg,
) -> Result<Response<NeutronMsg>, ContractError> {
    let deps = deps.into_empty();

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    validate_native_denom(&msg.astro_denom)?;

    let factory = deps.api.addr_validate(&msg.factory)?;

    let staking =
        if msg.xastro_denom.starts_with("factory/") && msg.xastro_denom.ends_with("/xASTRO") {
            deps.api
                .addr_validate(msg.xastro_denom.split('/').nth(1).unwrap())
        } else {
            Err(StdError::generic_err(format!(
                "Invalid xASTRO denom {}",
                msg.xastro_denom
            )))
        }?;

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        assembly: deps.api.addr_validate(&msg.assembly)?,
        vxastro: Addr::unchecked(""),
        incentives_addr: query_incentives_addr(deps.querier, &factory)?,
        factory,
        astro_denom: msg.astro_denom.to_string(),
        pools_per_outpost: msg.pools_per_outpost,
        whitelisting_fee: msg.whitelisting_fee,
        fee_receiver: deps.api.addr_validate(&msg.fee_receiver)?,
        whitelist_threshold: msg.whitelist_threshold,
        emissions_multiple: msg.emissions_multiple,
        max_astro: msg.max_astro,
        staking,
        xastro_denom: msg.xastro_denom.clone(),
    };
    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    // Query dynamic emissions curve state
    let (xastro_rate, _) = get_xastro_rate_and_share(deps.querier, &config)?;

    // Set tune_ts just for safety so the first tuning could happen in 2 weeks
    TUNE_INFO.save(
        deps.storage,
        &TuneInfo {
            tune_ts: get_epoch_start(env.block.time.seconds()),
            pools_grouped: Default::default(),
            outpost_emissions_statuses: Default::default(),
            emissions_state: EmissionsState {
                xastro_rate,
                collected_astro: msg.collected_astro,
                ema: msg.ema,
                emissions_amount: Uint128::zero(),
            },
        },
        env.block.time.seconds(),
    )?;

    // Instantiate vxASTRO contract
    let init_vxastro_msg = WasmMsg::Instantiate {
        admin: Some(msg.owner),
        code_id: msg.vxastro_code_id,
        msg: to_json_binary(&voting_escrow::InstantiateMsg {
            deposit_denom: msg.xastro_denom.to_string(),
            emissions_controller: env.contract.address.to_string(),
            marketing: msg.vxastro_marketing_info,
        })?,
        funds: vec![],
        label: "Vote Escrowed xASTRO".to_string(),
    };

    POOLS_WHITELIST.save(deps.storage, &vec![])?;

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
