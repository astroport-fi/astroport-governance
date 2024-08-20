use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    DepsMut, Empty, Env, IbcBasicResponse, IbcPacketAckMsg, IbcPacketReceiveMsg,
    IbcPacketTimeoutMsg, Response, StdResult,
};
use cw_multi_test::{Contract, ContractWrapper};

use astroport_emissions_controller_outpost::ibc::{
    do_packet_receive, ibc_packet_ack, ibc_packet_timeout,
};

pub fn token_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ))
}

pub fn pair_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    )
}

pub fn vxastro_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_voting_escrow::contract::execute,
        astroport_voting_escrow::contract::instantiate,
        astroport_voting_escrow::contract::query,
    ))
}

pub fn incentives_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_incentives::execute::execute,
        astroport_incentives::instantiate::instantiate,
        astroport_incentives::query::query,
    ))
}

pub fn factory_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply_empty(astroport_factory::contract::reply),
    )
}

#[cw_serde]
pub enum TestSudoMsg {
    Ack(IbcPacketAckMsg),
    Timeout(IbcPacketTimeoutMsg),
    IbcRecv(IbcPacketReceiveMsg),
}

fn sudo(deps: DepsMut, env: Env, msg: TestSudoMsg) -> StdResult<Response> {
    match msg {
        TestSudoMsg::Ack(packet) => ibc_packet_ack(deps, env, packet),
        TestSudoMsg::Timeout(packet) => ibc_packet_timeout(deps, env, packet),
        TestSudoMsg::IbcRecv(packet) => do_packet_receive(deps, env, packet).map(|ibc_response| {
            IbcBasicResponse::default()
                .add_attributes(ibc_response.attributes)
                .add_submessages(ibc_response.messages)
        }),
    }
    .map(|ibc_response| {
        Response::default()
            .add_attributes(ibc_response.attributes)
            .add_submessages(ibc_response.messages)
    })
}

pub fn emissions_controller() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new(
            astroport_emissions_controller_outpost::execute::execute,
            astroport_emissions_controller_outpost::instantiate::instantiate,
            astroport_emissions_controller_outpost::query::query,
        )
        .with_reply_empty(astroport_emissions_controller_outpost::instantiate::reply)
        .with_sudo_empty(sudo),
    )
}
