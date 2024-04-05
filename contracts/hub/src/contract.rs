use cosmwasm_std::{entry_point, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;

use astroport::staking::{ConfigResponse, QueryMsg};
use astroport_governance::{
    hub::{Config, InstantiateMsg, MigrateMsg},
    interchain::{MAX_IBC_TIMEOUT_SECONDS, MIN_IBC_TIMEOUT_SECONDS},
};

use crate::{error::ContractError, state::CONFIG};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-hub";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Instantiates the contract, storing the config and querying the staking contract.
/// Returns a `Response` object on successful execution or a `ContractError` on failure.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Query staking contract for ASTRO and xASTRO address
    let staking_config: ConfigResponse = deps
        .querier
        .query_wasm_smart(&msg.staking_addr, &QueryMsg::Config {})?;

    if !(MIN_IBC_TIMEOUT_SECONDS..=MAX_IBC_TIMEOUT_SECONDS).contains(&msg.ibc_timeout_seconds) {
        return Err(ContractError::InvalidIBCTimeout {
            timeout: msg.ibc_timeout_seconds,
            min: MIN_IBC_TIMEOUT_SECONDS,
            max: MAX_IBC_TIMEOUT_SECONDS,
        });
    }

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        assembly_addr: deps.api.addr_validate(&msg.assembly_addr)?,
        cw20_ics20_addr: deps.api.addr_validate(&msg.cw20_ics20_addr)?,
        staking_addr: deps.api.addr_validate(&msg.staking_addr)?,
        token_addr: staking_config.deposit_token_addr,
        xtoken_addr: staking_config.share_token_addr,
        generator_controller_addr: deps.api.addr_validate(&msg.generator_controller_addr)?,
        ibc_timeout_seconds: msg.ibc_timeout_seconds,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// Migrates the contract to a new version.
#[entry_point]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Err(ContractError::MigrationError {})
}

#[cfg(test)]
mod tests {

    use super::*;

    use crate::{
        contract::instantiate,
        mock::{mock_all, ASSEMBLY, CW20ICS20, GENERATOR_CONTROLLER, OWNER, STAKING},
    };

    // Test Cases:
    //
    // Expect Success
    //      - Invalid IBC timeouts are rejected
    //
    #[test]
    fn invalid_ibc_timeout() {
        let (mut deps, env, info) = mock_all(OWNER);

        // Test MAX + 1
        let ibc_timeout_seconds = MAX_IBC_TIMEOUT_SECONDS + 1;
        let err = instantiate(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            astroport_governance::hub::InstantiateMsg {
                owner: OWNER.to_string(),
                assembly_addr: ASSEMBLY.to_string(),
                cw20_ics20_addr: CW20ICS20.to_string(),
                staking_addr: STAKING.to_string(),
                generator_controller_addr: GENERATOR_CONTROLLER.to_string(),
                ibc_timeout_seconds,
            },
        )
        .unwrap_err();

        assert_eq!(
            err,
            ContractError::InvalidIBCTimeout {
                timeout: ibc_timeout_seconds,
                min: MIN_IBC_TIMEOUT_SECONDS,
                max: MAX_IBC_TIMEOUT_SECONDS
            }
        );

        // Test MIN - 1
        let ibc_timeout_seconds = MIN_IBC_TIMEOUT_SECONDS - 1;
        let err = instantiate(
            deps.as_mut(),
            env,
            info,
            astroport_governance::hub::InstantiateMsg {
                owner: OWNER.to_string(),
                assembly_addr: ASSEMBLY.to_string(),
                cw20_ics20_addr: CW20ICS20.to_string(),
                staking_addr: STAKING.to_string(),
                generator_controller_addr: GENERATOR_CONTROLLER.to_string(),
                ibc_timeout_seconds,
            },
        )
        .unwrap_err();

        assert_eq!(
            err,
            ContractError::InvalidIBCTimeout {
                timeout: ibc_timeout_seconds,
                min: MIN_IBC_TIMEOUT_SECONDS,
                max: MAX_IBC_TIMEOUT_SECONDS
            }
        );
    }
}
