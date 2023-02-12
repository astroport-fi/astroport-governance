use cosmwasm_std::{
    entry_point, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdResult,
};

use astroport_governance::nft::MigrateMsg;
use cw2::set_contract_version;
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

    let minter = deps.api.addr_validate(msg.minter.as_str())?;
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

/// Used for contract migration. Returns a default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
