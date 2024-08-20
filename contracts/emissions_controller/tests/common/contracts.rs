use std::fmt::Debug;

use cosmwasm_schema::cw_serde;
use cosmwasm_schema::schemars::JsonSchema;
use cosmwasm_std::{
    Binary, CustomMsg, CustomQuery, DepsMut, Empty, Env, IbcPacketReceiveMsg, MessageInfo,
    Response, StdResult,
};
use cw_multi_test::{Contract, ContractWrapper};
use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::sudo::msg::RequestPacket;

use astroport_emissions_controller::ibc::ibc_packet_receive;
use astroport_emissions_controller::sudo::process_ibc_reply;

pub fn token_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ))
}

pub fn pair_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    )
}

pub fn vxastro_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(ContractWrapper::new_with_empty(
        astroport_voting_escrow::contract::execute,
        astroport_voting_escrow::contract::instantiate,
        astroport_voting_escrow::contract::query,
    ))
}

pub fn incentives_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(ContractWrapper::new_with_empty(
        astroport_incentives::execute::execute,
        astroport_incentives::instantiate::instantiate,
        astroport_incentives::query::query,
    ))
}

pub fn factory_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply_empty(astroport_factory::contract::reply),
    )
}

/// Extended version of [`TransferSudoMsg`] with additional variants to test IBC endpoints.
#[cw_serde]
pub enum TestSudoMsg {
    Response {
        request: RequestPacket,
        data: Binary,
    },
    Error {
        request: RequestPacket,
        details: String,
    },
    Timeout {
        request: RequestPacket,
    },
    IbcRecv(IbcPacketReceiveMsg),
}

fn emissions_controller_sudo(deps: DepsMut, env: Env, msg: TestSudoMsg) -> StdResult<Response> {
    match msg {
        TestSudoMsg::Response { request, .. } => {
            process_ibc_reply(deps.storage, env, request, false)
        }
        TestSudoMsg::Error { request, .. } | TestSudoMsg::Timeout { request } => {
            process_ibc_reply(deps.storage, env, request, true)
        }
        TestSudoMsg::IbcRecv(packet) => {
            let ibc_response = ibc_packet_receive(deps, env, packet).unwrap();
            Ok(Response::default()
                .add_attributes(ibc_response.attributes)
                .add_submessages(ibc_response.messages))
        }
    }
}

pub fn emissions_controller() -> Box<dyn Contract<NeutronMsg, NeutronQuery>> {
    Box::new(
        ContractWrapper::new(
            astroport_emissions_controller::execute::execute,
            astroport_emissions_controller::instantiate::instantiate,
            astroport_emissions_controller::query::query,
        )
        .with_sudo_empty(emissions_controller_sudo)
        .with_reply_empty(astroport_emissions_controller::instantiate::reply),
    )
}

pub fn staking_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_staking::contract::execute,
            astroport_staking::contract::instantiate,
            astroport_staking::contract::query,
        )
        .with_reply_empty(astroport_staking::contract::reply),
    )
}

pub fn tracker_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(
        ContractWrapper::new_with_empty(
            |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
                unimplemented!()
            },
            astroport_tokenfactory_tracker::contract::instantiate,
            astroport_tokenfactory_tracker::query::query,
        )
        .with_sudo_empty(astroport_tokenfactory_tracker::contract::sudo),
    )
}

pub fn assembly_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(ContractWrapper::new_with_empty(
        astro_assembly::contract::execute,
        astro_assembly::contract::instantiate,
        astro_assembly::queries::query,
    ))
}

pub fn builder_unlock_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(ContractWrapper::new_with_empty(
        builder_unlock::contract::execute,
        builder_unlock::contract::instantiate,
        builder_unlock::query::query,
    ))
}
