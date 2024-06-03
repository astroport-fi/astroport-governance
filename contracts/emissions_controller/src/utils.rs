use std::collections::HashMap;

use astroport::asset::Asset;
use astroport::incentives::{IncentivesSchedule, InputSchedule};
use cosmwasm_schema::cw_serde;
use cosmwasm_schema::serde::Serialize;
use cosmwasm_std::{
    coin, Coin, CosmosMsg, Deps, Env, Order, StdError, StdResult, Storage, Uint128,
};
use itertools::Itertools;
use neutron_sdk::bindings::msg::{IbcFee, NeutronMsg};
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::query::min_ibc_fee::query_min_ibc_fee;
use neutron_sdk::sudo::msg::RequestPacketTimeoutHeight;

use astroport_governance::emissions_controller::consts::{
    EPOCHS_START, EPOCH_LENGTH, FEE_DENOM, IBC_TIMEOUT, LP_SUBDENOM,
};
use astroport_governance::emissions_controller::hub::{OutpostInfo, OutpostParams};
use astroport_governance::emissions_controller::outpost::OutpostMsg;

use crate::error::ContractError;
use crate::state::OUTPOSTS;

/// Determine outpost prefix from address or denom.
pub fn determine_outpost_prefix(value: &str) -> Option<String> {
    let mut maybe_addr = Some(value);

    if value.starts_with("factory/") && value.ends_with(LP_SUBDENOM) {
        maybe_addr = value.split('/').nth(1);
    }

    maybe_addr.and_then(|value| {
        value.find('1').and_then(|delim_ind| {
            if delim_ind > 0 && value.chars().all(char::is_alphanumeric) {
                Some(value[..delim_ind].to_string())
            } else {
                None
            }
        })
    })
}

/// Determine outpost prefix for the pool LP token and validate
/// that this outpost exists.
pub fn get_outpost_prefix(
    pool: &str,
    outpost_prefixes: &HashMap<String, OutpostInfo>,
) -> Option<String> {
    determine_outpost_prefix(pool).and_then(|maybe_prefix| {
        if outpost_prefixes.contains_key(&maybe_prefix) {
            Some(maybe_prefix)
        } else {
            None
        }
    })
}

/// Validate LP token denom or address matches outpost prefix.
pub fn validate_outpost_prefix(value: &str, prefix: &str) -> Result<(), ContractError> {
    determine_outpost_prefix(value)
        .and_then(|maybe_prefix| {
            if maybe_prefix == prefix {
                Some(maybe_prefix)
            } else {
                None
            }
        })
        .ok_or_else(|| ContractError::InvalidOutpostPrefix(value.to_string()))
        .map(|_| ())
}

