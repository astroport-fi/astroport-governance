use cosmwasm_std::{to_json_binary, DepsMut, IbcBasicResponse, WasmMsg};

use astroport_governance::{interchain::Hub, voting_escrow_lite};

use crate::{
    error::ContractError,
    ibc_mint::mint_xastro_msg,
    state::{CONFIG, PENDING_VOTES, VOTES},
};

pub fn handle_failed_messages(
    deps: DepsMut,
    failed_msg: Hub,
    mut response: IbcBasicResponse,
) -> Result<IbcBasicResponse, ContractError> {
    match failed_msg.clone() {
        Hub::CastAssemblyVote {
            proposal_id, voter, ..
        } => {
            // Vote failed, remove vote from the log so user may retry
            VOTES.remove(deps.storage, (&voter, proposal_id));

            response = response
                .add_attribute("interchain_action", failed_msg.to_string())
                .add_attribute("user", voter.to_string());
        }
        Hub::CastEmissionsVote { voter, .. } => {
            response = response
                .add_attribute("interchain_action", failed_msg.to_string())
                .add_attribute("user", voter.to_string());
        }
        Hub::QueryProposal { id } => {
            // If the proposal query failed we need to remove the pending vote
            // otherwise no other vote will be possible for this proposal
            let pending_vote = PENDING_VOTES.load(deps.storage, id)?;
            PENDING_VOTES.remove(deps.storage, id);

            response = response
                .add_attribute("interchain_action", failed_msg.to_string())
                .add_attribute("user", pending_vote.voter.to_string());
        }

        Hub::Unstake { receiver, amount } => {
            // Unstaking involves us burning the received xASTRO before
            // sending the unstake message to the Hub. If the unstaking
            // fails we need to mint the xASTRO back to the user
            let msg = mint_xastro_msg(deps.as_ref(), receiver.clone(), amount)?;
            response = response
                .add_message(msg)
                .add_attribute("interchain_action", failed_msg.to_string())
                .add_attribute("user", receiver);
        }
        Hub::KickUnlockedVoter { voter } => {
            // The voting power has not been removed for this user and we must
            // relock their unlocking position
            let config = CONFIG.load(deps.storage)?;

            let relock_msg = voting_escrow_lite::ExecuteMsg::Relock {
                user: voter.to_string(),
            };

            let msg = WasmMsg::Execute {
                contract_addr: config.vxastro_token_addr.to_string(),
                msg: to_json_binary(&relock_msg)?,
                funds: vec![],
            };

            response = response
                .add_message(msg)
                .add_attribute("interchain_action", failed_msg.to_string())
                .add_attribute("user", voter);
        }
        Hub::WithdrawFunds { user } => {
            response = response
                .add_attribute("interchain_action", failed_msg.to_string())
                .add_attribute("user", user.to_string());
        }
        // Not all Hub responses will be received here, we only handle the ones we have
        // control over
        _ => {
            response = response.add_attribute("action", failed_msg.to_string());
        }
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        attr,
        testing::{mock_info, MOCK_CONTRACT_ADDR},
        to_json_binary, Addr, IbcEndpoint, IbcPacket, IbcPacketTimeoutMsg, ReplyOn, StdError,
        SubMsg, Uint128, WasmMsg,
    };

    use super::*;
    use crate::{
        contract::instantiate,
        execute::execute,
        ibc::ibc_packet_timeout,
        mock::{mock_all, setup_channel, HUB, OWNER, VXASTRO_TOKEN, XASTRO_TOKEN},
        state::PendingVote,
    };

    // Test Cases:
    //
    // Expect Success
    //      - xASTRO is returned to the original sender
    //
    // Expect Error
    //      - Receive timeout from a different channel
    #[test]
    fn unstake_failure() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user = "user";
        let amount = Uint128::from(1000u64);
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

        // Attempt to get timeout from different contract
        let original_unstake_msg = to_json_binary(&Hub::Unstake {
            receiver: user.to_string(),
            amount,
        })
        .unwrap();

        let packet = IbcPacket::new(
            original_unstake_msg,
            IbcEndpoint {
                port_id: format!("wasm.{}", MOCK_CONTRACT_ADDR),
                channel_id: "channel-3".to_string(),
            },
            IbcEndpoint {
                port_id: format!("wasm.{}", HUB),
                channel_id: "channel-7".to_string(),
            },
            4,
            env.block.time.plus_seconds(ibc_timeout_seconds).into(),
        );

        // When the timeout occurs, we should see an unstake message to return the ASTRO to the user
        let timeout_packet = IbcPacketTimeoutMsg::new(packet, Addr::unchecked("relayer"));
        let res = ibc_packet_timeout(deps.as_mut(), env, timeout_packet).unwrap();

        // Should have exactly one message
        assert_eq!(res.messages.len(), 1);

        // Verify that the mint message matches the expected message
        let xastro_mint_msg = to_json_binary(&cw20::Cw20ExecuteMsg::Mint {
            recipient: user.to_string(),
            amount,
        })
        .unwrap();

        // We should see the mint xASTRO SubMessage
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: WasmMsg::Execute {
                    contract_addr: XASTRO_TOKEN.to_string(),
                    msg: xastro_mint_msg,
                    funds: vec![],
                }
                .into(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Vote fails to reach the Hub
    #[test]
    fn governance_vote_failure() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user = "user";
        let proposal_id = 1u64;
        let voting_power = Uint128::from(1000u64);
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

        // Construct the original message
        let original_msg = to_json_binary(&Hub::CastAssemblyVote {
            proposal_id,
            voter: Addr::unchecked(user),
            vote_option: astroport_governance::assembly::ProposalVoteOption::For,
            voting_power,
        })
        .unwrap();
        // Authorised channels
        let packet = IbcPacket::new(
            original_msg,
            IbcEndpoint {
                port_id: format!("wasm.{}", MOCK_CONTRACT_ADDR),
                channel_id: "channel-3".to_string(),
            },
            IbcEndpoint {
                port_id: format!("wasm.{}", HUB),
                channel_id: "channel-7".to_string(),
            },
            4,
            env.block.time.plus_seconds(ibc_timeout_seconds).into(),
        );

        // When the timeout occurs, we should see the correct attributes emitted
        let timeout_packet = IbcPacketTimeoutMsg::new(packet, Addr::unchecked("relayer"));
        let res = ibc_packet_timeout(deps.as_mut(), env, timeout_packet).unwrap();

        // Should have no messages
        assert_eq!(res.messages.len(), 0);

        // Should have the correct attributes
        assert_eq!(
            res.attributes,
            vec![
                attr("action".to_string(), "ibc_packet_timeout".to_string()),
                attr(
                    "interchain_action".to_string(),
                    "cast_assembly_vote".to_string()
                ),
                attr("user".to_string(), user.to_string()),
            ]
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Emissions Vote fails to reach the Hub
    #[test]
    fn emissions_vote_failure() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user = "user";
        let votes = vec![("pool".to_string(), 10000u16)];
        let voting_power = Uint128::from(1000u64);
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

        // Construct the original message
        let original_msg = to_json_binary(&Hub::CastEmissionsVote {
            voter: Addr::unchecked(user),
            voting_power,
            votes,
        })
        .unwrap();
        // Authorised channels
        let packet = IbcPacket::new(
            original_msg,
            IbcEndpoint {
                port_id: format!("wasm.{}", MOCK_CONTRACT_ADDR),
                channel_id: "channel-3".to_string(),
            },
            IbcEndpoint {
                port_id: format!("wasm.{}", HUB),
                channel_id: "channel-7".to_string(),
            },
            4,
            env.block.time.plus_seconds(ibc_timeout_seconds).into(),
        );

        // When the timeout occurs, we should see the correct attributes emitted
        let timeout_packet = IbcPacketTimeoutMsg::new(packet, Addr::unchecked("relayer"));
        let res = ibc_packet_timeout(deps.as_mut(), env, timeout_packet).unwrap();

        // Should have no messages
        assert_eq!(res.messages.len(), 0);

        // Should have the correct attributes
        assert_eq!(
            res.attributes,
            vec![
                attr("action".to_string(), "ibc_packet_timeout".to_string()),
                attr(
                    "interchain_action".to_string(),
                    "cast_emissions_vote".to_string()
                ),
                attr("user".to_string(), user.to_string()),
            ]
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Proposal query fails
    #[test]
    fn query_proposal_failure() {
        let (mut deps, env, info) = mock_all(OWNER);

        let proposal_id = 1u64;
        let user = "user";
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

        // Construct the original message
        let original_msg = to_json_binary(&Hub::QueryProposal { id: proposal_id }).unwrap();
        // Authorised channels
        let packet = IbcPacket::new(
            original_msg,
            IbcEndpoint {
                port_id: format!("wasm.{}", MOCK_CONTRACT_ADDR),
                channel_id: "channel-3".to_string(),
            },
            IbcEndpoint {
                port_id: format!("wasm.{}", HUB),
                channel_id: "channel-7".to_string(),
            },
            4,
            env.block.time.plus_seconds(ibc_timeout_seconds).into(),
        );

        // Ensure we have a pending vote
        PENDING_VOTES
            .save(
                &mut deps.storage,
                proposal_id,
                &PendingVote {
                    proposal_id,
                    vote_option: astroport_governance::assembly::ProposalVoteOption::For,
                    voter: Addr::unchecked(user),
                },
            )
            .unwrap();

        // When the timeout occurs, we should see the correct attributes emitted
        let timeout_packet = IbcPacketTimeoutMsg::new(packet, Addr::unchecked("relayer"));
        let res = ibc_packet_timeout(deps.as_mut(), env, timeout_packet).unwrap();

        // Should have no messages
        assert_eq!(res.messages.len(), 0);

        // Should have the correct attributes
        assert_eq!(
            res.attributes,
            vec![
                attr("action".to_string(), "ibc_packet_timeout".to_string()),
                attr(
                    "interchain_action".to_string(),
                    "query_proposal".to_string()
                ),
                attr("user".to_string(), user.to_string()),
            ]
        );

        // Also ensure pending votes for this proposal was removed
        let err = PENDING_VOTES.load(&deps.storage, proposal_id).unwrap_err();

        assert_eq!(
            err,
            StdError::NotFound {
                kind: "astroport_outpost::state::PendingVote".to_string()
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Kicking unlocked fails to reach the Hub
    #[test]
    fn kick_unlocked() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user = "user";
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

        // Construct the original message
        let original_msg = to_json_binary(&Hub::KickUnlockedVoter {
            voter: Addr::unchecked(user),
        })
        .unwrap();
        // Authorised channels
        let packet = IbcPacket::new(
            original_msg,
            IbcEndpoint {
                port_id: format!("wasm.{}", MOCK_CONTRACT_ADDR),
                channel_id: "channel-3".to_string(),
            },
            IbcEndpoint {
                port_id: format!("wasm.{}", HUB),
                channel_id: "channel-7".to_string(),
            },
            4,
            env.block.time.plus_seconds(ibc_timeout_seconds).into(),
        );

        // When the timeout occurs, we should see the correct attributes emitted
        let timeout_packet = IbcPacketTimeoutMsg::new(packet, Addr::unchecked("relayer"));
        let res = ibc_packet_timeout(deps.as_mut(), env, timeout_packet).unwrap();

        // Should have 1 relock message
        assert_eq!(res.messages.len(), 1);

        // Should have the correct attributes
        assert_eq!(
            res.attributes,
            vec![
                attr("action".to_string(), "ibc_packet_timeout".to_string()),
                attr(
                    "interchain_action".to_string(),
                    "kick_unlocked_voter".to_string()
                ),
                attr("user".to_string(), user.to_string()),
            ]
        );

        // Confirm relock message is correct
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: WasmMsg::Execute {
                    contract_addr: VXASTRO_TOKEN.to_string(),
                    msg: to_json_binary(
                        &astroport_governance::voting_escrow_lite::ExecuteMsg::Relock {
                            user: user.to_string()
                        }
                    )
                    .unwrap(),
                    funds: vec![],
                }
                .into(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - Kicking unlocked fails to reach the Hub
    #[test]
    fn withdraw_funds() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user = "user";
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

        // Construct the original message
        let original_msg = to_json_binary(&Hub::WithdrawFunds {
            user: Addr::unchecked(user),
        })
        .unwrap();
        // Authorised channels
        let packet = IbcPacket::new(
            original_msg,
            IbcEndpoint {
                port_id: format!("wasm.{}", MOCK_CONTRACT_ADDR),
                channel_id: "channel-3".to_string(),
            },
            IbcEndpoint {
                port_id: format!("wasm.{}", HUB),
                channel_id: "channel-7".to_string(),
            },
            4,
            env.block.time.plus_seconds(ibc_timeout_seconds).into(),
        );

        // When the timeout occurs, we should see the correct attributes emitted
        let timeout_packet = IbcPacketTimeoutMsg::new(packet, Addr::unchecked("relayer"));
        let res = ibc_packet_timeout(deps.as_mut(), env, timeout_packet).unwrap();

        // Should have no messages
        assert_eq!(res.messages.len(), 0);

        // Should have the correct attributes
        assert_eq!(
            res.attributes,
            vec![
                attr("action".to_string(), "ibc_packet_timeout".to_string()),
                attr(
                    "interchain_action".to_string(),
                    "withdraw_funds".to_string()
                ),
                attr("user".to_string(), user.to_string()),
            ]
        );
    }
}
