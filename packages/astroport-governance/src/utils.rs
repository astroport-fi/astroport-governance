use cosmwasm_std::{Addr, Api, ChannelResponse, IbcQuery, QuerierWrapper, StdError, StdResult};
use sha2::Digest;

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

const ESCROW_ADDRESS_VERSION: &str = "ics20-1";

/// Derives an escrow address for ICS20 IBC transfers.
/// Replicated logic from https://github.com/cosmos/ibc-go/blob/2beec482dc4b944be5378639cdc90433707a21bd/modules/apps/transfer/types/keys.go#L48-L62
/// The escrow address follows the format as outlined in ADR 028:
/// https://github.com/cosmos/cosmos-sdk/blob/master/docs/architecture/adr-028-public-key-addresses.md
pub fn determine_ics20_escrow_address(
    api: &dyn Api,
    port_id: &str,
    channel_id: &str,
) -> StdResult<Addr> {
    // a slash is used to create domain separation between port and channel identifiers to
    // prevent address collisions between escrow addresses created for different channels
    let contents = format!("{port_id}/{channel_id}");

    // ADR 028 AddressHash construction
    let mut pre_image = ESCROW_ADDRESS_VERSION.as_bytes().to_vec();
    pre_image.push(0);
    pre_image.extend_from_slice(contents.as_bytes());
    let hash = sha2::Sha256::digest(&pre_image);

    api.addr_humanize(&hash[..20].into())
}
