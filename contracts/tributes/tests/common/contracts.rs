use std::fmt::Debug;

use cosmwasm_schema::schemars::JsonSchema;
use cosmwasm_std::{CustomMsg, CustomQuery, DepsMut, Empty, Response, StdResult};
use cw_multi_test::{Contract, ContractWrapper};
use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::query::NeutronQuery;

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

// This is simplified mocked vxASTRO contract. Enough for tributes testing
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
    let dummy_execute = |_: DepsMut<C>, _, _, _: Empty| -> StdResult<_> { Ok(Response::new()) };
    Box::new(ContractWrapper::new(
        dummy_execute,
        dummy_execute,
        |_, _, _: Empty| -> StdResult<_> { unimplemented!() },
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

pub fn emissions_controller() -> Box<dyn Contract<NeutronMsg, NeutronQuery>> {
    Box::new(
        ContractWrapper::new(
            astroport_emissions_controller::execute::execute,
            astroport_emissions_controller::instantiate::instantiate,
            astroport_emissions_controller::query::query,
        )
        .with_reply_empty(astroport_emissions_controller::instantiate::reply),
    )
}

pub fn tributes_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: CustomMsg + Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_tributes::execute::execute,
            astroport_tributes::instantiate::instantiate,
            astroport_tributes::query::query,
        )
        .with_reply_empty(astroport_tributes::reply::reply),
    )
}

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
