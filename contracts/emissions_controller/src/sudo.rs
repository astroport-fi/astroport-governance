#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, DepsMut, Env, Response, StdError, StdResult, Storage};
use neutron_sdk::sudo::msg::{RequestPacket, TransferSudoMsg};

use astroport_governance::emissions_controller::hub::OutpostStatus;

use crate::state::TUNE_INFO;
use crate::utils::get_outpost_from_hub_channel;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: TransferSudoMsg) -> StdResult<Response> {
    match msg {
        TransferSudoMsg::Response { request, .. } => {
            process_ibc_reply(deps.storage, env, request, false)
        }
        TransferSudoMsg::Error { request, .. } | TransferSudoMsg::Timeout { request } => {
            process_ibc_reply(deps.storage, env, request, true)
        }
    }
}

/// Process outcome of an ics20 IBC packet with IBC hook.
/// If a packet was successful, it marks the outpost as done.
/// If a packet failed or timed out, it marks the outpost as failed, so it can be retried.
pub fn process_ibc_reply(
    storage: &mut dyn Storage,
    env: Env,
    packet: RequestPacket,
    failed: bool,
) -> StdResult<Response> {
    let source_channel = packet
        .source_channel
        .ok_or_else(|| StdError::generic_err("Missing source_channel in IBC ack packet"))?;
    let outpost =
        get_outpost_from_hub_channel(storage, source_channel, |params| &params.ics20_channel)?;

    let mut tune_info = TUNE_INFO.load(storage)?;
    tune_info
        .outpost_emissions_statuses
        .get_mut(&outpost)
        .ok_or_else(|| StdError::generic_err("Outpost status for {outpost} not found"))
        .and_then(|status| {
            ensure!(
                *status == OutpostStatus::InProgress,
                StdError::generic_err(format!("Outpost {outpost} is not in progress"))
            );
            *status = if failed {
                OutpostStatus::Failed
            } else {
                OutpostStatus::Done
            };
            Ok(())
        })?;
    TUNE_INFO.save(storage, &tune_info, env.block.time.seconds())?;

    let mut attrs = if failed {
        vec![("action", "ibc_failed")]
    } else {
        vec![("action", "ibc_transfer_ack")]
    };
    attrs.push(("outpost", &outpost));

    Ok(Response::default().add_attributes(attrs))
}
