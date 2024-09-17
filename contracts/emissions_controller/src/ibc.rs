#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, wasm_execute, DepsMut, Env, Ibc3ChannelOpenResponse, IbcBasicResponse,
    IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcPacketAckMsg,
    IbcPacketReceiveMsg, IbcPacketTimeoutMsg, IbcReceiveResponse, Never, StdError, StdResult,
};

use astroport_governance::assembly;
use astroport_governance::emissions_controller::consts::{IBC_APP_VERSION, IBC_ORDERING};
use astroport_governance::emissions_controller::msg::{
    ack_fail, ack_ok, IbcAckResult, VxAstroIbcMsg,
};

use crate::error::ContractError;
use crate::execute::{handle_update_user, handle_vote};
use crate::state::{get_all_outposts, CONFIG};

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
    if let Some(counter_version) = msg.counterparty_version() {
        if counter_version != IBC_APP_VERSION {
            return Err(StdError::generic_err(format!(
                "Counterparty version must be `{IBC_APP_VERSION}`"
            )));
        }
    }

    let channel = msg.channel();

    Ok(IbcBasicResponse::new()
        .add_attribute("action", "ibc_connect")
        .add_attribute("channel_id", &channel.endpoint.channel_id))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, Never> {
    do_packet_receive(deps, env, msg).or_else(|err| {
        Ok(IbcReceiveResponse::new()
            .add_attribute("action", "ibc_packet_receive")
            .set_ack(ack_fail(err)))
    })
}

