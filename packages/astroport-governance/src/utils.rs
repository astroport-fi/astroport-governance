use cosmwasm_std::{Addr, ChannelResponse, IbcQuery, QuerierWrapper, StdError, StdResult};

/// Checks that a contract supports a given IBC-channel.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **contract** is the contract to check channel support on.
///
/// * **given_channel** is an IBC channel id the function needs to check.
pub fn check_contract_supports_channel(
    querier: QuerierWrapper,
    contract: &Addr,
    given_channel: &String,
) -> StdResult<()> {
    let port_id = Some(format!("wasm.{contract}"));
    let ChannelResponse { channel } = querier.query(
        &IbcQuery::Channel {
            channel_id: given_channel.to_string(),
            port_id,
        }
        .into(),
    )?;
    channel.map(|_| ()).ok_or_else(|| {
        StdError::generic_err(format!(
            "The contract does not have channel {given_channel}"
        ))
    })
}
