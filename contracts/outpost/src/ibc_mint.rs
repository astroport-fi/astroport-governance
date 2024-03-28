use astroport_governance::interchain::Response;
use cosmwasm_std::{to_json_binary, Deps, DepsMut, IbcReceiveResponse, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;

use crate::{error::ContractError, state::CONFIG};

/// Mint new xASTRO based on the message received from the Hub, it cannot be
/// called directly.
///
/// This is called in response to a staking message sent to the Hub
pub fn handle_ibc_xastro_mint(
    deps: DepsMut,
    recipient: String,
    amount: Uint128,
) -> Result<IbcReceiveResponse, ContractError> {
    // Mint the new amount of xASTRO to the recipient that originally initiated
    // the ASTRO staking
    let msg = mint_xastro_msg(deps.as_ref(), recipient.clone(), amount)?;

    // If the minting succeeds, the ack will be sent back to the Hub
    let ack_data = to_json_binary(&Response::new_success(
        "mint_xastro".to_owned(),
        recipient.to_string(),
    ))?;

    let response = IbcReceiveResponse::new()
        .add_message(msg)
        .set_ack(ack_data)
        .add_attribute("action", "mint_xastro")
        .add_attribute("user", recipient)
        .add_attribute("amount", amount);

    Ok(response)
}

/// Create a new message to mint xASTRO to a specific address
pub fn mint_xastro_msg(
    deps: Deps,
    recipient: String,
    amount: Uint128,
) -> Result<WasmMsg, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mint_msg = Cw20ExecuteMsg::Mint { recipient, amount };
    Ok(WasmMsg::Execute {
        contract_addr: config.xastro_token_addr.to_string(),
        msg: to_json_binary(&mint_msg)?,
        funds: vec![],
    })
}

#[cfg(test)]
mod tests {
    use astroport_governance::interchain::Outpost;
    use cosmwasm_std::{
        from_json, testing::mock_info, Addr, IbcPacketReceiveMsg, ReplyOn, SubMsg, Uint128,
    };

    use super::*;
    use crate::{
        contract::instantiate,
        execute::execute,
        ibc::ibc_packet_receive,
        mock::{mock_all, mock_ibc_packet, setup_channel, HUB, OWNER, VXASTRO_TOKEN, XASTRO_TOKEN},
    };

    // Test Cases:
    //
    // Expect Success
    //      - Mint the amount of xASTRO from the Hub to the recipient
    //
    // Expect Error
    //      - Sender is not the Hub
    #[test]
    fn ibc_mint_xastro() {
        let (mut deps, env, info) = mock_all(OWNER);

        let receiver = "user";
        let amount = Uint128::from(1000u64);

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

        let ibc_mint = to_json_binary(&Outpost::MintXAstro {
            receiver: receiver.to_string(),
            amount,
        })
        .unwrap();

        // Attempts to mint xASTRO from any other address than the Hub
        let recv_packet = mock_ibc_packet("wasm.nothub", "channel-7", ibc_mint.clone());

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env.clone(), msg).unwrap();
        let ack: Response = from_json(&res.acknowledgement).unwrap();
        match ack {
            Response::Result { error, .. } => {
                assert!(error == Some("Unauthorized".to_string()));
            }
            _ => panic!("Wrong response type"),
        }

        // Attempts to mint xASTRO from any other channel than the Hub
        let recv_packet = mock_ibc_packet(&format!("wasm.{}", HUB), "channel-7", ibc_mint.clone());
        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env.clone(), msg).unwrap();
        let ack: Response = from_json(&res.acknowledgement).unwrap();
        match ack {
            Response::Result { error, .. } => {
                assert!(error == Some("Unauthorized".to_string()));
            }
            _ => panic!("Wrong response type"),
        }

        // Mint from Hub contract and channel
        let recv_packet = mock_ibc_packet(&format!("wasm.{}", HUB), "channel-3", ibc_mint);
        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env, msg).unwrap();

        let ack: Response = from_json(&res.acknowledgement).unwrap();
        match ack {
            Response::Result { error, .. } => {
                assert!(error.is_none());
            }
            _ => panic!("Wrong response type"),
        }

        // Should have exactly one message
        assert_eq!(res.messages.len(), 1);

        // Verify that the mint message matches the expected message
        let xastro_mint_msg = to_json_binary(&cw20::Cw20ExecuteMsg::Mint {
            recipient: receiver.to_string(),
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
}