pub fn do_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    // Ensure this outpost was ever registered
    let (prefix, outpost) = get_all_outposts(deps.storage)?
        .into_iter()
        .find_map(|(prefix, outpost)| {
            outpost.params.as_ref().and_then(|params| {
                if msg.packet.dest.channel_id == params.voting_channel {
                    Some((prefix.clone(), outpost.clone()))
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| {
            StdError::generic_err(format!(
                "Unknown outpost with {} voting channel",
                msg.packet.dest.channel_id
            ))
        })?;

    let ibc_msg: VxAstroIbcMsg = from_json(&msg.packet.data)?;

    if outpost.jailed {
        match ibc_msg {
            VxAstroIbcMsg::UpdateUserVotes {
                voter,
                voting_power,
                is_unlock: true,
            } => handle_update_user(deps.storage, env, voter.as_str(), voting_power).map(
                |orig_response| {
                    IbcReceiveResponse::new()
                        .add_attributes(orig_response.attributes)
                        .set_ack(ack_ok())
                },
            ),
            _ => Err(ContractError::JailedOutpost { prefix }),
        }
    } else {
        match ibc_msg {
            VxAstroIbcMsg::EmissionsVote {
                voter,
                voting_power,
                votes,
            } => handle_vote(deps, env, &voter, voting_power, votes).map(|orig_response| {
                IbcReceiveResponse::new()
                    .add_attributes(orig_response.attributes)
                    .set_ack(ack_ok())
            }),
            VxAstroIbcMsg::UpdateUserVotes {
                voter,
                voting_power,
                ..
            } => handle_update_user(deps.storage, env, voter.as_str(), voting_power).map(
                |orig_response| {
                    IbcReceiveResponse::new()
                        .add_attributes(orig_response.attributes)
                        .set_ack(ack_ok())
                },
            ),
            VxAstroIbcMsg::GovernanceVote {
                voter,
                voting_power,
                proposal_id,
                vote,
            } => {
                let config = CONFIG.load(deps.storage)?;
                let cast_vote_msg = wasm_execute(
                    config.assembly,
                    &assembly::ExecuteMsg::CastVoteOutpost {
                        voter,
                        voting_power,
                        proposal_id,
                        vote,
                    },
                    vec![],
                )?;

                Ok(IbcReceiveResponse::new()
                    .add_message(cast_vote_msg)
                    .set_ack(ack_ok()))
            }
            VxAstroIbcMsg::RegisterProposal { .. } => {
                unreachable!("Hub can't receive RegisterProposal message")
            }
        }
    }
}

#[cfg(not(tarpaulin_include))]
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    _deps: DepsMut,
    _env: Env,
    msg: IbcPacketAckMsg,
) -> StdResult<IbcBasicResponse> {
    match from_json(msg.acknowledgement.data)? {
        IbcAckResult::Ok(_) => {
            Ok(IbcBasicResponse::default().add_attribute("action", "ibc_packet_ack"))
        }
        IbcAckResult::Error(err) => Ok(IbcBasicResponse::default().add_attribute("error", err)),
    }
}

#[cfg(not(tarpaulin_include))]
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_timeout(
    _deps: DepsMut,
    _env: Env,
    _msg: IbcPacketTimeoutMsg,
) -> StdResult<IbcBasicResponse> {
    Ok(IbcBasicResponse::default().add_attribute("action", "ibc_packet_timeout"))
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

#[cfg(test)]
mod unit_tests {
    use std::collections::HashMap;
    use std::marker::PhantomData;

    use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage};
    use cosmwasm_std::{
        to_json_binary, Addr, Decimal, IbcChannel, IbcEndpoint, IbcOrder, IbcPacket, IbcTimeout,
        OwnedDeps, Timestamp,
    };
    use neutron_sdk::bindings::query::NeutronQuery;

    use astroport_governance::assembly::ProposalVoteOption;
    use astroport_governance::emissions_controller::hub::{
        OutpostInfo, OutpostParams, VotedPoolInfo,
    };
    use astroport_governance::emissions_controller::msg::IbcAckResult;

    use crate::state::{OUTPOSTS, POOLS_WHITELIST, VOTED_POOLS};

    use super::*;

    pub fn mock_custom_dependencies() -> OwnedDeps<MockStorage, MockApi, MockQuerier, NeutronQuery>
    {
        OwnedDeps {
            storage: MockStorage::default(),
            api: MockApi::default(),
            querier: MockQuerier::default(),
            custom_query_type: PhantomData,
        }
    }

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

    #[test]
    fn test_packet_receive() {
        let mut deps = mock_custom_dependencies();

        let voting_msg = VxAstroIbcMsg::EmissionsVote {
            voter: "osmo1voter".to_string(),
            voting_power: 1000u128.into(),
            votes: HashMap::from([("osmo1pool1".to_string(), Decimal::one())]),
        };
        let packet = IbcPacket::new(
            to_json_binary(&voting_msg).unwrap(),
            IbcEndpoint {
                port_id: "".to_string(),
                channel_id: "".to_string(),
            },
            IbcEndpoint {
                port_id: "".to_string(),
                channel_id: "channel-2".to_string(),
            },
            1,
            IbcTimeout::with_timestamp(Timestamp::from_seconds(100)),
        );
        let ibc_msg = IbcPacketReceiveMsg::new(packet, Addr::unchecked("doesnt matter"));

        let resp =
            ibc_packet_receive(deps.as_mut().into_empty(), mock_env(), ibc_msg.clone()).unwrap();
        let ack_err: IbcAckResult = from_json(resp.acknowledgement).unwrap();
        assert_eq!(
            ack_err,
            IbcAckResult::Error(
                "Generic error: Unknown outpost with channel-2 voting channel".to_string()
            )
        );

        // Mock added outpost and whitelist
        OUTPOSTS
            .save(
                deps.as_mut().storage,
                "osmo",
                &OutpostInfo {
                    params: Some(OutpostParams {
                        emissions_controller: "".to_string(),
                        voting_channel: "channel-2".to_string(),
                        ics20_channel: "".to_string(),
                    }),
                    astro_denom: "".to_string(),
                    astro_pool_config: None,
                    jailed: false,
                },
            )
            .unwrap();
        POOLS_WHITELIST
            .save(deps.as_mut().storage, &vec!["osmo1pool1".to_string()])
            .unwrap();

        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1724922008);

        VOTED_POOLS
            .save(
                deps.as_mut().storage,
                "osmo1pool1",
                &VotedPoolInfo {
                    init_ts: env.block.time.seconds(),
                    voting_power: 0u128.into(),
                },
                env.block.time.seconds(),
            )
            .unwrap();

        let resp =
            ibc_packet_receive(deps.as_mut().into_empty(), env.clone(), ibc_msg.clone()).unwrap();
        let ack_err: IbcAckResult = from_json(resp.acknowledgement).unwrap();
        assert_eq!(ack_err, IbcAckResult::Ok(b"ok".into()));

        // The same user can only vote at the next epoch
        let resp = ibc_packet_receive(deps.as_mut().into_empty(), env.clone(), ibc_msg).unwrap();
        let ack_err: IbcAckResult = from_json(resp.acknowledgement).unwrap();
        assert_eq!(
            ack_err,
            IbcAckResult::Error("Next time you can change your vote is at 1725840000".to_string())
        );

        // Voting from random channel is not possible
        let packet = IbcPacket::new(
            to_json_binary(&voting_msg).unwrap(),
            IbcEndpoint {
                port_id: "".to_string(),
                channel_id: "".to_string(),
            },
            IbcEndpoint {
                port_id: "".to_string(),
                channel_id: "channel-3".to_string(),
            },
            1,
            IbcTimeout::with_timestamp(Timestamp::from_seconds(100)),
        );
        let ibc_msg = IbcPacketReceiveMsg::new(packet, Addr::unchecked("doesnt matter"));
        let resp = ibc_packet_receive(deps.as_mut().into_empty(), env.clone(), ibc_msg).unwrap();
        let ack_err: IbcAckResult = from_json(resp.acknowledgement).unwrap();
        assert_eq!(
            ack_err,
            IbcAckResult::Error(
                "Generic error: Unknown outpost with channel-3 voting channel".to_string()
            )
        );

        // However, his voting power can be updated any time
        let update_msg = VxAstroIbcMsg::UpdateUserVotes {
            voter: "osmo1voter".to_string(),
            voting_power: 2000u128.into(),
            is_unlock: false,
        };
        let packet = IbcPacket::new(
            to_json_binary(&update_msg).unwrap(),
            IbcEndpoint {
                port_id: "".to_string(),
                channel_id: "".to_string(),
            },
            IbcEndpoint {
                port_id: "".to_string(),
                channel_id: "channel-2".to_string(),
            },
            1,
            IbcTimeout::with_timestamp(Timestamp::from_seconds(100)),
        );
        let ibc_msg = IbcPacketReceiveMsg::new(packet, Addr::unchecked("doesnt matter"));
        let resp = ibc_packet_receive(deps.as_mut().into_empty(), env.clone(), ibc_msg).unwrap();
        let ack_err: IbcAckResult = from_json(resp.acknowledgement).unwrap();
        assert_eq!(ack_err, IbcAckResult::Ok(b"ok".into()));
    }

    #[test]
    fn test_jailed_outpost() {
        let mut deps = mock_custom_dependencies();

        // Mock jailed outpost
        OUTPOSTS
            .save(
                deps.as_mut().storage,
                "osmo",
                &OutpostInfo {
                    params: Some(OutpostParams {
                        emissions_controller: "".to_string(),
                        voting_channel: "channel-2".to_string(),
                        ics20_channel: "".to_string(),
                    }),
                    astro_denom: "".to_string(),
                    astro_pool_config: None,
                    jailed: true,
                },
            )
            .unwrap();

        for (msg, is_error) in [
            (
                VxAstroIbcMsg::EmissionsVote {
                    voter: "osmo1voter".to_string(),
                    voting_power: 1000u128.into(),
                    votes: HashMap::from([("osmo1pool1".to_string(), Decimal::one())]),
                },
                true,
            ),
            (
                VxAstroIbcMsg::GovernanceVote {
                    voter: "osmo1voter".to_string(),
                    voting_power: 1000u128.into(),
                    proposal_id: 1,
                    vote: ProposalVoteOption::For,
                },
                true,
            ),
            (
                VxAstroIbcMsg::UpdateUserVotes {
                    voter: "osmo1voter".to_string(),
                    voting_power: 2000u128.into(),
                    is_unlock: false,
                },
                true,
            ),
            (
                VxAstroIbcMsg::UpdateUserVotes {
                    voter: "osmo1voter".to_string(),
                    voting_power: 0u128.into(),
                    is_unlock: true,
                },
                false,
            ),
        ] {
            let packet = IbcPacket::new(
                to_json_binary(&msg).unwrap(),
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: "".to_string(),
                },
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: "channel-2".to_string(),
                },
                1,
                IbcTimeout::with_timestamp(Timestamp::from_seconds(100)),
            );
            let ibc_msg = IbcPacketReceiveMsg::new(packet, Addr::unchecked("doesnt matter"));

            let resp = ibc_packet_receive(deps.as_mut().into_empty(), mock_env(), ibc_msg).unwrap();
            let ack_err: IbcAckResult = from_json(resp.acknowledgement).unwrap();

            if is_error {
                assert_eq!(
                    ack_err,
                    IbcAckResult::Error(
                        ContractError::JailedOutpost {
                            prefix: "osmo".to_string()
                        }
                        .to_string()
                    )
                );
            } else {
                assert_eq!(ack_err, IbcAckResult::Ok(b"ok".into()));
            }
        }
    }
}
