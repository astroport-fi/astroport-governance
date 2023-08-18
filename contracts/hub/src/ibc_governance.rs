use cosmwasm_std::{to_binary, Addr, DepsMut, Env, IbcReceiveResponse, Uint128, WasmMsg};

use astroport_governance::{
    assembly::{Proposal, ProposalVoteOption},
    generator_controller_lite,
    interchain::Response,
};

use crate::{
    error::ContractError,
    state::{channel_balance_at, CONFIG},
};

/// Handle an IBC message to cast a vote on an Assembly proposal from an Outpost
/// and return an IBC acknowledgement
///
/// The Outpost is responsible for checking and sending the voting power of the
/// voter, we add an additional check to make sure that the voting power is not
/// more than the xASTRO minted remotely via this channel
pub fn handle_ibc_cast_assembly_vote(
    deps: DepsMut,
    outpost_channel: String,
    proposal_id: u64,
    voter: Addr,
    vote_option: ProposalVoteOption,
    voting_power: Uint128,
) -> Result<IbcReceiveResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Cast the vote in the Assembly
    let vote_msg = astroport_governance::assembly::ExecuteMsg::CastOutpostVote {
        proposal_id,
        voter: voter.to_string(),
        vote: vote_option,
        voting_power,
    };
    let wasm_msg = WasmMsg::Execute {
        contract_addr: config.assembly_addr.to_string(),
        msg: to_binary(&vote_msg)?,
        funds: vec![],
    };

    // Assert that the voting power does not exceed the xASTRO minted via this channel
    // at the time the proposal was created
    let proposal: Proposal = deps.querier.query_wasm_smart(
        config.assembly_addr,
        &astroport_governance::assembly::QueryMsg::Proposal { proposal_id },
    )?;

    let xastro_balance = channel_balance_at(deps.storage, &outpost_channel, proposal.start_time)?;

    if voting_power > xastro_balance {
        return Err(ContractError::InvalidVotingPower {});
    }

    // If the vote succeeds, the ack will be sent back to the Outpost
    let ack_data = to_binary(&Response::new_success(
        "cast_assembly_vote".to_owned(),
        voter.to_string(),
    ))?;

    Ok(IbcReceiveResponse::new()
        .add_message(wasm_msg)
        .set_ack(ack_data))
}

/// Handle an IBC message to cast a vote on emissions during a voting period
/// from an Outpost and return an IBC acknowledgement
///
/// The Outpost is responsible for checking and sending the voting power of the
/// voter, we add an additional check to make sure that the voting power is not
/// more than the xASTRO minted remotely via this channel. vxASTRO lite does
/// not boost voting power and must be equal to the deposit
pub fn handle_ibc_cast_emissions_vote(
    deps: DepsMut,
    env: Env,
    outpost_channel: String,
    voter: Addr,
    voting_power: Uint128,
    votes: Vec<(String, u16)>,
) -> Result<IbcReceiveResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Cast the emissions vote
    let vote_msg = generator_controller_lite::ExecuteMsg::OutpostVote {
        voter: voter.to_string(),
        votes,
        voting_power,
    };
    let msg = WasmMsg::Execute {
        contract_addr: config.generator_controller_addr.to_string(),
        msg: to_binary(&vote_msg)?,
        funds: vec![],
    };

    // Assert that the voting power does not exceed the xASTRO minted via this channel at the current block
    let xastro_balance =
        channel_balance_at(deps.storage, &outpost_channel, env.block.time.seconds())?;
    if voting_power > xastro_balance {
        return Err(ContractError::InvalidVotingPower {});
    }

    // If the vote succeeds, the ack will be sent back to the Outpost
    let ack_data = to_binary(&Response::new_success(
        "cast_emissions_vote".to_owned(),
        voter.to_string(),
    ))?;

    Ok(IbcReceiveResponse::new().add_message(msg).set_ack(ack_data))
}