/// Helper function to get outpost prefix from the ICS20 IBC packet.
pub fn get_outpost_from_hub_ics20_channel(
    store: &dyn Storage,
    source_channel: Option<String>,
) -> StdResult<String> {
    let source_channel = source_channel
        .ok_or_else(|| StdError::generic_err("Missing source_channel in IBC ack packet"))?;
    // Find outpost by ics20 channel
    OUTPOSTS
        .range(store, None, None, Order::Ascending)
        .find_map(|data| {
            let (outpost_prefix, outpost) = data.ok()?;
            outpost.params.as_ref().and_then(|params| {
                if source_channel == params.ics20_channel {
                    Some(outpost_prefix.clone())
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| {
            StdError::generic_err(format!(
                "Unknown outpost with {source_channel} ics20 channel"
            ))
        })
}

#[cw_serde]
pub enum IbcHookMemo<T> {
    Wasm { contract: String, msg: T },
}

impl<T: Serialize> IbcHookMemo<T> {
    pub fn build(contract: &str, msg: T) -> StdResult<String> {
        serde_json::to_string(&IbcHookMemo::Wasm {
            contract: contract.to_string(),
            msg,
        })
        .map_err(|err| StdError::generic_err(err.to_string()))
    }
}

pub fn min_ntrn_ibc_fee(deps: Deps<NeutronQuery>) -> Result<IbcFee, ContractError> {
    let fee = query_min_ibc_fee(deps)?.min_fee;

    Ok(IbcFee {
        recv_fee: fee.recv_fee,
        ack_fee: fee
            .ack_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
        timeout_fee: fee
            .timeout_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
    })
}

/// Compose ics20 message with IBC hook memo for outpost emissions controller.
pub fn build_emission_ibc_msg(
    env: &Env,
    params: &OutpostParams,
    ibc_fee: &IbcFee,
    astro_funds: Coin,
    schedules: &[(String, InputSchedule)],
) -> StdResult<CosmosMsg<NeutronMsg>> {
    let outpost_controller_msg =
        astroport_governance::emissions_controller::msg::ExecuteMsg::Custom(
            OutpostMsg::SetEmissions {
                schedules: schedules.to_vec(),
            },
        );
    Ok(NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: params.ics20_channel.clone(),
        token: astro_funds,
        sender: env.contract.address.to_string(),
        receiver: params.emissions_controller.clone(),
        timeout_height: RequestPacketTimeoutHeight {
            revision_number: None,
            revision_height: None,
        },
        timeout_timestamp: env.block.time.plus_seconds(IBC_TIMEOUT).nanos(),
        memo: IbcHookMemo::build(&params.emissions_controller, outpost_controller_msg)?,
        fee: ibc_fee.clone(),
    }
    .into())
}

/// This function converts schedule pairs (lp_token, ASTRO amount)
/// into the incentives contract executable message.
/// It also calculates total ASTRO funds required for the emissions.
pub fn raw_emissions_to_schedules(
    env: &Env,
    raw_schedules: &[(String, Uint128)],
    schedule_denom: &str,
    hub_denom: &str,
) -> (Vec<(String, InputSchedule)>, Coin) {
    let mut total_astro = Uint128::zero();
    // Ensure emissions >=1 uASTRO per second.
    // >= 1 uASTRO per second is the requirement in the incentives contract.
    let schedules = raw_schedules
        .iter()
        .filter_map(|(pool, astro_amount)| {
            let schedule = InputSchedule {
                reward: Asset::native(schedule_denom, *astro_amount),
                duration_periods: 1,
            };
            // Schedule validation imported from the incentives contract
            IncentivesSchedule::from_input(env, &schedule).ok()?;

            total_astro += astro_amount;
            Some((pool.clone(), schedule))
        })
        .collect_vec();

    let astro_funds = coin(total_astro.u128(), hub_denom);

    (schedules, astro_funds)
}

/// Normalize current timestamp to the beginning of the current epoch (Monday).
pub fn get_epoch_start(timestamp: u64) -> u64 {
    let rem = timestamp % EPOCHS_START;
    if rem % EPOCH_LENGTH == 0 {
        // Hit at the beginning of the current epoch
        timestamp
    } else {
        // Hit somewhere in the middle
        EPOCHS_START + rem / EPOCH_LENGTH * EPOCH_LENGTH
    }
}

// TODO: Implement dynamic emissions curve
pub fn astro_emissions_curve() -> Uint128 {
    Uint128::new(100_000_000_000)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_determine_outpost_prefix() {
        assert_eq!(
            determine_outpost_prefix(&format!("factory/wasm1addr{LP_SUBDENOM}")).unwrap(),
            "wasm"
        );
        assert_eq!(determine_outpost_prefix("wasm1addr").unwrap(), "wasm");
        assert_eq!(determine_outpost_prefix("1addr"), None);
        assert_eq!(
            determine_outpost_prefix(&format!("factory/1addr{LP_SUBDENOM}")),
            None
        );
        assert_eq!(determine_outpost_prefix("factory/wasm1addr/random"), None);
        assert_eq!(
            determine_outpost_prefix(&format!("factory{LP_SUBDENOM}")),
            None
        );
    }

    #[test]
    fn test_epoch_start() {
        assert_eq!(get_epoch_start(1716163200), 1716163200);
        assert_eq!(get_epoch_start(1716163200 + 1), 1716163200);
        assert_eq!(
            get_epoch_start(1716163200 + EPOCH_LENGTH),
            1716163200 + EPOCH_LENGTH
        );
        assert_eq!(
            get_epoch_start(1716163200 + EPOCH_LENGTH + 1),
            1716163200 + EPOCH_LENGTH
        );
    }
}
