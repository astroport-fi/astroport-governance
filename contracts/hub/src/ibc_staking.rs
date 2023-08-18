use astroport::querier::query_token_balance;
use cosmwasm_std::{
    to_binary, DepsMut, Env, IbcReceiveResponse, QuerierWrapper, Storage, SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use astroport_governance::interchain::Response;

use crate::{
    error::ContractError,
    reply::UNSTAKE_ID,
    state::{ReplyData, CONFIG, REPLY_DATA},
};

/// Handle an unstake command from an Outpost
///
/// Once the xASTRO has been unstaked, the resulting ASTRO will be sent back
/// to the user on the Outpost
pub fn handle_ibc_unstake(
    deps: DepsMut,
    env: Env,
    receive_channel: String,
    receiver: String,
    amount: Uint128,
) -> Result<IbcReceiveResponse, ContractError> {
    let msg = construct_unstake_msg(
        deps.storage,
        deps.querier,
        env,
        receive_channel,
        receiver.clone(),
        amount,
    )?;
    // Add to SubMessage to handle the reply
    let sub_msg = SubMsg::reply_on_success(msg, UNSTAKE_ID);

    // Set the acknowledgement. This is only to indicate that the unstake
    // was processed without error, not that the funds were successfully
    let ack_data = to_binary(&Response::new_success("unstake".to_owned(), receiver))?;

    Ok(IbcReceiveResponse::new()
        .set_ack(ack_data)
        .add_submessage(sub_msg))
}

/// Create the messages and state to correctly handle the unstaking of xASTRO
pub fn construct_unstake_msg(
    storage: &mut dyn Storage,
    querier: QuerierWrapper,
    env: Env,
    receiving_channel: String,
    receiver: String,
    amount: Uint128,
) -> Result<WasmMsg, ContractError> {
    let config = CONFIG.load(storage)?;

    // Unstake the received xASTRO amount
    // We need a SubMessage here to ensure that we send the correct amount
    // of ASTRO to the receiver as the ratio isn't 1:1
    let leave_msg = astroport::staking::Cw20HookMsg::Leave {};
    let send_msg = Cw20ExecuteMsg::Send {
        contract: config.staking_addr.to_string(),
        amount,
        msg: to_binary(&leave_msg)?,
    };

    // Send the xASTRO held in the contract to the Staking contract
    let msg = WasmMsg::Execute {
        contract_addr: config.xtoken_addr.to_string(),
        msg: to_binary(&send_msg)?,
        funds: vec![],
    };

    // Log the amount of ASTRO we currently hold
    let current_astro_balance = query_token_balance(
        &querier,
        config.token_addr.to_string(),
        env.contract.address,
    )?;

    // Temporarily save the data needed for the SubMessage reply
    let reply_data = ReplyData {
        receiver,
        receiving_channel,
        value: current_astro_balance,
        original_value: amount,
    };
    REPLY_DATA.save(storage, &reply_data)?;

    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use astroport::cw20_ics20::TransferMsg;
    use astroport_governance::{hub::HubBalance, interchain::Hub};
    use cosmwasm_std::{
        from_binary,
        testing::{mock_info, MOCK_CONTRACT_ADDR},
        Addr, IbcPacketReceiveMsg, Reply, ReplyOn, SubMsgResponse, SubMsgResult, Uint64,
    };
    use cw20::Cw20ReceiveMsg;

    use crate::{
        contract::instantiate,
        execute::execute,
        ibc::ibc_packet_receive,
        mock::{
            mock_all, mock_ibc_packet, setup_channel, ASSEMBLY, ASTRO_TOKEN, CW20ICS20,
            GENERATOR_CONTROLLER, OWNER, STAKING, XASTRO_TOKEN,
        },
        query::query,
        reply::{reply, STAKE_ID},
    };

    // Test Cases:
    //
    // Expect Success
    //      - Unstaked tokens must be returned to the user

    #[test]
    fn ibc_unstake() {
        let (mut deps, env, info) = mock_all(OWNER);

        let unstaker = "unstaker";
        let unstake_amount = Uint128::from(100u128);

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

        // Send a valid stake memo so we have something to unstake
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: unstake_amount,
                msg: to_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
                    channel: "channel-1".to_string(),
                    sender: unstaker.to_string(),
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
            amount: unstake_amount,
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

        let ibc_unstake = to_binary(&Hub::Unstake {
            receiver: unstaker.to_owned(),
            amount: unstake_amount,
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_unstake);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env.clone(), msg).unwrap();

        let ack: Response = from_binary(&res.acknowledgement).unwrap();
        match ack {
            Response::Result { error, .. } => {
                assert!(error.is_none());
            }
            _ => panic!("Wrong response type"),
        }

        // Should have exactly one message
        assert_eq!(res.messages.len(), 1);

        // Verify that the unstake message matches the expected message
        let unstake_msg = to_binary(&astroport::staking::Cw20HookMsg::Leave {}).unwrap();
        let send_msg = to_binary(&Cw20ExecuteMsg::Send {
            contract: STAKING.to_string(),
            amount: unstake_amount,
            msg: unstake_msg,
        })
        .unwrap();

        // We should see the unstake SubMessage
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 9001,
                gas_limit: None,
                reply_on: ReplyOn::Success,
                msg: WasmMsg::Execute {
                    contract_addr: XASTRO_TOKEN.to_string(),
                    msg: send_msg,
                    funds: vec![],
                }
                .into(),
            }
        );

        // Construct the reply from the staking contract that will be returned
        // to the contract
        let unstake_reply = Reply {
            id: UNSTAKE_ID,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                data: None,
            }),
        };

        let res = reply(deps.as_mut(), env.clone(), unstake_reply).unwrap();

        // We must have one CW20-ICS20 transfer message
        assert_eq!(res.messages.len(), 1);

        // Contruct the CW20-ICS20 ASTRO token transfer we expect to see
        let transfer_msg = to_binary(&TransferMsg {
            channel: "channel-1".to_string(),
            remote_address: unstaker.to_string(),
            timeout: Some(10),
            memo: None,
        })
        .unwrap();
        let send_msg = to_binary(&Cw20ExecuteMsg::Send {
            contract: CW20ICS20.to_string(),
            amount: unstake_amount,
            msg: transfer_msg,
        })
        .unwrap();

        // We should see the ASTRO token transfer
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: WasmMsg::Execute {
                    contract_addr: ASTRO_TOKEN.to_string(),
                    msg: send_msg,
                    funds: vec![],
                }
                .into(),
            }
        );

        // At this point the channel must have a zero balance as everything
        // has been unstaked
        let balances = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::ChannelBalanceAt {
                channel: "channel-3".to_string(),
                timestamp: Uint64::from(env.block.time.seconds()),
            },
        )
        .unwrap();

        let expected = HubBalance {
            balance: Uint128::zero(),
        };

        assert_eq!(balances, to_binary(&expected).unwrap());
    }
}
