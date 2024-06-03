#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, wasm_execute, DepsMut, Env, Ibc3ChannelOpenResponse, IbcBasicResponse,
    IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcPacketAckMsg,
    IbcPacketReceiveMsg, IbcPacketTimeoutMsg, IbcReceiveResponse, Never, StdError, StdResult,
    Storage,
};

use astroport_governance::emissions_controller::consts::{IBC_APP_VERSION, IBC_ORDERING};
use astroport_governance::emissions_controller::msg::{IbcAckResult, VxAstroIbcMsg};
use astroport_governance::emissions_controller::outpost::UserIbcError;
use astroport_governance::voting_escrow;

use crate::state::{CONFIG, PENDING_MESSAGES, USER_IBC_ERROR};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_open(
    _deps: DepsMut,
    _env: Env,
    msg: IbcChannelOpenMsg,
) -> StdResult<Option<Ibc3ChannelOpenResponse>> {
    let channel = msg.channel();

    ensure!(
        channel.order == IBC_ORDERING,
        StdError::generic_err("Ordering is invalid. The channel must be unordered",)
    );
    ensure!(
        channel.version == IBC_APP_VERSION,
        StdError::generic_err(format!("Must set version to `{IBC_APP_VERSION}`",))
    );
    if let Some(counter_version) = msg.counterparty_version() {
        if counter_version != IBC_APP_VERSION {
            return Err(StdError::generic_err(format!(
                "Counterparty version must be `{IBC_APP_VERSION}`"
            )));
        }
    }

    Ok(Some(Ibc3ChannelOpenResponse {
        version: IBC_APP_VERSION.to_string(),
    }))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_connect(
    _deps: DepsMut,
    _env: Env,
    msg: IbcChannelConnectMsg,
) -> StdResult<IbcBasicResponse> {
    let channel = msg.channel();

    if let Some(counter_version) = msg.counterparty_version() {
        if counter_version != IBC_APP_VERSION {
            return Err(StdError::generic_err(format!(
                "Counterparty version must be `{IBC_APP_VERSION}`"
            )));
        }
    }

    Ok(IbcBasicResponse::new()
        .add_attribute("action", "ibc_connect")
        .add_attribute("channel_id", &channel.endpoint.channel_id))
}

#[cfg(not(tarpaulin_include))]
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    _deps: DepsMut,
    _env: Env,
    _msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, Never> {
    unimplemented!("This contract is only sending IBC messages")
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    deps: DepsMut,
    _env: Env,
    msg: IbcPacketAckMsg,
) -> StdResult<IbcBasicResponse> {
    let orig_msg: VxAstroIbcMsg = from_json(&msg.original_packet.data)?;
    match from_json(&msg.acknowledgement.data)? {
        IbcAckResult::Ok(_) => {
            let mut response = IbcBasicResponse::new().add_attribute("action", "ibc_packet_ack");
            let voter = match &orig_msg {
                VxAstroIbcMsg::UpdateUserVotes {
                    voter,
                    is_unlock: true,
                    ..
                } => {
                    let config = CONFIG.load(deps.storage)?;
                    let relock_msg = wasm_execute(
                        config.vxastro,
                        &voting_escrow::ExecuteMsg::ConfirmUnlock {
                            user: voter.to_string(),
                        },
                        vec![],
                    )?;
                    response = response
                        .add_attribute("action", "confirm_vxastro_unlock")
                        .add_message(relock_msg);

                    voter
                }
                VxAstroIbcMsg::UpdateUserVotes { voter, .. }
                | VxAstroIbcMsg::Vote { voter, .. } => voter,
            };
            USER_IBC_ERROR.remove(deps.storage, voter);
            PENDING_MESSAGES.remove(deps.storage, voter);

            Ok(response)
        }
        IbcAckResult::Error(err) => process_ibc_error(deps.storage, orig_msg, err),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_timeout(
    deps: DepsMut,
    _env: Env,
    msg: IbcPacketTimeoutMsg,
) -> StdResult<IbcBasicResponse> {
    process_ibc_error(
        deps.storage,
        from_json(msg.packet.data)?,
        "IBC packet timeout".to_string(),
    )
}

#[cfg(not(tarpaulin_include))]
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_close(
    _deps: DepsMut,
    _env: Env,
    _channel: IbcChannelCloseMsg,
) -> StdResult<IbcBasicResponse> {
    unimplemented!()
}

pub fn process_ibc_error(
    storage: &mut dyn Storage,
    msg: VxAstroIbcMsg,
    err: String,
) -> StdResult<IbcBasicResponse> {
    let mut response = IbcBasicResponse::default().add_attribute("action", "process_ibc_error");
    let voter = match &msg {
        VxAstroIbcMsg::UpdateUserVotes {
            voter,
            is_unlock: true,
            ..
        } => {
            // Relock user vxASTRO in case IBC failed
            let config = CONFIG.load(storage)?;
            let relock_msg = wasm_execute(
                config.vxastro,
                &voting_escrow::ExecuteMsg::ForceRelock {
                    user: voter.to_string(),
                },
                vec![],
            )?;
            response = response
                .add_attribute("action", "relock_user_vxastro")
                .add_message(relock_msg);
            voter.clone()
        }
        VxAstroIbcMsg::Vote { voter, .. } | VxAstroIbcMsg::UpdateUserVotes { voter, .. } => {
            voter.clone()
        }
    };

    USER_IBC_ERROR.save(storage, &voter, &UserIbcError { msg, err })?;
    PENDING_MESSAGES.remove(storage, &voter);

    Ok(response)
}

#[cfg(test)]
mod unit_tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{IbcChannel, IbcEndpoint, IbcOrder};

    use super::*;

    #[test]
    fn test_channel_open() {
        let mut deps = mock_dependencies();

        let mut ibc_channel = IbcChannel::new(
            IbcEndpoint {
                port_id: "doesnt matter".to_string(),
                channel_id: "doesnt matter".to_string(),
            },
            IbcEndpoint {
                port_id: "doesnt matter".to_string(),
                channel_id: "doesnt matter".to_string(),
            },
            IbcOrder::Unordered,
            IBC_APP_VERSION,
            "doesnt matter",
        );
        let res = ibc_channel_open(
            deps.as_mut(),
            mock_env(),
            IbcChannelOpenMsg::new_init(ibc_channel.clone()),
        )
        .unwrap()
        .unwrap();

        assert_eq!(res.version, IBC_APP_VERSION);

        ibc_channel.order = IbcOrder::Ordered;

        let res = ibc_channel_open(
            deps.as_mut(),
            mock_env(),
            IbcChannelOpenMsg::new_init(ibc_channel.clone()),
        )
        .unwrap_err();
        assert_eq!(
            res,
            StdError::generic_err("Ordering is invalid. The channel must be unordered")
        );

        ibc_channel.order = IbcOrder::Unordered;
        ibc_channel.version = "wrong_version".to_string();

        let res = ibc_channel_open(
            deps.as_mut(),
            mock_env(),
            IbcChannelOpenMsg::new_init(ibc_channel.clone()),
        )
        .unwrap_err();
        assert_eq!(
            res,
            StdError::generic_err(format!("Must set version to `{IBC_APP_VERSION}`"))
        );

        ibc_channel.version = IBC_APP_VERSION.to_string();

        let res = ibc_channel_open(
            deps.as_mut(),
            mock_env(),
            IbcChannelOpenMsg::new_try(ibc_channel.clone(), "wrong_version"),
        )
        .unwrap_err();
        assert_eq!(
            res,
            StdError::generic_err(format!("Counterparty version must be `{IBC_APP_VERSION}`"))
        );

        ibc_channel_open(
            deps.as_mut(),
            mock_env(),
            IbcChannelOpenMsg::new_try(ibc_channel.clone(), IBC_APP_VERSION),
        )
        .unwrap()
        .unwrap();
    }

    #[test]
    fn test_channel_connect() {
        let mut deps = mock_dependencies();

        let ibc_channel = IbcChannel::new(
            IbcEndpoint {
                port_id: "doesnt matter".to_string(),
                channel_id: "doesnt matter".to_string(),
            },
            IbcEndpoint {
                port_id: "doesnt matter".to_string(),
                channel_id: "doesnt matter".to_string(),
            },
            IbcOrder::Unordered,
            IBC_APP_VERSION,
            "doesnt matter",
        );

        ibc_channel_connect(
            deps.as_mut(),
            mock_env(),
            IbcChannelConnectMsg::new_ack(ibc_channel.clone(), IBC_APP_VERSION),
        )
        .unwrap();

        let err = ibc_channel_connect(
            deps.as_mut(),
            mock_env(),
            IbcChannelConnectMsg::new_ack(ibc_channel.clone(), "wrong version"),
        )
        .unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err(format!("Counterparty version must be `{IBC_APP_VERSION}`"))
        );
    }
}
