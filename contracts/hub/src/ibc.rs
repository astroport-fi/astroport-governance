use astroport::querier::query_token_balance;
use cosmwasm_std::{
    entry_point, from_json, to_json_binary, Deps, DepsMut, Env, Ibc3ChannelOpenResponse,
    IbcBasicResponse, IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg,
    IbcChannelOpenResponse, IbcOrder, IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg,
    IbcReceiveResponse, Never, StdError, StdResult, SubMsg,
};

use astroport_governance::interchain::{get_contract_from_ibc_port, Hub, Outpost, Response};

use crate::{
    error::ContractError,
    ibc_governance::{
        handle_ibc_blacklisted, handle_ibc_cast_assembly_vote, handle_ibc_cast_emissions_vote,
        handle_ibc_unlock,
    },
    ibc_misc::handle_ibc_withdraw_stuck_funds,
    ibc_query::handle_ibc_query_proposal,
    ibc_staking::{construct_unstake_msg, handle_ibc_unstake},
    reply::UNSTAKE_ID,
    state::{ReplyData, CONFIG, OUTPOSTS, REPLY_DATA},
};

pub const IBC_APP_VERSION: &str = "astroport-outpost-v1";
pub const IBC_ORDERING: IbcOrder = IbcOrder::Unordered;

/// Handle the opening of a new IBC channel
///
/// We verify that the connection is using the correct configuration
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_open(
    _deps: DepsMut,
    _env: Env,
    msg: IbcChannelOpenMsg,
) -> Result<IbcChannelOpenResponse, ContractError> {
    let channel = msg.channel();

    if channel.order != IBC_ORDERING {
        return Err(ContractError::Std(StdError::generic_err(
            "Ordering is invalid. The channel must be unordered".to_string(),
        )));
    }
    if channel.version != IBC_APP_VERSION {
        return Err(ContractError::Std(StdError::generic_err(format!(
            "Must set version to `{IBC_APP_VERSION}`"
        ))));
    }

    if let Some(counter_version) = msg.counterparty_version() {
        if counter_version != IBC_APP_VERSION {
            return Err(ContractError::Std(StdError::generic_err(format!(
                "Counterparty version must be `{IBC_APP_VERSION}`"
            ))));
        }
    }

    Ok(Some(Ibc3ChannelOpenResponse {
        version: IBC_APP_VERSION.to_string(),
    }))
}

/// Handle the connection of a new IBC channel
///
/// We verify that the connection is being made to an allowed Outpost and
/// if the channel has not been set, add it
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_connect(
    deps: DepsMut,
    _env: Env,
    msg: IbcChannelConnectMsg,
) -> Result<IbcBasicResponse, ContractError> {
    let channel = msg.channel();

    if let Some(counter_version) = msg.counterparty_version() {
        if counter_version != IBC_APP_VERSION {
            return Err(ContractError::Std(StdError::generic_err(format!(
                "Counterparty version must be `{IBC_APP_VERSION}`"
            ))));
        }
    }

    // We allow any contract with any channel to connect, but we will only
    // allow messages from whitelisted Outposts to be accepted
    // If a channel has already been established, we will reject the connection
    let counterparty_port =
        get_contract_from_ibc_port(channel.counterparty_endpoint.port_id.as_str());
    if let Some(channels) = OUTPOSTS.may_load(deps.storage, counterparty_port)? {
        return Err(ContractError::ChannelAlreadyEstablished {
            channel_id: channels.outpost,
        });
    }

    Ok(IbcBasicResponse::new()
        .add_attribute("action", "ibc_connect")
        .add_attribute("channel_id", &channel.endpoint.channel_id))
}

/// Handle the receiving the packets while wrapping the actual call to provide
/// returning errors as an acknowledgement.
///
/// This allows the original caller from another chain to handle the failure
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, Never> {
    do_packet_receive(deps, env, msg).or_else(|err| {
        // Construct an error acknowledgement that can be handled on the Outpost
        let ack_data = to_json_binary(&Response::new_error(err.to_string())).unwrap();

        Ok(IbcReceiveResponse::new()
            .add_attribute("action", "ibc_packet_receive")
            .add_attribute("error", err.to_string())
            .set_ack(ack_data))
    })
}

