use crate::contract::{instantiate, CONTRACT_NAME, CONTRACT_VERSION};
use crate::error::ContractError;
use astroport_governance::assembly::InstantiateMsg;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Addr, DepsMut, Env, IbcMsg, MessageInfo, Response, StdError};

const EXPECTED_CONTRACT_NAME: &str = "astro-satellite-neutron";
const EXPECTED_CONTRACT_VERSION: &str = "1.1.0-hubmove";

/// This migration is used to convert the satellite contract on Neutron into Assembly.
/// Cosmwasm migration is meant to be executed from multisig controlled by Astroport to prevent abnormal subsequences
/// and be able to react promptly in case of any issues.
///
/// Mainnet contract which is only subject of this migration: https://neutron.celat.one/neutron-1/contracts/neutron1ffus553eet978k024lmssw0czsxwr97mggyv85lpcsdkft8v9ufsz3sa07
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: InstantiateMsg) -> Result<Response, ContractError> {
    cw2::assert_contract_version(
        deps.storage,
        EXPECTED_CONTRACT_NAME,
        EXPECTED_CONTRACT_VERSION,
    )?;

    // Clear satellite's state
    astro_satellite::state::LATEST_HUB_SIGNAL_TIME.remove(deps.storage);
    astro_satellite::state::REPLY_DATA.remove(deps.storage);
    astro_satellite::state::RESULTS.clear(deps.storage);

    // Close old governance channel with Terra
    let satellite_config = astro_satellite::state::CONFIG.load(deps.storage)?;
    let close_msg = IbcMsg::CloseChannel {
        channel_id: satellite_config.gov_channel.ok_or_else(|| {
            StdError::generic_err("Missing governance channel in satellite config")
        })?,
    };

    let cw_admin = deps
        .querier
        .query_wasm_contract_info(&env.contract.address)?
        .admin
        .unwrap();
    // Even though info object is ignored in instantiate, we provide it for clarity
    let info = MessageInfo {
        sender: Addr::unchecked(cw_admin),
        funds: vec![],
    };
    // Instantiate Assembly state.
    // Config and cw2 info will be overwritten.
    let contract_version = cw2::get_contract_version(deps.storage)?;

    instantiate(deps, env, info, msg).map(|resp| {
        resp.add_message(close_msg).add_attributes([
            ("previous_contract_name", contract_version.contract.as_str()),
            (
                "previous_contract_version",
                contract_version.version.as_str(),
            ),
            ("new_contract_name", CONTRACT_NAME),
            ("new_contract_version", CONTRACT_VERSION),
        ])
    })
}
