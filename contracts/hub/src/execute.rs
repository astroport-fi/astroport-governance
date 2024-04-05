use cosmwasm_std::{
    entry_point, from_json, to_json_binary, Addr, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, SubMsg, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use astroport::{
    common::{claim_ownership, drop_ownership_proposal, propose_new_owner},
    querier::query_token_balance,
};
use astroport_governance::{
    hub::{Config, Cw20HookMsg, ExecuteMsg},
    interchain::{Hub, MAX_IBC_TIMEOUT_SECONDS, MIN_IBC_TIMEOUT_SECONDS},
    utils::check_contract_supports_channel,
};

use crate::{
    error::ContractError,
    reply::STAKE_ID,
    state::{
        OutpostChannels, ReplyData, CONFIG, OUTPOSTS, OWNERSHIP_PROPOSAL, REPLY_DATA, USER_FUNDS,
    },
};

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::UpdateConfig { ibc_timeout_seconds }** Update parameters in the Hub contract. Only the owner is allowed to
/// update the config
///
/// * **ExecuteMsg::AddOutpost { outpost_addr, cw20_ics20_channel }** Add an Outpost to the contract,
/// allowing new IBC connections and IBC messages
///
/// * **ExecuteMsg::RemoveOutpost { outpost_addr }** Removes an Outpost from the contract,
/// blocking new IBC connections as well as any IBC messages
///
/// * **ExecuteMsg::ProposeNewOwner { new_owner, expires_in }** Creates a new request to change
/// contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::UpdateConfig {
            ibc_timeout_seconds,
        } => update_config(deps, info, ibc_timeout_seconds),
        ExecuteMsg::AddOutpost {
            outpost_addr,
            outpost_channel,
            cw20_ics20_channel,
        } => add_outpost(
            deps,
            env,
            info,
            outpost_addr,
            outpost_channel,
            cw20_ics20_channel,
        ),
        ExecuteMsg::RemoveOutpost { outpost_addr } => remove_outpost(deps, info, outpost_addr),
        ExecuteMsg::ProposeNewOwner {
            new_owner,
            expires_in,
        } => {
            let config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                new_owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.owner = new_owner;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on
/// the received template
///
/// Funds received here must be from the CW20-ICS20 contract and is used for
/// actions initiated from an Outpost that require ASTRO tokens
///
/// * **cw20_msg** CW20 message to process
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // We only allow ASTRO tokens to be sent here
    if info.sender != config.token_addr {
        return Err(ContractError::Unauthorized {});
    }

    // The sender of the ASTRO tokens must be the CW20-ICS20 contract
    if cw20_msg.sender != config.cw20_ics20_addr {
        return Err(ContractError::Unauthorized {});
    }

    // We can't do anything with no tokens
    if cw20_msg.amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    // Match the CW20 template
    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::OutpostMemo {
            channel,
            sender,
            receiver,
            memo,
        } => handle_outpost_memo(deps, env, cw20_msg, channel, sender, receiver, memo),
        Cw20HookMsg::TransferFailure { receiver } => {
            handle_transfer_failure(deps, info, cw20_msg, receiver)
        }
    }
}

/// Handle the JSON memo from an Outpost by matching against the available
/// actions.
///
/// If the memo is not in a valid format for the actions it is
/// considered invalid.
///
/// If the memo wasn't intended for us we forward it to the original
/// intended receiver
fn handle_outpost_memo(
    deps: DepsMut,
    env: Env,
    msg: Cw20ReceiveMsg,
    receiving_channel: String,
    original_sender: String,
    original_receiver: String,
    memo: String,
) -> Result<Response, ContractError> {
    // If the receiver is not our contract we assume this is incorrect and fail
    // the transfer, causing the funds to be returned to the sender on the
    // original chain
    if env.contract.address != original_receiver {
        return Err(ContractError::InvalidDestination {});
    }

    // But if this was intended for us, parse and handle the memo
    let sub_msg: SubMsg = match serde_json_wasm::from_str::<Hub>(memo.as_str()) {
        Ok(hub) => match hub {
            Hub::Stake {} => handle_stake_instruction(
                deps,
                env,
                msg,
                receiving_channel,
                original_sender.clone(),
            )?,
            _ => {
                return Err(ContractError::NotMemoAction {
                    action: hub.to_string(),
                })
            }
        },
        Err(reason) => {
            // This memo doesn't match any of our action formats
            // In case the receiver is set to our handler contract we
            // assume the funds were intended to have a valid action but
            // are invalid, thus we need to fail the transaction and return
            // the funds
            return Err(ContractError::InvalidMemo { reason });
        }
    };

    Ok(Response::default()
        .add_submessage(sub_msg)
        .add_attribute("hub", "handle_memo")
        .add_attribute("memo_type", "instruction")
        .add_attribute("sender", original_sender))
}