/// Process the received packet and return the response
///
/// Packets are expected to be wrapped in the Hub format, if it doesn't conform
/// it will be failed.
///
/// If a ContractError is returned, it will be wrapped into a Response
/// containing the error to be handled on the Outpost
fn do_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    block_unauthorized_packets(
        deps.as_ref(),
        msg.packet.src.port_id.clone(),
        msg.packet.dest.channel_id.to_string(),
    )?;

    // Parse the packet data into a Hub message
    let outpost_msg: Hub = from_json(&msg.packet.data)?;
    match outpost_msg {
        Hub::QueryProposal { id } => handle_ibc_query_proposal(deps, id),
        Hub::CastAssemblyVote {
            proposal_id,
            voter,
            vote_option,
            voting_power,
        } => handle_ibc_cast_assembly_vote(
            deps,
            msg.packet.dest.channel_id,
            proposal_id,
            voter,
            vote_option,
            voting_power,
        ),
        Hub::CastEmissionsVote {
            voter,
            voting_power,
            votes,
        } => handle_ibc_cast_emissions_vote(
            deps,
            env,
            msg.packet.dest.channel_id,
            voter,
            voting_power,
            votes,
        ),
        Hub::Unstake { receiver, amount } => {
            handle_ibc_unstake(deps, env, msg.packet.dest.channel_id, receiver, amount)
        }
        Hub::KickUnlockedVoter { voter } => handle_ibc_unlock(deps, voter),
        Hub::KickBlacklistedVoter { voter } => handle_ibc_blacklisted(deps, voter),
        Hub::WithdrawFunds { user } => {
            handle_ibc_withdraw_stuck_funds(deps, msg.packet.dest.channel_id, user)
        }
        _ => Err(ContractError::NotIBCAction {
            action: outpost_msg.to_string(),
        }),
    }
}

/// Handle IBC packet timeouts for messages we sent
///
/// Timeouts will cause certain actions to be reversed and, when applicable, return
/// funds to the user
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_timeout(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketTimeoutMsg,
) -> Result<IbcBasicResponse, ContractError> {
    let failed_msg: Outpost = from_json(&msg.packet.data)?;
    match failed_msg {
        Outpost::MintXAstro { receiver, amount } => {
            let config = CONFIG.load(deps.storage)?;

            // If we get a timeout on a packet to mint remote xASTRO
            // we need to undo the transaction and return the original ASTRO
            // If we get another timeout returning the original ASTRO the funds
            // will be held in this contract to withdraw later
            let wasm_msg = construct_unstake_msg(
                deps.storage,
                deps.querier,
                env.clone(),
                msg.packet.src.channel_id.clone(),
                receiver.clone(),
                amount,
            )?;
            let sub_msg = SubMsg::reply_on_success(wasm_msg, UNSTAKE_ID);

            // We don't decrease the channel balance here, but only after unstaking
            let current_astro_balance = query_token_balance(
                &deps.querier,
                config.token_addr.to_string(),
                env.contract.address,
            )?;

            // Temporarily save the data needed for the SubMessage reply
            let reply_data = ReplyData {
                receiver: receiver.clone(),
                receiving_channel: msg.packet.src.channel_id,
                value: current_astro_balance,
                original_value: amount,
            };
            REPLY_DATA.save(deps.storage, &reply_data)?;

            Ok(IbcBasicResponse::new()
                .add_attribute("action", "ibc_packet_timeout")
                .add_submessage(sub_msg)
                .add_attribute("original_action", "mint_remote_xastro")
                .add_attribute("original_receiver", receiver)
                .add_attribute("original_amount", amount.to_string()))
        }
    }
}

/// Handle IBC packet acknowledgements for messages we sent
///
/// We don't need acks for now, we handle failures instead
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    _deps: DepsMut,
    _env: Env,
    _msg: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, ContractError> {
    Ok(IbcBasicResponse::new().add_attribute("action", "ibc_packet_ack"))
}

