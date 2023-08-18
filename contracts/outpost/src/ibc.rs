use cosmwasm_std::{
    ensure, entry_point, from_binary, to_binary, CosmosMsg, Deps, DepsMut, Env,
    Ibc3ChannelOpenResponse, IbcBasicResponse, IbcChannelCloseMsg, IbcChannelConnectMsg,
    IbcChannelOpenMsg, IbcChannelOpenResponse, IbcMsg, IbcOrder, IbcPacketAckMsg,
    IbcPacketReceiveMsg, IbcPacketTimeoutMsg, IbcReceiveResponse, Never, StdError, StdResult,
};

use astroport_governance::interchain::{get_contract_from_ibc_port, Hub, Outpost, Response};

use crate::{
    error::ContractError,
    ibc_failure::handle_failed_messages,
    ibc_mint::handle_ibc_xastro_mint,
    query::get_user_voting_power,
    state::{CONFIG, PENDING_VOTES, PROPOSALS_CACHE},
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
/// We verify that the connection is being made to the configured Hub and
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

    // Only a connection to the Hub is allowed
    let counterparty_port =
        get_contract_from_ibc_port(channel.counterparty_endpoint.port_id.as_str());

    let config = CONFIG.load(deps.storage)?;
    match config.hub_channel {
        Some(channel_id) => {
            return Err(ContractError::ChannelAlreadyEstablished { channel_id });
        }
        None => {
            if counterparty_port != config.hub_addr {
                return Err(ContractError::InvalidSourcePort {
                    invalid: counterparty_port.to_string(),
                    valid: config.hub_addr.to_string(),
                });
            }
        }
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
        // Construct an error acknowledgement that can be handled on the Hub
        let ack_data = to_binary(&Response::new_error(err.to_string())).unwrap();

        Ok(IbcReceiveResponse::new()
            .add_attribute("action", "ibc_packet_receive")
            .add_attribute("error", err.to_string())
            .set_ack(ack_data))
    })
}