/// Handle a stake instruction sent via memo from an Outpost
///
/// The full amount is staked and the resulting xASTRO is sent to the
/// original sender on the Outpost
fn handle_stake_instruction(
    deps: DepsMut,
    env: Env,
    msg: Cw20ReceiveMsg,
    receiving_channel: String,
    original_sender: String,
) -> Result<SubMsg, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Stake all the received ASTRO tokens
    // We need a SubMessage here to ensure we only mint the actual
    // amount of ASTRO that was staked, which *might* not the full amount sent
    let enter_msg = astroport::staking::Cw20HookMsg::Enter {};
    let send_msg = Cw20ExecuteMsg::Send {
        contract: config.staking_addr.to_string(),
        amount: msg.amount,
        msg: to_json_binary(&enter_msg)?,
    };

    // Execute the message, we're using a CW20, so no funds added here
    let stake_msg = WasmMsg::Execute {
        contract_addr: config.token_addr.to_string(),
        msg: to_json_binary(&send_msg)?,
        funds: vec![],
    };

    let current_xastro_balance = query_token_balance(
        &deps.querier,
        config.xtoken_addr.to_string(),
        env.contract.address,
    )?;

    // Temporarily save the data needed for the SubMessage reply
    let reply_data = ReplyData {
        receiver: original_sender,
        receiving_channel,
        value: current_xastro_balance,
        original_value: msg.amount,
    };
    REPLY_DATA.save(deps.storage, &reply_data)?;

    Ok(SubMsg::reply_on_success(stake_msg, STAKE_ID))
}

