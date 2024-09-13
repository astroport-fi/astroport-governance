#![cfg(not(tarpaulin_include))]

use cosmwasm_std::{DepsMut, Empty, Env, Response};
use cw2::{get_contract_version, set_contract_version};

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::error::ContractError;

#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        CONTRACT_NAME => match contract_version.version.as_ref() {
            "1.0.0" => Ok(()),
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