/// Process the received packet and return the response
///
/// Packets are expected to be wrapped in the Outpost format, if it doesn't conform
/// it will be failed.
///
/// If a ContractError is returned, it will be wrapped into a Response
/// containing the error to be handled on the Outpost
fn do_packet_receive(
    deps: DepsMut,
    _env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    block_unauthorized_packets(
        deps.as_ref(),
        msg.packet.src.port_id.clone(),
        msg.packet.dest.channel_id.clone(),
    )?;

    // Parse the packet data into a Hub message
    let hub_msg: Outpost = from_binary(&msg.packet.data)?;
    match hub_msg {
        Outpost::MintXAstro { receiver, amount } => handle_ibc_xastro_mint(deps, receiver, amount),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_timeout(
    deps: DepsMut,
    _env: Env,
    msg: IbcPacketTimeoutMsg,
) -> Result<IbcBasicResponse, ContractError> {
    let mut response = IbcBasicResponse::new().add_attribute("action", "ibc_packet_timeout");

    // In case of an IBC timeout we might need to reverse actions similar
    // to failed messages.
    // We look at the original packet to determine what failed and take
    // the appropriate action
    let failed_msg: Hub = from_binary(&msg.packet.data)?;
    response = handle_failed_messages(deps, failed_msg, response)?;

    Ok(response)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, ContractError> {
    let mut response = IbcBasicResponse::new().add_attribute("action", "ibc_packet_ack");

    let ack: Result<Response, StdError> = from_binary(&msg.acknowledgement.data);
    match ack {
        Ok(hub_response) => {
            match hub_response {
                Response::QueryProposal(proposal) => {
                    // We cache the proposal ID and start time for future vote
                    // checks without needing to query the Hub again
                    PROPOSALS_CACHE.save(deps.storage, proposal.id.u64(), &proposal)?;

                    // We need to submit the initial vote that triggered this
                    // proposal to be queried from the pending vote cache
                    if let Some(pending_vote) =
                        PENDING_VOTES.may_load(deps.storage, proposal.id.u64())?
                    {
                        let config = CONFIG.load(deps.storage)?;

                        let voting_power = get_user_voting_power(
                            deps.as_ref(),
                            pending_vote.voter.clone(),
                            proposal.start_time,
                        )?;

                        if voting_power.is_zero() {
                            return Err(ContractError::NoVotingPower {
                                address: pending_vote.voter.to_string(),
                            });
                        }

                        let hub_channel = config
                            .hub_channel
                            .ok_or(ContractError::MissingHubChannel {})?;

                        // Construct the vote message and submit it to the Hub
                        let cast_vote = Hub::CastAssemblyVote {
                            proposal_id: proposal.id.u64(),
                            vote_option: pending_vote.vote_option,
                            voter: pending_vote.voter.clone(),
                            voting_power,
                        };
                        let hub_msg = CosmosMsg::Ibc(IbcMsg::SendPacket {
                            channel_id: hub_channel,
                            data: to_binary(&cast_vote)?,
                            timeout: env
                                .block
                                .time
                                .plus_seconds(config.ibc_timeout_seconds)
                                .into(),
                        });
                        response = response
                            .add_message(hub_msg)
                            .add_attribute("action", cast_vote.to_string())
                            .add_attribute("user", pending_vote.voter.to_string());

                        // Remove this pending vote from the cache
                        PENDING_VOTES.remove(deps.storage, proposal.id.u64());
                    }

                    response = response
                        .add_attribute("hub_response", "query_response")
                        .add_attribute("response_type", "proposal")
                        .add_attribute("proposal_id", proposal.id.to_string())
                        .add_attribute("proposal_start", proposal.start_time.to_string())
                }
                Response::Result {
                    action,
                    address,
                    error,
                } => {
                    response = response
                        .add_attribute("action", action.unwrap_or_else(|| "unknown".to_string()))
                        .add_attribute("user", address.unwrap_or_else(|| "unknown".to_string()))
                        .add_attribute("err", error.unwrap_or_else(|| "unknown".to_string()))
                }
            }
        }
        Err(err) => {
            // In case of error, ack.data will be in the format similar to
            // {"error":"ABCI code: 5: error handling packet: see events for details"}
            // but the events do not contain the details
            //
            // Instead we look at the original packet to determine what failed,
            // the reason for the failure can't be determined at this time due
            // to a limitation in wasmd/wasmvm. For us we just need to know what failed,
            // the reason is not required to continue
            // See https://github.com/CosmWasm/cosmwasm/issues/1707

            let raw_error = base64::encode(&msg.acknowledgement.data);
            // Attach the errors to the response
            response = response
                .add_attribute("raw_error", raw_error)
                .add_attribute("ack_error", err.to_string());

            // Handle the possible failures
            let original: Hub = from_binary(&msg.original_packet.data)?;
            response = handle_failed_messages(deps, original, response)?;
        }
    }
    Ok(response)
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

/// Checks the provided port against the known Hub.
///
/// If the port doesn't exist, this function will  return an error, effectively blocking the packet.
fn block_unauthorized_packets(
    deps: Deps,
    port_id: String,
    channel_id: String,
) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let counterparty_port = get_contract_from_ibc_port(port_id.as_str());
    ensure!(
        config.hub_addr == counterparty_port,
        ContractError::Unauthorized {}
    );

    ensure!(
        config.hub_channel == Some(channel_id),
        ContractError::Unauthorized {}
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use astroport_governance::interchain::ProposalSnapshot;
    use cosmwasm_std::{
        testing::{mock_info, MOCK_CONTRACT_ADDR},
        Addr, IbcAcknowledgement, IbcEndpoint, IbcPacket, ReplyOn, SubMsg, Uint128, Uint64,
    };

    use super::*;
    use crate::{
        contract::instantiate,
        execute::execute,
        mock::{mock_all, mock_channel, setup_channel, HUB, OWNER, VXASTRO_TOKEN, XASTRO_TOKEN},
        state::PendingVote,
    };

    // Test Cases:
    //
    // Expect Success
    //      - Creating a channel with correct settings
    //
    // Expect Error
    //      - Attempt to create a channel with an invalid version
    //      - Attempt to create a channel with an invalid ordering
    #[test]
    fn ibc_open_channel() {
        let (mut deps, env, info) = mock_all(OWNER);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::outpost::InstantiateMsg {
                owner: OWNER.to_string(),
                xastro_token_addr: XASTRO_TOKEN.to_string(),
                vxastro_token_addr: VXASTRO_TOKEN.to_string(),
                hub_addr: HUB.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        // A connection with invalid ordering is not allowed
        let channel = mock_channel(
            "wasm.outpost",
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
            "wasm.outpost",
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
            "wasm.outpost",
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
            astroport_governance::outpost::InstantiateMsg {
                owner: OWNER.to_string(),
                xastro_token_addr: XASTRO_TOKEN.to_string(),
                vxastro_token_addr: VXASTRO_TOKEN.to_string(),
                hub_addr: HUB.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        // Opening a connection with unknown contracts is not allowed
        let channel = mock_channel(
            "wasm.outpost",
            "channel-2",
            "wasm.unknown_contract",
            "channel-7",
            IbcOrder::Unordered,
            IBC_APP_VERSION,
        );
        let open_msg = IbcChannelOpenMsg::new_init(channel.clone());
        ibc_channel_open(deps.as_mut(), env.clone(), open_msg).unwrap();
        let connect_msg = IbcChannelConnectMsg::new_ack(channel, IBC_APP_VERSION);
        let err = ibc_channel_connect(deps.as_mut(), env.clone(), connect_msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::InvalidSourcePort {
                invalid: "unknown_contract".to_string(),
                valid: "hub".to_string()
            }
        );

        // Opening a connection with the hub is allowed
        let channel = mock_channel(
            "wasm.outpost",
            "channel-3",
            format!("wasm.{}", HUB).as_str(),
            "channel-7",
            IbcOrder::Unordered,
            IBC_APP_VERSION,
        );

        // Attempt to connect with the wrong IBC app version
        let open_msg = IbcChannelOpenMsg::new_init(channel.clone());
        ibc_channel_open(deps.as_mut(), env.clone(), open_msg).unwrap();
        let connect_msg = IbcChannelConnectMsg::new_ack(channel.clone(), "WRONG_VERSION");
        let err = ibc_channel_connect(deps.as_mut(), env.clone(), connect_msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::Std(StdError::generic_err(format!(
                "Counterparty version must be `{}`",
                IBC_APP_VERSION
            )))
        );

        let open_msg = IbcChannelOpenMsg::new_init(channel.clone());
        ibc_channel_open(deps.as_mut(), env.clone(), open_msg).unwrap();
        let connect_msg = IbcChannelConnectMsg::new_ack(channel, IBC_APP_VERSION);
        ibc_channel_connect(deps.as_mut(), env.clone(), connect_msg).unwrap();

        // Update config with new channel
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::outpost::ExecuteMsg::UpdateConfig {
                hub_addr: None,
                hub_channel: Some("channel-3".to_string()),
                ibc_timeout_seconds: None,
            },
        )
        .unwrap();

        // Attempting to open the channel again is not allowed
        let channel = mock_channel(
            "wasm.outpost",
            "channel-3",
            format!("wasm.{}", HUB).as_str(),
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
                channel_id: "channel-3".to_string(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Query results returned in the acknoledgement data is processed correctly
    #[test]
    fn ibc_ack_packet() {
        let (mut deps, env, info) = mock_all(OWNER);

        let proposal_id = 1u64;
        let user = "user";
        let voting_power = 1000u64;
        let ibc_timeout_seconds = 10u64;

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::outpost::InstantiateMsg {
                owner: OWNER.to_string(),
                xastro_token_addr: XASTRO_TOKEN.to_string(),
                vxastro_token_addr: VXASTRO_TOKEN.to_string(),
                hub_addr: HUB.to_string(),
                ibc_timeout_seconds,
            },
        )
        .unwrap();

        // Set up valid Hub
        setup_channel(deps.as_mut(), env.clone());

        // Update config with new channel
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::outpost::ExecuteMsg::UpdateConfig {
                hub_addr: None,
                hub_channel: Some("channel-3".to_string()),
                ibc_timeout_seconds: None,
            },
        )
        .unwrap();

        // The pending would be stored in the contract before the query is sent
        let pending_vote = PendingVote {
            proposal_id,
            voter: Addr::unchecked(user),
            vote_option: astroport_governance::assembly::ProposalVoteOption::For,
        };
        PENDING_VOTES
            .save(&mut deps.storage, proposal_id, &pending_vote)
            .unwrap();

        let proposal_response = Response::QueryProposal(ProposalSnapshot {
            id: Uint64::from(proposal_id),
            start_time: 1689942949u64,
        });

        let ack = IbcAcknowledgement::new(to_binary(&proposal_response).unwrap());
        let mint_msg = to_binary(&Outpost::MintXAstro {
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
                port_id: format!("wasm.{}", HUB),
                channel_id: "channel-7".to_string(),
            },
            3,
            env.block.time.plus_seconds(10).into(),
        );

        let ack_msg = IbcPacketAckMsg::new(ack, original_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_ack(deps.as_mut(), env.clone(), ack_msg).unwrap();

        // If we received the proposal, we can now submit the vote
        assert_eq!(res.messages.len(), 1);

        // Build the expected message
        let ibc_message = to_binary(&Hub::CastAssemblyVote {
            proposal_id,
            voter: Addr::unchecked(user),
            vote_option: astroport_governance::assembly::ProposalVoteOption::For,
            voting_power: Uint128::from(voting_power),
        })
        .unwrap();

        // Ensure a vote is emitted
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: IbcMsg::SendPacket {
                    channel_id: "channel-3".to_string(),
                    data: ibc_message,
                    timeout: env.block.time.plus_seconds(ibc_timeout_seconds).into(),
                }
                .into(),
            }
        );
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
            astroport_governance::outpost::InstantiateMsg {
                owner: OWNER.to_string(),
                xastro_token_addr: XASTRO_TOKEN.to_string(),
                vxastro_token_addr: VXASTRO_TOKEN.to_string(),
                hub_addr: HUB.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        setup_channel(deps.as_mut(), env.clone());

        let channel = mock_channel(
            "wasm.outpost",
            "channel-3",
            "wasm.hub",
            "channel-7",
            IbcOrder::Unordered,
            IBC_APP_VERSION,
        );

        let close_msg = IbcChannelCloseMsg::new_init(channel);
        let err = ibc_channel_close(deps.as_mut(), env, close_msg).unwrap_err();

        assert_eq!(err, StdError::generic_err("Closing channel is not allowed"));
    }
}