/// Handle an IBC message to kick an unlocked voter from the Outpost.
///
/// We rely on the Outpost to verify the unlock before sending it here. If this
/// transaction succeeds, the voting power will be removed immediately
pub fn handle_ibc_unlock(deps: DepsMut, user: Addr) -> Result<IbcReceiveResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Remove the vxASTRO voter's voting power
    let unlock_msg = generator_controller_lite::ExecuteMsg::KickUnlockedOutpostVoter {
        unlocked_voter: user.to_string(),
    };
    let msg = WasmMsg::Execute {
        contract_addr: config.generator_controller_addr.to_string(),
        msg: to_binary(&unlock_msg)?,
        funds: vec![],
    };

    // If the unlock succeeds, the ack will be sent back to the Outpost
    let ack_data = to_binary(&Response::new_success(
        "unlock".to_owned(),
        user.to_string(),
    ))?;

    Ok(IbcReceiveResponse::new().add_message(msg).set_ack(ack_data))
}

/// Handle an IBC message to kick a blacklisted voter from the Outpost.
///
/// We rely on the Outpost to verify the blacklist before sending it here. If this
/// transaction succeeds, the voting power will be removed immediately
pub fn handle_ibc_blacklisted(
    deps: DepsMut,
    user: Addr,
) -> Result<IbcReceiveResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Remove the vxASTRO voter's voting power
    let blacklist_msg = generator_controller_lite::ExecuteMsg::KickBlacklistedVoters {
        blacklisted_voters: vec![user.to_string()],
    };
    let msg = WasmMsg::Execute {
        contract_addr: config.generator_controller_addr.to_string(),
        msg: to_binary(&blacklist_msg)?,
        funds: vec![],
    };

    // If the vote succeeds, the ack will be sent back to the Outpost
    let ack_data = to_binary(&Response::new_success(
        "kick_blacklisted".to_owned(),
        user.to_string(),
    ))?;

    Ok(IbcReceiveResponse::new().add_message(msg).set_ack(ack_data))
}

#[cfg(test)]
mod tests {
    use astroport_governance::interchain::Hub;
    use cosmwasm_std::{
        from_binary,
        testing::{mock_info, MOCK_CONTRACT_ADDR},
        IbcPacketReceiveMsg, Reply, ReplyOn, SubMsg, SubMsgResponse, SubMsgResult,
    };
    use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

    use super::*;
    use crate::{
        contract::instantiate,
        execute::execute,
        ibc::ibc_packet_receive,
        mock::{
            mock_all, mock_ibc_packet, setup_channel, ASSEMBLY, ASTRO_TOKEN, CW20ICS20,
            GENERATOR_CONTROLLER, OWNER, STAKING,
        },
        reply::{reply, STAKE_ID},
    };

