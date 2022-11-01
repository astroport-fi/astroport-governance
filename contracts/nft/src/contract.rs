#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdError, StdResult};

use ap_nft::MigrateMsg;
use astroport::asset::addr_validate_to_lower;
use cw2::{get_contract_version, set_contract_version};
use cw721::ContractInfoResponse;
use cw721_base::msg::{ExecuteMsg, InstantiateMsg};
use cw721_base::state::Cw721Contract;
use cw721_base::{ContractError, Extension, QueryMsg};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-nft";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let info = ContractInfoResponse {
        name: msg.name,
        symbol: msg.symbol,
    };
    let tract = Cw721Contract::<Extension, Empty, Empty, Empty>::default();
    tract.contract_info.save(deps.storage, &info)?;

    let minter = addr_validate_to_lower(deps.api, msg.minter)?;
    tract.minter.save(deps.storage, &minter)?;
    Ok(Response::default())
}

/// Exposes execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg<Extension, Empty>,
) -> Result<Response, ContractError> {
    let tract = Cw721Contract::<Extension, Empty, Empty, Empty>::default();
    tract.execute(deps, env, info, msg)
}

/// Exposes queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg<Empty>) -> StdResult<Binary> {
    let tract = Cw721Contract::<Extension, Empty, Empty, Empty>::default();
    tract.query(deps, env, msg)
}

/// Manages contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-nft" => match contract_version.version.as_ref() {
            "1.0.0" => {}
            _ => return Err(StdError::generic_err("Contract can't be migrated!")),
        },
        _ => return Err(StdError::generic_err("Contract can't be migrated!")),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
