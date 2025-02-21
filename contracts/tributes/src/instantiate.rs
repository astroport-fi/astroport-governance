use astroport::asset::validate_native_denom;
use astroport_governance::emissions_controller::utils::get_epoch_start;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, DepsMut, Env, MessageInfo, Response};

use astroport_governance::tributes::{
    Config, InstantiateMsg, REWARDS_AMOUNT_LIMITS, TOKEN_TRANSFER_GAS_LIMIT,
};

use crate::error::ContractError;
use crate::state::CONFIG;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    ensure!(
        REWARDS_AMOUNT_LIMITS.contains(&msg.rewards_limit),
        ContractError::InvalidRewardsLimit {}
    );

    deps.api
        .addr_validate(msg.tribute_fee_info.fee_collector.as_str())?;

    validate_native_denom(&msg.tribute_fee_info.fee.denom)?;

    ensure!(
        !msg.tribute_fee_info.fee.amount.is_zero(),
        ContractError::InvalidTributeFeeAmount {}
    );

    ensure!(
        TOKEN_TRANSFER_GAS_LIMIT.contains(&msg.token_transfer_gas_limit),
        ContractError::InvalidTokenTransferGasLimit {}
    );

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            emissions_controller: deps.api.addr_validate(&msg.emissions_controller)?,
            tribute_fee_info: msg.tribute_fee_info,
            rewards_limit: msg.rewards_limit,
            initial_epoch: get_epoch_start(env.block.time.seconds()),
            token_transfer_gas_limit: msg.token_transfer_gas_limit,
        },
    )?;

    Ok(Response::new().add_attribute("action", "instantiate_tributes"))
}