    // Test Cases:
    //
    // Expect Success
    //      - Submitting the vote results in an Assembly message
    //
    // Expect Error
    //      - An error is returned instead
    #[test]
    fn ibc_assembly_vote() {
        let (mut deps, env, info) = mock_all(OWNER);

        let voter = "voter1234";
        let voting_power = Uint128::from(100u128);

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

        // Stake tokens to ensure the channel has a non-zero balance
        let user1 = "user1";
        let user1_funds = Uint128::from(100u128);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
                    channel: "channel-1".to_string(),
                    sender: user1.to_string(),
                    receiver: MOCK_CONTRACT_ADDR.to_owned(),
                    memo: "{\"stake\":{}}".to_string(),
                })
                .unwrap(),
            }),
        )
        .unwrap();

        // Verify that the stake message matches the expected message
        let stake_msg = to_binary(&astroport::staking::Cw20HookMsg::Enter {}).unwrap();
        let send_msg = to_binary(&Cw20ExecuteMsg::Send {
            contract: STAKING.to_string(),
            amount: user1_funds,
            msg: stake_msg,
        })
        .unwrap();

        // Verify that we see a stake message reply
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 9000,
                gas_limit: None,
                reply_on: ReplyOn::Success,
                msg: WasmMsg::Execute {
                    contract_addr: ASTRO_TOKEN.to_string(),
                    msg: send_msg,
                    funds: vec![],
                }
                .into(),
            }
        );

        // Construct the reply from the staking contract that will be returned
        // to the contract
        let stake_reply = Reply {
            id: STAKE_ID,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                data: None,
            }),
        };

        let res = reply(deps.as_mut(), env.clone(), stake_reply).unwrap();

        // We must have one IBC message
        assert_eq!(res.messages.len(), 1);

        // At this point we now have 100 staked tokens
        // We can test that voting power may not exceed this
        let proposal_id = 1u64;
        let vote_option = ProposalVoteOption::For;

        // Attempt a vote with double the voting power
        let ibc_vote = to_binary(&Hub::CastAssemblyVote {
            proposal_id,
            voter: Addr::unchecked(voter),
            vote_option: vote_option.clone(),
            voting_power: voting_power.checked_add(Uint128::from(100u128)).unwrap(),
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_vote);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env.clone(), msg).unwrap();

        let hub_respone: Response = from_binary(&res.acknowledgement).unwrap();
        match hub_respone {
            Response::Result { error, .. } => {
                assert_eq!(
                    error,
                    Some("Voting power exceeds channel balance".to_string())
                );
            }
            _ => panic!("Wrong response type"),
        }

        // Attempt a vote with the correct voting power
        let ibc_vote = to_binary(&Hub::CastAssemblyVote {
            proposal_id,
            voter: Addr::unchecked(voter),
            vote_option,
            voting_power,
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_vote);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env, msg).unwrap();

        let hub_respone: Response = from_binary(&res.acknowledgement).unwrap();
        match hub_respone {
            Response::Result { error, .. } => {
                assert!(error.is_none());
            }
            _ => panic!("Wrong response type"),
        }

        assert_eq!(res.messages.len(), 1);

        let assembly_msg = to_binary(
            &astroport_governance::assembly::ExecuteMsg::CastOutpostVote {
                proposal_id,
                vote: ProposalVoteOption::For,
                voter: voter.to_string(),
                voting_power,
            },
        )
        .unwrap();

        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: WasmMsg::Execute {
                    contract_addr: ASSEMBLY.to_string(),
                    msg: assembly_msg,
                    funds: vec![],
                }
                .into(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Submitting the vote results in a Generator controller message
    //
    // Expect Error
    //      - An error is returned instead
    #[test]
    fn ibc_emissions_vote() {
        let (mut deps, env, info) = mock_all(OWNER);

        let voter = "voter1234";
        let voting_power = Uint128::from(100u128);
        let votes = vec![("pooladdress".to_string(), 10000)];

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

        // Voting must fail if the channel balance in insufficient
        let ibc_unstake = to_binary(&Hub::CastEmissionsVote {
            voter: Addr::unchecked(voter),
            voting_power,
            votes: votes.clone(),
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_unstake);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env.clone(), msg).unwrap();

        let hub_respone: Response = from_binary(&res.acknowledgement).unwrap();
        match hub_respone {
            Response::Result { error, .. } => {
                assert_eq!(
                    error.unwrap(),
                    "Voting power exceeds channel balance".to_string()
                );
            }
            _ => panic!("Wrong response type"),
        }

        // Stake some ASTRO remotely
        // Stake tokens to ensure the channel has a non-zero balance
        let user1 = "user1";
        let user1_funds = Uint128::from(100u128);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
                    channel: "channel-1".to_string(),
                    sender: user1.to_string(),
                    receiver: MOCK_CONTRACT_ADDR.to_owned(),
                    memo: "{\"stake\":{}}".to_string(),
                })
                .unwrap(),
            }),
        )
        .unwrap();

        // Verify that the stake message matches the expected message
        let stake_msg = to_binary(&astroport::staking::Cw20HookMsg::Enter {}).unwrap();
        let send_msg = to_binary(&Cw20ExecuteMsg::Send {
            contract: STAKING.to_string(),
            amount: user1_funds,
            msg: stake_msg,
        })
        .unwrap();

        // Verify that we see a stake message reply
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 9000,
                gas_limit: None,
                reply_on: ReplyOn::Success,
                msg: WasmMsg::Execute {
                    contract_addr: ASTRO_TOKEN.to_string(),
                    msg: send_msg,
                    funds: vec![],
                }
                .into(),
            }
        );

        // Construct the reply from the staking contract that will be returned
        // to the contract
        let stake_reply = Reply {
            id: STAKE_ID,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                data: None,
            }),
        };

        let res = reply(deps.as_mut(), env.clone(), stake_reply).unwrap();

        // We must have one IBC message
        assert_eq!(res.messages.len(), 1);

        let ibc_vote = to_binary(&Hub::CastEmissionsVote {
            voter: Addr::unchecked(voter),
            voting_power,
            votes: votes.clone(),
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_vote);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env, msg).unwrap();

        let hub_respone: Response = from_binary(&res.acknowledgement).unwrap();
        match hub_respone {
            Response::Result { error, .. } => {
                assert!(error.is_none(),);
            }
            _ => panic!("Wrong response type"),
        }

        assert_eq!(res.messages.len(), 1);

        let generator_controller_msg = to_binary(
            &astroport_governance::generator_controller_lite::ExecuteMsg::OutpostVote {
                voter: voter.to_string(),
                voting_power,
                votes,
            },
        )
        .unwrap();

        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: WasmMsg::Execute {
                    contract_addr: GENERATOR_CONTROLLER.to_string(),
                    msg: generator_controller_msg,
                    funds: vec![],
                }
                .into(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Kicking the user results in a Generator controller message
    //
    // Expect Error
    //      - An error is returned instead
    #[test]
    fn ibc_kick_unlocked() {
        let (mut deps, env, info) = mock_all(OWNER);

        let voter = "voter1234";

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

        // Kick the voter
        let ibc_kick_unlocked = to_binary(&Hub::KickUnlockedVoter {
            voter: Addr::unchecked(voter),
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_kick_unlocked);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env, msg).unwrap();

        let hub_respone: Response = from_binary(&res.acknowledgement).unwrap();
        match hub_respone {
            Response::Result { error, .. } => {
                assert!(error.is_none());
            }
            _ => panic!("Wrong response type"),
        }

        // We must have one message
        assert_eq!(res.messages.len(), 1);

        // Verify that the message matches the expected message
        let controller_msg = to_binary(
        &astroport_governance::generator_controller_lite::ExecuteMsg::KickUnlockedOutpostVoter {
            unlocked_voter:voter.to_string(),
        },
        )
        .unwrap();

        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: WasmMsg::Execute {
                    contract_addr: GENERATOR_CONTROLLER.to_string(),
                    msg: controller_msg,
                    funds: vec![],
                }
                .into(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Kicking the user results in a Generator controller message
    //
    // Expect Error
    //      - An error is returned instead
    #[test]
    fn ibc_kick_blacklisted() {
        let (mut deps, env, info) = mock_all(OWNER);

        let voter = "voter1234";

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

        // Kick the voter
        let ibc_kick_blacklisted = to_binary(&Hub::KickBlacklistedVoter {
            voter: Addr::unchecked(voter),
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_kick_blacklisted);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env, msg).unwrap();

        let hub_respone: Response = from_binary(&res.acknowledgement).unwrap();
        match hub_respone {
            Response::Result { error, .. } => {
                assert!(error.is_none());
            }
            _ => panic!("Wrong response type"),
        }

        // We must have one message
        assert_eq!(res.messages.len(), 1);

        // Verify that the message matches the expected message
        let controller_msg = to_binary(
            &astroport_governance::generator_controller_lite::ExecuteMsg::KickBlacklistedVoters {
                blacklisted_voters: vec![voter.to_string()],
            },
        )
        .unwrap();

        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: WasmMsg::Execute {
                    contract_addr: GENERATOR_CONTROLLER.to_string(),
                    msg: controller_msg,
                    funds: vec![],
                }
                .into(),
            }
        );
    }
}
