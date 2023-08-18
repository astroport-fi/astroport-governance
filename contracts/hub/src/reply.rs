use astroport::{cw20_ics20::TransferMsg, querier::query_token_balance};
use cosmwasm_std::{
    entry_point, to_binary, CosmosMsg, DepsMut, Env, IbcMsg, Reply, Response, SubMsgResult, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use astroport_governance::interchain::Outpost;

use crate::{
    error::ContractError,
    state::{
        decrease_channel_balance, get_outpost_from_cw20ics20_channel,
        get_transfer_channel_from_outpost_channel, increase_channel_balance, CONFIG, REPLY_DATA,
    },
};

/// Reply ID when staking tokens
pub const STAKE_ID: u64 = 9000;
/// Reply ID when unstaking tokens
pub const UNSTAKE_ID: u64 = 9001;

/// Handle SubMessage replies
///
/// To correctly handle staking and unstaking amount we execute the calls using
/// SubMessages and the replies are handled here
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, reply: Reply) -> Result<Response, ContractError> {
    match reply.id {
        STAKE_ID => handle_stake_reply(deps, env, reply),
        UNSTAKE_ID => handle_unstake_reply(deps, env, reply),
        _ => Err(ContractError::UnknownReplyId { id: reply.id }),
    }
}

/// Handle the reply from a staking transaction
fn handle_stake_reply(deps: DepsMut, env: Env, reply: Reply) -> Result<Response, ContractError> {
    match reply.result {
        SubMsgResult::Ok(..) => {
            let config = CONFIG.load(deps.storage)?;

            // Load the temporary data stored before the SubMessage was executed
            let reply_data = REPLY_DATA.load(deps.storage)?;

            // Determine the actual amount of xASTRO we received from staking
            // and mint on the Outpost
            let current_x_astro_balance =
                query_token_balance(&deps.querier, config.xtoken_addr, env.contract.address)?;
            let xastro_received = current_x_astro_balance.checked_sub(reply_data.value)?;

            // The channel we received the ASTRO to stake on was the CW20-ICS20
            // channel, we need to determine the channel to use for minting the
            // xASTRO be checking the known Outposts
            let outpost_channels =
                get_outpost_from_cw20ics20_channel(deps.as_ref(), &reply_data.receiving_channel)?;

            // Submit an IBC transaction to mint the same amount of xASTRO
            // we received from staking on the Outpost
            let mint_remote = Outpost::MintXAstro {
                amount: xastro_received,
                receiver: reply_data.receiver.clone(),
            };
            let msg = CosmosMsg::Ibc(IbcMsg::SendPacket {
                channel_id: outpost_channels.outpost.clone(),
                data: to_binary(&mint_remote)?,
                timeout: env
                    .block
                    .time
                    .plus_seconds(config.ibc_timeout_seconds)
                    .into(),
            });

            // Keep track of the amount of xASTRO minted on the related Outpost
            increase_channel_balance(
                deps.storage,
                env.block.time.seconds(),
                &outpost_channels.outpost,
                xastro_received,
            )?;

            Ok(Response::new()
                .add_message(msg)
                .add_attribute("action", "mint_remote_xastro")
                .add_attribute("amount", xastro_received)
                .add_attribute("channel", outpost_channels.outpost)
                .add_attribute("receiver", reply_data.receiver))
        }
        // In the case where staking fails, the funds will either automatically be returned
        // through the CW20-ICS20 contract or the user will need to manually withdraw them
        // from this contract. In either case, we don't need to do anything here as the
        // original staking memo is already a SubMessage in the CW20-ICS20 contract
        SubMsgResult::Err(err) => Err(ContractError::InvalidSubmessage { reason: err }),
    }
}

/// Handle the reply from an unstaking transaction
fn handle_unstake_reply(deps: DepsMut, env: Env, reply: Reply) -> Result<Response, ContractError> {
    match reply.result {
        SubMsgResult::Ok(..) => {
            let config = CONFIG.load(deps.storage)?;

            // Load the temporary data stored before the SubMessage was executed
            let reply_data = REPLY_DATA.load(deps.storage)?;

            // Determine the actual amount of ASTRO we received from unstaking
            // to determine how much to send back to the user
            let current_astro_balance = query_token_balance(
                &deps.querier,
                config.token_addr.clone(),
                env.contract.address,
            )?;
            let astro_received = current_astro_balance.checked_sub(reply_data.value)?;

            // The channel we received the unstaking from was the Outpost contract
            // channel, we need to determine the channel to use for sending the
            // ASTRO back using the CW20-ICS20 contract
            let outpost_channels = get_transfer_channel_from_outpost_channel(
                deps.as_ref(),
                &reply_data.receiving_channel,
            )?;

            // Send the ASTRO back to the unstaking user on the Outpost chain
            // via the CW20-ICS20 contract
            let transfer_msg = TransferMsg {
                channel: outpost_channels.cw20_ics20.clone(),
                remote_address: reply_data.receiver.clone(),
                timeout: Some(config.ibc_timeout_seconds),
                memo: None,
            };

            let transfer = Cw20ExecuteMsg::Send {
                contract: config.cw20_ics20_addr.to_string(),
                amount: astro_received,
                msg: to_binary(&transfer_msg)?,
            };

            let wasm_msg = WasmMsg::Execute {
                contract_addr: config.token_addr.to_string(),
                msg: to_binary(&transfer)?,
                funds: vec![],
            };

            // Decrease the amount of xASTRO minted via this Outpost
            decrease_channel_balance(
                deps.storage,
                env.block.time.seconds(),
                &outpost_channels.outpost,
                reply_data.original_value,
            )?;

            Ok(Response::new()
                .add_message(wasm_msg)
                .add_attribute("action", "return_unstaked_astro")
                .add_attribute("amount", astro_received)
                .add_attribute("channel", outpost_channels.cw20_ics20)
                .add_attribute("receiver", reply_data.receiver))
        }
        // If unstaking fails the error will be returned to the Outpost that would undo
        // the burning of xASTRO and return the tokens to the user
        SubMsgResult::Err(err) => Err(ContractError::InvalidSubmessage { reason: err }),
    }
}
