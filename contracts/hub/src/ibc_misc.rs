use astroport::cw20_ics20::TransferMsg;
use cosmwasm_std::{to_json_binary, Addr, DepsMut, IbcReceiveResponse, WasmMsg};
use cw20::Cw20ExecuteMsg;

use astroport_governance::interchain::Response;

use crate::{
    error::ContractError,
    state::{get_transfer_channel_from_outpost_channel, CONFIG, USER_FUNDS},
};

/// Handle an IBC message to withdraw funds stuck on the Hub
///
/// In some cases where the CW20-ICS20 IBC transfer to the Outpost user fails
/// (due to timeout or otherwise), the funds will be stuck on the Hub chain. In
/// such a case the CW20-ICS20 contract will send the funds back here and this
/// function will attempt to send them back to the user.
pub fn handle_ibc_withdraw_stuck_funds(
    deps: DepsMut,
    receive_channel: String,
    user: Addr,
) -> Result<IbcReceiveResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Check if this user has any funds stuck on the Hub chain
    let balance = USER_FUNDS.load(deps.storage, &user)?;
    if balance.is_zero() {
        return Err(ContractError::NoFunds {});
    }

    // Map the channel the request was received on to the channel used in the
    // CW20-ICS20 transfer
    // We can use the request channel safely as the Outpost contract enforces the
    // address, we can't receive a request for funds for a different address from an
    // incorrect Outpost
    // Example, an Injective address can't request funds from a Neutron channel
    let outpost_channels =
        get_transfer_channel_from_outpost_channel(deps.as_ref(), &receive_channel)?;

    // User has funds, try to send it back to them
    let transfer_msg = TransferMsg {
        channel: outpost_channels.cw20_ics20,
        remote_address: user.to_string(),
        timeout: Some(config.ibc_timeout_seconds),
        memo: None,
    };

    let send_msg = Cw20ExecuteMsg::Send {
        contract: config.cw20_ics20_addr.to_string(),
        amount: balance,
        msg: to_json_binary(&transfer_msg)?,
    };

    let msg = WasmMsg::Execute {
        contract_addr: config.token_addr.to_string(),
        msg: to_json_binary(&send_msg)?,
        funds: vec![],
    };

    // This acknowledgement only indicates that the withdraw was processed without
    // error, not that the funds were successfully transferred over IBC to the user
    let ack_data = to_json_binary(&Response::new_success(
        "withdraw_funds".to_owned(),
        user.to_string(),
    ))?;

    // We're sending everything back to the user, so we can delete their balance
    // If this fails again, the balance will be re-added from the CW20-ICS20 contract
    USER_FUNDS.remove(deps.storage, &user);

    Ok(IbcReceiveResponse::new().set_ack(ack_data).add_message(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use astroport_governance::interchain::{self, Hub};
    use cosmwasm_std::{
        from_json, testing::mock_info, IbcPacketReceiveMsg, ReplyOn, SubMsg, Uint128,
    };
    use cw20::Cw20ReceiveMsg;

    use crate::{
        contract::instantiate,
        execute::execute,
        ibc::ibc_packet_receive,
        mock::{
            mock_all, mock_ibc_packet, setup_channel, ASSEMBLY, ASTRO_TOKEN, CW20ICS20,
            GENERATOR_CONTROLLER, OWNER, STAKING,
        },
    };

    // Test Cases:
    //
    // Expect Success
    //      - Withdrawing stuck funds results in IBC message
    //
    // Expect Error
    //      - When address has no funds stuck
    //
    // This tests that balances are correctly tracked by the contract in case of
    // IBC failures that result in funds getting stuck on the Hub
    #[test]
    fn ibc_withdraw_stuck_funds() {
        let (mut deps, env, info) = mock_all(OWNER);

        let stuck_amount = Uint128::from(100u128);
        let user = "user1";

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

        // Add a valid failure
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: stuck_amount,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::TransferFailure {
                    receiver: user.to_owned(),
                })
                .unwrap(),
            }),
        )
        .unwrap();

        // Withdraw must fail if the user has no funds stuck
        let ibc_withdraw = to_json_binary(&Hub::WithdrawFunds {
            user: Addr::unchecked("not_user"),
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_withdraw);
        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env.clone(), msg).unwrap();

        let hub_respone: interchain::Response = from_json(&res.acknowledgement).unwrap();
        match hub_respone {
            interchain::Response::Result { error, .. } => {
                assert!(error.is_some());
                assert_eq!(
                    error.unwrap(),
                    "cosmwasm_std::math::uint128::Uint128 not found"
                );
            }
            _ => panic!("Wrong response type"),
        }

        // Our user has funds stuck, so withdrawal must succeed
        let ibc_withdraw = to_json_binary(&Hub::WithdrawFunds {
            user: Addr::unchecked(user),
        })
        .unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_withdraw);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env, msg).unwrap();

        let hub_respone: interchain::Response = from_json(&res.acknowledgement).unwrap();
        match hub_respone {
            interchain::Response::Result { address, error, .. } => {
                assert!(error.is_none());
                assert_eq!(address.unwrap(), user);
            }
            _ => panic!("Wrong response type"),
        }

        // We must see one message being emitted from the withdraw
        assert_eq!(res.messages.len(), 1);

        // It must be a CW20-ICS20 transfer message
        let ibc_transfer_msg = to_json_binary(&TransferMsg {
            remote_address: user.to_string(),
            channel: "channel-1".to_string(),
            timeout: Some(10),
            memo: None,
        })
        .unwrap();
        let cw_send_msg = to_json_binary(&Cw20ExecuteMsg::Send {
            contract: CW20ICS20.to_string(),
            amount: stuck_amount,
            msg: ibc_transfer_msg,
        })
        .unwrap();

        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: WasmMsg::Execute {
                    contract_addr: "astro".to_string(),
                    msg: cw_send_msg,
                    funds: vec![],
                }
                .into(),
            }
        );
    }
}
