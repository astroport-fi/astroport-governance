use cosmwasm_std::{to_json_binary, Env, IbcMsg, Storage};

use astroport_governance::emissions_controller::consts::IBC_TIMEOUT;
use astroport_governance::emissions_controller::msg::VxAstroIbcMsg;

use crate::error::ContractError;
use crate::state::PENDING_MESSAGES;

/// Ensure voter has no pending IBC requests and prepare an IBC packet.
pub fn prepare_ibc_packet(
    storage: &mut dyn Storage,
    env: &Env,
    voter: &str,
    payload: VxAstroIbcMsg,
    channel_id: String,
) -> Result<IbcMsg, ContractError> {
    // Block any new IBC messages for users with pending IBC requests
    // until the previous one is acknowledged, failed or timed out.
    PENDING_MESSAGES.update(storage, voter, |v| match v {
        Some(_) => Err(ContractError::PendingUser(voter.to_string())),
        None => Ok(payload.clone()),
    })?;

    Ok(IbcMsg::SendPacket {
        channel_id,
        data: to_json_binary(&payload)?,
        timeout: env.block.time.plus_seconds(IBC_TIMEOUT).into(),
    })
}