/// Update the Hub config
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    ibc_timeout_seconds: Option<u64>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Only owner can update the config
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(ibc_timeout_seconds) = ibc_timeout_seconds {
        if !(MIN_IBC_TIMEOUT_SECONDS..=MAX_IBC_TIMEOUT_SECONDS).contains(&ibc_timeout_seconds) {
            return Err(ContractError::InvalidIBCTimeout {
                timeout: ibc_timeout_seconds,
                min: MIN_IBC_TIMEOUT_SECONDS,
                max: MAX_IBC_TIMEOUT_SECONDS,
            });
        }
        config.ibc_timeout_seconds = ibc_timeout_seconds;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// Add an Outpost to the Hub
///
/// Adding an Outpost requires the Outpost address and the CW20-ICS20 channel
/// where funds will be sent through. Adding an Outpost will allow a new IBC
/// channel to be established with the Outpost and the Hub
fn add_outpost(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    outpost_addr: String,
    outpost_channel: String,
    cw20_ics20_channel: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Only owner can add Outposts
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if OUTPOSTS.has(deps.storage, &outpost_addr) {
        return Err(ContractError::OutpostAlreadyAdded {
            address: outpost_addr,
        });
    }

    // Check if the channel is supported in the CW20-ICS20 contract
    check_contract_supports_channel(deps.querier, &config.cw20_ics20_addr, &cw20_ics20_channel)?;
    // Check that the Hub supports the Outpost channel
    check_contract_supports_channel(deps.querier, &env.contract.address, &outpost_channel)?;

    let outpost = OutpostChannels {
        outpost: outpost_channel,
        cw20_ics20: cw20_ics20_channel.clone(),
    };

    // Store the CW20-ICS20 transfer channel for the Outpost
    OUTPOSTS.save(deps.storage, &outpost_addr, &outpost)?;

    Ok(Response::default()
        .add_attribute("action", "add_outpost")
        .add_attribute("address", outpost_addr)
        .add_attribute("cw20_ics20_channel", cw20_ics20_channel))
}

/// Remove an Outpost from the Hub
///
/// Removing an Outpost will block new IBC channels to be established between the
/// Hub and the provided Outpost. All IBC messages will also fail
///
/// IMPORTANT: This does not close any existing IBC channels
fn remove_outpost(
    deps: DepsMut,
    info: MessageInfo,
    outpost_addr: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Only owner can remove Outposts
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    OUTPOSTS.remove(deps.storage, &outpost_addr);

    Ok(Response::default()
        .add_attribute("action", "remove_outpost")
        .add_attribute("address", outpost_addr))
}

/// Handle failed CW20-ICS20 IBC transfers
///
/// If a CW20-ICS20 IBC transfer fails that we initiated, we receive the original
/// tokens back and need to store them for the user to retrieve manually
///
/// Once funds are held here, the original user will need to issue a withdraw
/// transaction on the Outpost to retrieve their funds.
fn handle_transfer_failure(
    deps: DepsMut,
    info: MessageInfo,
    msg: Cw20ReceiveMsg,
    receiver: String,
) -> Result<Response, ContractError> {
    let user_addr = Addr::unchecked(&receiver);
    USER_FUNDS.update(deps.storage, &user_addr, |balance| -> StdResult<_> {
        Ok(balance.unwrap_or_default().checked_add(msg.amount)?)
    })?;

    Ok(Response::default()
        .add_attribute("outpost_handler", "handle_transfer_failure")
        .add_attribute("sender", info.sender)
        .add_attribute("og_receiver", receiver))
}

#[cfg(test)]
mod tests {
    use super::*;
    use astroport_governance::{hub::HubBalance, interchain::Outpost};
    use cosmwasm_std::{
        testing::{mock_info, MOCK_CONTRACT_ADDR},
        IbcEndpoint, IbcMsg, IbcPacket, IbcPacketTimeoutMsg, Reply, ReplyOn, SubMsgResponse,
        SubMsgResult, Uint128, Uint64,
    };
    use serde_json_wasm::de::Error as SerdeError;

    use crate::{
        contract::instantiate,
        execute::execute,
        ibc::ibc_packet_timeout,
        mock::{
            mock_all, setup_channel, ASSEMBLY, ASTRO_TOKEN, CW20ICS20, GENERATOR_CONTROLLER, OWNER,
            STAKING, XASTRO_TOKEN,
        },
        query::query,
        reply::{reply, UNSTAKE_ID},
    };

    // Test Cases:
    //
    // Expect Success
    //      - Adding and removing Outposts work correctly
    //
    // Expect Error
    //      - Adding an Outpost with duplicate address
    //      - Adding an Outpost when not the owner
    //      - Removing an Outpost when not the owner
    //
    #[test]
    fn add_remove_outpost() {
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

        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "wasm1contractaddress1".to_string(),
                outpost_channel: "channel-2".to_string(),
                cw20_ics20_channel: "channel-1".to_string(),
            },
        )
        .unwrap();

        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "wasm1contractaddress2".to_string(),
                outpost_channel: "channel-2".to_string(),
                cw20_ics20_channel: "channel-2".to_string(),
            },
        )
        .unwrap();

        // Test paging, should return a single result
        let outposts = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::Outposts {
                start_after: None,
                limit: Some(1),
            },
        )
        .unwrap();

        assert_eq!(
            outposts,
            to_json_binary(&vec![astroport_governance::hub::OutpostConfig {
                address: "wasm1contractaddress1".to_string(),
                channel: "channel-2".to_string(),
                cw20_ics20_channel: "channel-1".to_string(),
            },])
            .unwrap()
        );

        // Test paging, should return a single result of the second item
        let outposts = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::Outposts {
                start_after: Some("wasm1contractaddress1".to_string()),
                limit: Some(1),
            },
        )
        .unwrap();

        assert_eq!(
            outposts,
            to_json_binary(&vec![astroport_governance::hub::OutpostConfig {
                address: "wasm1contractaddress2".to_string(),
                channel: "channel-2".to_string(),
                cw20_ics20_channel: "channel-2".to_string(),
            },])
            .unwrap()
        );

        // Get all
        let outposts = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::Outposts {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

        assert_eq!(
            outposts,
            to_json_binary(&vec![
                astroport_governance::hub::OutpostConfig {
                    address: "wasm1contractaddress1".to_string(),
                    channel: "channel-2".to_string(),
                    cw20_ics20_channel: "channel-1".to_string(),
                },
                astroport_governance::hub::OutpostConfig {
                    address: "wasm1contractaddress2".to_string(),
                    channel: "channel-2".to_string(),
                    cw20_ics20_channel: "channel-2".to_string(),
                }
            ])
            .unwrap()
        );

        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::RemoveOutpost {
                outpost_addr: "wasm1contractaddress1".to_string(),
            },
        )
        .unwrap();

        let outposts = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::Outposts {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

        assert_eq!(
            outposts,
            to_json_binary(&vec![astroport_governance::hub::OutpostConfig {
                address: "wasm1contractaddress2".to_string(),
                channel: "channel-2".to_string(),
                cw20_ics20_channel: "channel-2".to_string(),
            },])
            .unwrap()
        );

        // Must not allow duplicate Outpost addresses
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "wasm1contractaddress2".to_string(),
                outpost_channel: "channel-2".to_string(),
                cw20_ics20_channel: "channel-2".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::OutpostAlreadyAdded { address: _ }
        ));

        // Must not allow adding if not the owner
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not_owner", &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "wasm1contractaddress3".to_string(),
                outpost_channel: "channel-2".to_string(),
                cw20_ics20_channel: "channel-4".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        // Must not allow removing if not the owner
        let err = execute(
            deps.as_mut(),
            env,
            mock_info("not_owner", &[]),
            astroport_governance::hub::ExecuteMsg::RemoveOutpost {
                outpost_addr: "wasm1contractaddress2".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    // Test Cases:
    //
    // Expect Success
    //      - Updating config works
    //
    // Expect Error
    //      - Updating config with invalid addresses
    //      - Updating config when not the owner
    //
    #[test]
    fn update_config() {
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

        let config = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::Config {},
        )
        .unwrap();

        // Ensure the config set during instantiation is correct
        assert_eq!(
            config,
            to_json_binary(&astroport_governance::hub::Config {
                owner: Addr::unchecked(OWNER),
                assembly_addr: Addr::unchecked(ASSEMBLY),
                cw20_ics20_addr: Addr::unchecked(CW20ICS20),
                staking_addr: Addr::unchecked(STAKING),
                token_addr: Addr::unchecked(ASTRO_TOKEN),
                xtoken_addr: Addr::unchecked(XASTRO_TOKEN),
                generator_controller_addr: Addr::unchecked(GENERATOR_CONTROLLER),
                ibc_timeout_seconds: 10,
            })
            .unwrap()
        );

        // Update the IBC timeout to a value below min
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::UpdateConfig {
                ibc_timeout_seconds: Some(MIN_IBC_TIMEOUT_SECONDS - 1),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::InvalidIBCTimeout {
                timeout: _,
                min: MIN_IBC_TIMEOUT_SECONDS,
                max: MAX_IBC_TIMEOUT_SECONDS
            }
        ));

        // Update the IBC timeout to a value below max
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::UpdateConfig {
                ibc_timeout_seconds: Some(MAX_IBC_TIMEOUT_SECONDS + 1),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::InvalidIBCTimeout {
                timeout: _,
                min: MIN_IBC_TIMEOUT_SECONDS,
                max: MAX_IBC_TIMEOUT_SECONDS
            }
        ));

        // Update the IBC timeout to a correct value
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::UpdateConfig {
                ibc_timeout_seconds: Some(50),
            },
        )
        .unwrap();
        // Query the new config
        let config = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::Config {},
        )
        .unwrap();

        assert_eq!(
            config,
            to_json_binary(&astroport_governance::hub::Config {
                owner: Addr::unchecked(OWNER),
                assembly_addr: Addr::unchecked(ASSEMBLY),
                cw20_ics20_addr: Addr::unchecked(CW20ICS20),
                staking_addr: Addr::unchecked(STAKING),
                token_addr: Addr::unchecked(ASTRO_TOKEN),
                xtoken_addr: Addr::unchecked(XASTRO_TOKEN),
                generator_controller_addr: Addr::unchecked(GENERATOR_CONTROLLER),
                ibc_timeout_seconds: 50,
            })
            .unwrap()
        );

        // Must not allow updating if not the owner
        let err = execute(
            deps.as_mut(),
            env,
            mock_info("not_owner", &[]),
            astroport_governance::hub::ExecuteMsg::UpdateConfig {
                ibc_timeout_seconds: Some(200),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    // Test Cases:
    //
    // Expect Success
    //      - Sending the funds results in correct balances
    //
    // Expect Error
    //      - When not sent by the CW20-ICS20 contract
    //      - When tokens are not ASTRO
    //      - When amount is zero
    //
    // This tests that balances are correctly tracked by the contract in case of
    // IBC failures that result in funds getting stuck on the Hub
    #[test]
    fn cw20_ics20_transfer_failure() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user1 = "user1";
        let user2 = "user2";
        let user1_funds = Uint128::from(100u128);
        let user2_funds = Uint128::from(300u128);

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

        // Add an allowed Outpost
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "outpost".to_string(),
                outpost_channel: "channel-2".to_string(),
                cw20_ics20_channel: "channel-1".to_string(),
            },
        )
        .unwrap();

        // Transfer failures are only allowed to be recorded when sent by the
        // CW20-ICS20 contract and if the tokens are ASTRO
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: "not_cw20_ics20".to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::TransferFailure {
                    receiver: user1.to_owned(),
                })
                .unwrap(),
            }),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::Unauthorized {}));

        // Transfer failures must only accept ASTRO tokens
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(CW20ICS20, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::TransferFailure {
                    receiver: user1.to_owned(),
                })
                .unwrap(),
            }),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::Unauthorized {}));

        // Transfer failures will must not accept zero amounts
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: Uint128::zero(),
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::TransferFailure {
                    receiver: user1.to_owned(),
                })
                .unwrap(),
            }),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::ZeroAmount {}));

        // Add a valid failure for user
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::TransferFailure {
                    receiver: user1.to_owned(),
                })
                .unwrap(),
            }),
        )
        .unwrap();

        // Verify that the amount was added to the user's balance
        let balance = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::UserFunds {
                user: Addr::unchecked(user1),
            },
        )
        .unwrap();

        assert_eq!(
            balance,
            to_json_binary(&HubBalance {
                balance: user1_funds
            })
            .unwrap()
        );

        execute(
            deps.as_mut(),
            env,
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user2_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::TransferFailure {
                    receiver: user2.to_owned(),
                })
                .unwrap(),
            }),
        )
        .unwrap();

        // Verify that the amount was added to the user's balance
        let stuck_funds = USER_FUNDS
            .load(&deps.storage, &Addr::unchecked(user2))
            .unwrap();

        assert_eq!(stuck_funds, user2_funds);
    }

    // Test Cases:
    //
    // Expect Success
    //      - Memo is sent from authorised CW20-ICS20 contract
    //
    // Expect Error
    //      - Memo's sent from anywhere other than the CW20-ICS20 contract
    //      - Memo's sent with tokens other than ASTRO
    //      - Memo's sent with no funds
    #[test]
    fn receive_memo_auth_checks() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user1 = "user1";
        let user1_funds = Uint128::from(100u128);

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

        // Set up a valid IBC connection
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

        // Memo's can only be handled when sent by the CW20-ICS20 contract
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
                    channel: "channel-1".to_string(),
                    sender: user1.to_string(),
                    receiver: MOCK_CONTRACT_ADDR.to_string(),
                    memo: "{\"stake\":{}}".to_string(),
                })
                .unwrap(),
            }),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::Unauthorized {}));

        // Memo's must only accept ASTRO tokens
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(CW20ICS20, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
                    channel: "channel-1".to_string(),
                    sender: user1.to_string(),
                    receiver: MOCK_CONTRACT_ADDR.to_string(),
                    memo: "{\"stake\":{}}".to_string(),
                })
                .unwrap(),
            }),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::Unauthorized {}));

        // Memo will must not accept zero amounts
        let err = execute(
            deps.as_mut(),
            env,
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: Uint128::zero(),
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
                    channel: "channel-1".to_string(),
                    sender: user1.to_string(),
                    receiver: MOCK_CONTRACT_ADDR.to_string(),
                    memo: "{\"stake\":{}}".to_string(),
                })
                .unwrap(),
            }),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::ZeroAmount {}));
    }

    // Test Cases:
    //
    // Expect Success
    //      - Invalid memo is received and handled
    //
    // Expect Error
    //      - Calling this from an unauthorised contract must fail
    #[test]
    fn receive_invalid_memo() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user1 = "user1";
        let user1_funds = Uint128::from(100u128);

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

        // Set up a valid IBC connection
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

        // Send an invalid memo / broken JSON sent to us must fail
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
                    channel: "channel-1".to_string(),
                    sender: user1.to_string(),
                    receiver: MOCK_CONTRACT_ADDR.to_string(),
                    memo: "{\"stak}}".to_string(),
                })
                .unwrap(),
            }),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            ContractError::InvalidMemo {
                reason: SerdeError::EofWhileParsingString
            }
        ));

        // Send an unknown memo action
        let err = execute(
            deps.as_mut(),
            env,
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
                    channel: "channel-1".to_string(),
                    sender: user1.to_string(),
                    receiver: MOCK_CONTRACT_ADDR.to_string(),
                    memo: "{\"staking\":{}}".to_string(),
                })
                .unwrap(),
            }),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            ContractError::InvalidMemo {
                reason: SerdeError::Custom(_)
            }
        ));
    }

    // Test Cases:
    //
    // Expect Success
    //      - Memo wasn't intended for us, forward funds
    #[test]
    fn receive_standard_transfer_memo() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user1 = "user1";
        let user1_funds = Uint128::from(100u128);
        let receiving_user = "user2";

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

        // Set up a valid IBC connection
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

        // Send an unknown memo action
        let err = execute(
            deps.as_mut(),
            env,
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
                    channel: "channel-1".to_string(),
                    sender: user1.to_string(),
                    receiver: receiving_user.to_string(),
                    memo: "Hello fren, have some ASTRO".to_string(),
                })
                .unwrap(),
            }),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::InvalidDestination {}));
    }

    // Test Cases:
    //
    // Expect Success
    //      - Memo was a staking instruction, stake funds
    #[test]
    fn receive_stake_memo() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user1 = "user1";
        let user1_funds = Uint128::from(100u128);
        let ibc_timeout_seconds = 10u64;

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
                ibc_timeout_seconds,
            },
        )
        .unwrap();

        // Set up a valid IBC connection
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

        // Send a valid stake memo
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
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
        let stake_msg = to_json_binary(&astroport::staking::Cw20HookMsg::Enter {}).unwrap();
        let send_msg = to_json_binary(&Cw20ExecuteMsg::Send {
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

        // Once staked, we mint the xASTRO on the remote chain
        let mint_msg = to_json_binary(&Outpost::MintXAstro {
            receiver: user1.to_string(),
            amount: user1_funds,
        })
        .unwrap();

        // We should see the IBC message
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: IbcMsg::SendPacket {
                    channel_id: "channel-3".to_string(),
                    data: mint_msg,
                    timeout: env.block.time.plus_seconds(ibc_timeout_seconds).into(),
                }
                .into(),
            }
        );

        // At this point the channel must have a balance that matches the amount
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
            balance: user1_funds,
        };

        assert_eq!(balances, to_json_binary(&expected).unwrap());

        // At this point the total channel balance must have a balance that matches the amount
        let total_balance = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::TotalChannelBalancesAt {
                timestamp: Uint64::from(env.block.time.seconds()),
            },
        )
        .unwrap();

        let total_expected = HubBalance {
            balance: user1_funds,
        };

        assert_eq!(total_balance, to_json_binary(&total_expected).unwrap());
    }

    // Test Cases:
    //
    // Expect Success
    //      - Memo was a staking instruction, stake funds
    #[test]
    fn receive_stake_xastro_mint_timeout() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user1 = "user1";
        let user1_funds = Uint128::from(100u128);
        let ibc_timeout_seconds = 10u64;

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
                ibc_timeout_seconds,
            },
        )
        .unwrap();

        // Set up a valid IBC connection
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

        // Mint some xASTRO that we can trigger a timeout for
        // Send a valid stake memo
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(ASTRO_TOKEN, &[]),
            astroport_governance::hub::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: CW20ICS20.to_string(),
                amount: user1_funds,
                msg: to_json_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
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
        let stake_msg = to_json_binary(&astroport::staking::Cw20HookMsg::Enter {}).unwrap();
        let send_msg = to_json_binary(&Cw20ExecuteMsg::Send {
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

        // At this point the channel must hold user1_funds of value
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
            balance: user1_funds,
        };

        assert_eq!(balances, to_json_binary(&expected).unwrap());

        // At this point the total channel balance must have a balance that matches the amount
        let total_balance = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::TotalChannelBalancesAt {
                timestamp: Uint64::from(env.block.time.seconds()),
            },
        )
        .unwrap();

        let total_expected = HubBalance {
            balance: user1_funds,
        };

        assert_eq!(total_balance, to_json_binary(&total_expected).unwrap());

        // Trigger a timeout on minting xASTRO remotely
        let mint_msg = to_json_binary(&Outpost::MintXAstro {
            receiver: user1.to_owned(),
            amount: user1_funds,
        })
        .unwrap();
        let packet = IbcPacket::new(
            mint_msg,
            IbcEndpoint {
                port_id: format!("wasm.{}", MOCK_CONTRACT_ADDR),
                channel_id: "channel-3".to_string(),
            },
            IbcEndpoint {
                port_id: "wasm.outpost".to_string(),
                channel_id: "channel-5".to_string(),
            },
            3,
            env.block.time.plus_seconds(ibc_timeout_seconds).into(),
        );

        // When the timeout occurs, we should see an unstake message to return the ASTRO to the user
        let timeout_packet = IbcPacketTimeoutMsg::new(packet, Addr::unchecked("relayer"));
        let res = ibc_packet_timeout(deps.as_mut(), env.clone(), timeout_packet).unwrap();

        // Should have exactly one message
        assert_eq!(res.messages.len(), 1);

        // Verify that the unstake message matches the expected message
        let unstake_msg = to_json_binary(&astroport::staking::Cw20HookMsg::Leave {}).unwrap();
        let send_msg = to_json_binary(&Cw20ExecuteMsg::Send {
            contract: STAKING.to_string(),
            amount: user1_funds,
            msg: unstake_msg,
        })
        .unwrap();

        // We should see the unstake SubMessagge
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

        // At this point the channel must still hold the tokens
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
            balance: user1_funds,
        };

        assert_eq!(balances, to_json_binary(&expected).unwrap());

        // And the total must still match
        let total_balance = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::TotalChannelBalancesAt {
                timestamp: Uint64::from(env.block.time.seconds()),
            },
        )
        .unwrap();

        let total_expected = HubBalance {
            balance: user1_funds,
        };

        assert_eq!(total_balance, to_json_binary(&total_expected).unwrap());

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

        // At this point the channel must have a zero balance after minting remotely
        // failed and the tokens were unstaked
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

        assert_eq!(balances, to_json_binary(&expected).unwrap());

        // And now it shoul be zero
        let total_balance = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::hub::QueryMsg::TotalChannelBalancesAt {
                timestamp: Uint64::from(env.block.time.seconds()),
            },
        )
        .unwrap();

        let total_expected = HubBalance {
            balance: Uint128::zero(),
        };

        assert_eq!(total_balance, to_json_binary(&total_expected).unwrap());

        // The rest of the unstaking flow is covered in ibc_staking tests
    }
}