/// Handle the closing of IBC channels, which we don't allow
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_close(
    _deps: DepsMut,
    _env: Env,
    _channel: IbcChannelCloseMsg,
) -> StdResult<IbcBasicResponse> {
    Err(StdError::generic_err("Closing channel is not allowed"))
}

/// Checks the provided port against the Outpost list.
///
/// If the port doesn't exist or the channel doesn't match, this function will
/// return an error, effectively blocking the packet.
fn block_unauthorized_packets(
    deps: Deps,
    source_port_id: String,
    destination_channel_id: String,
) -> Result<(), ContractError> {
    let counterparty_port = get_contract_from_ibc_port(source_port_id.as_str());

    let outpost_channels = OUTPOSTS.load(deps.storage, counterparty_port)?;
    if outpost_channels.outpost != destination_channel_id {
        return Err(ContractError::Unauthorized {});
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{mock_info, MOCK_CONTRACT_ADDR},
        Addr, IbcAcknowledgement, IbcEndpoint, IbcPacket, Uint128,
    };

    use super::*;
    use crate::{
        contract::instantiate,
        execute::execute,
        mock::{
            mock_all, mock_channel, mock_ibc_packet, setup_channel, ASSEMBLY, CW20ICS20,
            GENERATOR_CONTROLLER, OWNER, STAKING,
        },
    };

    // Test Cases:
    //
    // Expect Success
    //      - Creating a channel with correct settings
    //
    // Expect Error
    //      - Attempt to create a channel with an invalid version
    //      - Attempt to create a channel with an invalid ordering
    //      - Attempt to create a channel before registering an Outpost
    //      - Attempt to create a channel with an unauthorize Outpost address
    #[test]
    fn ibc_open_channel() {
        let (mut deps, env, info) = mock_all(OWNER);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::hub::InstantiateMsg {
                owner: OWNER.to_string(),
                assembly_addr: ASSEMBLY.to_string(),
                cw20_ics20_addr: CW20ICS20.to_string(),
                staking_addr: STAKING.to_string(),
                generator_controller_addr: GENERATOR_CONTROLLER.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        // A connection with invalid ordering is not allowed
        let channel = mock_channel(
            "wasm.hub",
            "channel-2",
            "wasm.unknown_contract",
            "channel-7",
            IbcOrder::Ordered,
            "non-astroport-v1",
        );
        let open_msg = IbcChannelOpenMsg::new_init(channel);
        let err = ibc_channel_open(deps.as_mut(), env.clone(), open_msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::Std(StdError::generic_err(
                "Ordering is invalid. The channel must be unordered"
            ))
        );

        // A connection with invalid version is not allowed
        let channel = mock_channel(
            "wasm.hub",
            "channel-2",
            "wasm.unknown_contract",
            "channel-7",
            IbcOrder::Unordered,
            "non-astroport-v1",
        );
        let open_msg = IbcChannelOpenMsg::new_init(channel);
        let err = ibc_channel_open(deps.as_mut(), env.clone(), open_msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::Std(StdError::generic_err(
                "Must set version to `astroport-outpost-v1`"
            ))
        );

        // A connection with correct settings is allowed
        let channel = mock_channel(
            "wasm.hub",
            "channel-2",
            "wasm.unknown_contract",
            "channel-7",
            IbcOrder::Unordered,
            IBC_APP_VERSION,
        );
        let open_msg = IbcChannelOpenMsg::new_init(channel);
        ibc_channel_open(deps.as_mut(), env, open_msg).unwrap();

        // let connect_msg = IbcChannelConnectMsg::new_ack(channel, IBC_APP_VERSION);
        // ibc_channel_connect(deps.as_mut(), env.clone(), connect_msg).unwrap();
    }

    // Test Cases:
    //
    // Expect Success
    //      - Creating a channel with an allowed Outpost
    //
    // Expect Error
    //      - Attempt to connect a channel with an invalid version
    //      - Attempt to connect a channel before registering an Outpost
    //      - Attempt to connect a channel with an unauthorize Outpost address
    #[test]
    fn ibc_connect_channel() {
        let (mut deps, env, info) = mock_all(OWNER);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::hub::InstantiateMsg {
                owner: OWNER.to_string(),
                assembly_addr: ASSEMBLY.to_string(),
                cw20_ics20_addr: CW20ICS20.to_string(),
                staking_addr: STAKING.to_string(),
                generator_controller_addr: GENERATOR_CONTROLLER.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        // Opening a connection with unknown contracts is allowed
        let channel = mock_channel(
            "wasm.hub",
            "channel-2",
            "wasm.unknown_contract",
            "channel-7",
            IbcOrder::Unordered,
            IBC_APP_VERSION,
        );
        // This should pass
        let open_msg = IbcChannelOpenMsg::new_init(channel.clone());
        ibc_channel_open(deps.as_mut(), env.clone(), open_msg).unwrap();
        let connect_msg = IbcChannelConnectMsg::new_ack(channel, IBC_APP_VERSION);
        ibc_channel_connect(deps.as_mut(), env.clone(), connect_msg).unwrap();

        // Now set the allowed Outpost
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "outpost".to_string(),
                outpost_channel: "channel-3".to_string(),
                cw20_ics20_channel: "channel-1".to_string(),
            },
        )
        .unwrap();

        // Attempting to connect again should now fail
        let channel = mock_channel(
            "wasm.hub",
            "channel-3",
            "wasm.outpost",
            "channel-7",
            IbcOrder::Unordered,
            IBC_APP_VERSION,
        );
        let open_msg = IbcChannelOpenMsg::new_init(channel.clone());
        ibc_channel_open(deps.as_mut(), env.clone(), open_msg).unwrap();
        let connect_msg = IbcChannelConnectMsg::new_ack(channel, IBC_APP_VERSION);
        let err = ibc_channel_connect(deps.as_mut(), env, connect_msg).unwrap_err();

        assert_eq!(
            err,
            ContractError::ChannelAlreadyEstablished {
                channel_id: "channel-3".to_string()
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Packets are acknowledged without error
    #[test]
    fn ibc_ack_packet() {
        let (mut deps, env, info) = mock_all(OWNER);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::hub::InstantiateMsg {
                owner: OWNER.to_string(),
                assembly_addr: ASSEMBLY.to_string(),
                cw20_ics20_addr: CW20ICS20.to_string(),
                staking_addr: STAKING.to_string(),
                generator_controller_addr: GENERATOR_CONTROLLER.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        // Set up valid IBC channel
        setup_channel(deps.as_mut(), env.clone());

        // Add allowed Outpost
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "outpost".to_string(),
                outpost_channel: "channel-3".to_string(),
                cw20_ics20_channel: "channel-1".to_string(),
            },
        )
        .unwrap();

        // The Hub doesn't do anything with acks, we just check that
        // it doesn't fail
        let ack = IbcAcknowledgement::new(
            to_json_binary(&Response::Result {
                action: None,
                address: None,
                error: None,
            })
            .unwrap(),
        );
        let mint_msg = to_json_binary(&Outpost::MintXAstro {
            receiver: "user".to_owned(),
            amount: Uint128::one(),
        })
        .unwrap();
        let original_packet = IbcPacket::new(
            mint_msg,
            IbcEndpoint {
                port_id: format!("wasm.{}", MOCK_CONTRACT_ADDR),
                channel_id: "channel-3".to_string(),
            },
            IbcEndpoint {
                port_id: "wasm.outpost".to_string(),
                channel_id: "channel-3".to_string(),
            },
            3,
            env.block.time.plus_seconds(10).into(),
        );

        let ack_msg = IbcPacketAckMsg::new(ack, original_packet, Addr::unchecked("relayer"));
        ibc_packet_ack(deps.as_mut(), env, ack_msg).unwrap();
    }

    // Test Cases:
    //
    // Expect Success
    //      - Creating a channel with an allowed Outpost
    //
    // Expect Error
    //      - Attempt to connect a channel with an invalid version
    //      - Attempt to connect a channel before registering an Outpost
    //      - Attempt to connect a channel with an unauthorize Outpost address
    #[test]
    fn ibc_close_channel() {
        let (mut deps, env, info) = mock_all(OWNER);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::hub::InstantiateMsg {
                owner: OWNER.to_string(),
                assembly_addr: ASSEMBLY.to_string(),
                cw20_ics20_addr: CW20ICS20.to_string(),
                staking_addr: STAKING.to_string(),
                generator_controller_addr: GENERATOR_CONTROLLER.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        // Set up a valid IBC channel
        setup_channel(deps.as_mut(), env.clone());

        // Add an allowed Outpost
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "outpost".to_string(),
                outpost_channel: "channel-3".to_string(),
                cw20_ics20_channel: "channel-1".to_string(),
            },
        )
        .unwrap();

        let channel = mock_channel(
            "wasm.hub",
            "channel-3",
            "wasm.outpost",
            "channel-7",
            IbcOrder::Unordered,
            IBC_APP_VERSION,
        );

        let close_msg = IbcChannelCloseMsg::new_init(channel);
        let err = ibc_channel_close(deps.as_mut(), env, close_msg).unwrap_err();

        assert_eq!(err, StdError::generic_err("Closing channel is not allowed"));
    }

    // Test Cases:
    //
    // Expect Success
    //      - Only packets from the whitelisted Outpost contract and channel are allowed
    //
    // Expect Error
    //      - Attempt to send a packet from an invalid counterparty port
    //      - Attempt to send a packet from a valid port but invalid channel
    #[test]
    fn ibc_check_receive_auth() {
        let (mut deps, env, info) = mock_all(OWNER);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::hub::InstantiateMsg {
                owner: OWNER.to_string(),
                assembly_addr: ASSEMBLY.to_string(),
                cw20_ics20_addr: CW20ICS20.to_string(),
                staking_addr: STAKING.to_string(),
                generator_controller_addr: GENERATOR_CONTROLLER.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        // Create a random channel
        // Creating an unauthorised channel is allowed
        let channel = mock_channel(
            "wasm.hub",
            "channel-100",
            "wasm.outpost",
            "channel-150",
            IbcOrder::Unordered,
            IBC_APP_VERSION,
        );
        let open_msg = IbcChannelOpenMsg::new_init(channel.clone());
        ibc_channel_open(deps.as_mut(), env.clone(), open_msg).unwrap();
        let connect_msg = IbcChannelConnectMsg::new_ack(channel, IBC_APP_VERSION);
        ibc_channel_connect(deps.as_mut(), env.clone(), connect_msg).unwrap();

        // Attempt to unstake via the unauthorised channel
        // This must always fail as the port and channel is not whitelisted
        // We don't need to test every type of Hub message as the safety check
        // happens in do_packet_receive which is the entrypoint for all messages
        // being received
        let ibc_unstake_msg = to_json_binary(&Hub::Unstake {
            receiver: "unstaker".to_string(),
            amount: Uint128::from(100u128),
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-100", ibc_unstake_msg.clone());

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env.clone(), msg).unwrap();
        let ack: Response = from_json(&res.acknowledgement).unwrap();
        match ack {
            Response::Result { error, .. } => {
                assert!(
                    error == Some("astroport_hub::state::OutpostChannels not found".to_string())
                );
            }
            _ => panic!("Wrong response type"),
        }

        // Whitelist the Outpost
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "outpost".to_string(),
                outpost_channel: "channel-100".to_string(),
                cw20_ics20_channel: "channel-1".to_string(),
            },
        )
        .unwrap();

        // Attempt to unstake again via an unauthorised Outpost
        let recv_packet = mock_ibc_packet("channel-55", ibc_unstake_msg.clone());
        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env.clone(), msg).unwrap();
        let ack: Response = from_json(&res.acknowledgement).unwrap();
        match ack {
            Response::Result { error, .. } => {
                assert!(error == Some("Unauthorized".to_string()));
            }
            _ => panic!("Wrong response type"),
        }

        // Attempt to unstake via the authorised Outpost
        let recv_packet = mock_ibc_packet("channel-100", ibc_unstake_msg);
        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        ibc_packet_receive(deps.as_mut(), env, msg).unwrap();
    }
}
