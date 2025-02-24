use astroport::asset::{validate_native_denom, Asset, AssetInfo, AssetInfoExt};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, ensure, ensure_eq, wasm_execute, BankMsg, CosmosMsg, DepsMut, Env, MessageInfo, Order,
    ReplyOn, Response, StdError, StdResult,
};
use cw_utils::nonpayable;

use astroport_governance::emissions_controller;
use astroport_governance::emissions_controller::consts::EPOCH_LENGTH;
use astroport_governance::emissions_controller::utils::get_epoch_start;
use astroport_governance::tributes::{
    ExecuteMsg, TributeFeeInfo, TributeInfo, REWARDS_AMOUNT_LIMITS, TOKEN_TRANSFER_GAS_LIMIT,
};

use crate::error::ContractError;
use crate::reply::POST_TRANSFER_REPLY_ID;
use crate::state::{CONFIG, OWNERSHIP_PROPOSAL, TRIBUTES, USER_LAST_CLAIM_EPOCH};
use crate::utils::{asset_info_key, calculate_user_rewards};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AddTribute { lp_token, asset } => add_tribute(deps, env, info, lp_token, asset),
        ExecuteMsg::Claim { receiver } => claim_tributes(deps, env, info, receiver),
        ExecuteMsg::RemoveTribute {
            lp_token,
            asset_info,
            receiver,
        } => remove_tribute(deps, env, info, lp_token, asset_info, receiver),
        ExecuteMsg::UpdateConfig {
            tribute_fee_info,
            rewards_limit,
            token_transfer_gas_limit,
        } => update_config(
            deps,
            info,
            tribute_fee_info,
            rewards_limit,
            token_transfer_gas_limit,
        ),
        ExecuteMsg::ProposeNewOwner {
            new_owner,
            expires_in,
        } => {
            nonpayable(&info)?;
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
            nonpayable(&info)?;
            let config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            nonpayable(&info)?;
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

pub fn add_tribute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lp_token: String,
    asset: Asset,
) -> Result<Response, ContractError> {
    // Ensure we received tribute tokens
    let mut funds = info.funds.clone();
    let mut msgs: Vec<CosmosMsg> = match &asset.info {
        AssetInfo::Token { contract_addr } => {
            let pull_msg = wasm_execute(
                contract_addr,
                &cw20::Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: asset.amount,
                },
                vec![],
            )?
            .into();
            vec![pull_msg]
        }
        AssetInfo::NativeToken { denom } => {
            // Mutate funds array
            funds
                .iter_mut()
                .find(|coin| coin.denom.eq(denom))
                .and_then(|found| {
                    found.amount = found.amount.checked_sub(asset.amount).ok()?;
                    Some(())
                })
                .ok_or_else(|| ContractError::InsuffiicientTributeToken {
                    reward: asset.to_string(),
                })?;
            vec![]
        }
    };

    let config = CONFIG.load(deps.storage)?;

    // Ensure lp token is whitelisted in the emissions controller
    let is_whitelisted: Vec<(String, bool)> = deps.querier.query_wasm_smart(
        config.emissions_controller,
        &emissions_controller::hub::QueryMsg::CheckWhitelist {
            lp_tokens: vec![lp_token.clone()],
        },
    )?;
    ensure!(is_whitelisted[0].1, ContractError::LpTokenNotWhitelisted {});

    let next_epoch_start = get_epoch_start(env.block.time.seconds()) + EPOCH_LENGTH;

    // Ensure the number of rewards is within limits
    let rewards_num = TRIBUTES
        .prefix((next_epoch_start, &lp_token))
        .range_raw(deps.storage, None, None, Order::Ascending)
        .count();

    ensure!(
        rewards_num < config.rewards_limit as usize,
        ContractError::RewardsLimitExceeded {
            limit: config.rewards_limit
        }
    );

    let asset_key = asset_info_key(&asset.info);
    let tribute_key = (next_epoch_start, lp_token.as_str(), asset_key.as_slice());

    if let Some(tribute_info) = TRIBUTES.may_load(deps.storage, tribute_key)? {
        let new_amount = tribute_info.allocated + asset.amount;
        TRIBUTES.save(
            deps.storage,
            tribute_key,
            &TributeInfo {
                allocated: new_amount,
                available: new_amount,
            },
        )?;
    } else {
        // If tribute is new, we expect tribute fee
        funds
            .iter_mut()
            .find(|coin| coin.denom == config.tribute_fee_info.fee.denom)
            .and_then(|found| {
                found.amount = found
                    .amount
                    .checked_sub(config.tribute_fee_info.fee.amount)
                    .ok()?;
                Some(())
            })
            .ok_or_else(|| ContractError::TributeFeeExpected {
                fee: config.tribute_fee_info.fee.to_string(),
            })?;

        msgs.push(
            BankMsg::Send {
                to_address: config.tribute_fee_info.fee_collector.to_string(),
                amount: vec![config.tribute_fee_info.fee.clone()],
            }
            .into(),
        );

        TRIBUTES.save(
            deps.storage,
            tribute_key,
            &TributeInfo {
                allocated: asset.amount,
                available: asset.amount,
            },
        )?;
    }

    for coin in funds {
        ensure!(
            coin.amount.is_zero(),
            StdError::generic_err(format!("Supplied coins contain unexpected {coin}"))
        );
    }

    Ok(Response::new().add_messages(msgs).add_attributes([
        ("action", "add_tribute"),
        ("lp_token", &lp_token),
        ("asset", &asset.to_string()),
    ]))
}

pub fn claim_tributes(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    {
        let config = CONFIG.load(deps.storage)?;

        let (rewards, events) = calculate_user_rewards(
            deps.as_ref(),
            &config,
            info.sender.as_str(),
            env.block.time.seconds(),
        )?;

        USER_LAST_CLAIM_EPOCH.save(
            deps.storage,
            info.sender.as_str(),
            &get_epoch_start(env.block.time.seconds()),
        )?;

        let receiver = receiver.unwrap_or_else(|| info.sender.to_string());

        let rewards_msgs = rewards
            .into_iter()
            .map(|asset| {
                asset.into_submsg(
                    &receiver,
                    Some((ReplyOn::Error, POST_TRANSFER_REPLY_ID)),
                    Some(config.token_transfer_gas_limit),
                )
            })
            .collect::<StdResult<Vec<_>>>()?;

        Ok(Response::new()
            .add_submessages(rewards_msgs)
            .add_events(events)
            .add_attributes([
                ("action", "claim_tributes"),
                ("voter", info.sender.as_str()),
                ("receiver", &receiver),
            ]))
    }
}

pub fn remove_tribute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lp_token: String,
    asset_info: AssetInfo,
    receiver: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});

    let next_epoch_start = get_epoch_start(env.block.time.seconds()) + EPOCH_LENGTH;
    let asset_key = asset_info_key(&asset_info);
    let tribute_key = (next_epoch_start, lp_token.as_str(), asset_key.as_slice());

    if let Some(tribute_info) = TRIBUTES.may_load(deps.storage, tribute_key)? {
        TRIBUTES.remove(deps.storage, tribute_key);

        let send_msg = asset_info
            .with_balance(tribute_info.allocated)
            .into_submsg(
                receiver,
                Some((ReplyOn::Error, POST_TRANSFER_REPLY_ID)),
                Some(config.token_transfer_gas_limit),
            )?;

        Ok(Response::new().add_submessage(send_msg).add_attributes([
            ("action", "deregister_tribute"),
            ("lp_token", &lp_token),
            ("asset", &asset_info.to_string()),
        ]))
    } else {
        Err(ContractError::TributeNotFound {
            lp_token,
            asset_info: asset_info.to_string(),
        })
    }
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    tribute_fee_info: Option<TributeFeeInfo>,
    rewards_limit: Option<u8>,
    token_transfer_gas_limit: Option<u64>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});

    let mut attrs = vec![attr("action", "update_config")];

    if let Some(tribute_fee_info) = tribute_fee_info {
        deps.api
            .addr_validate(tribute_fee_info.fee_collector.as_str())?;
        ensure!(
            !tribute_fee_info.fee.amount.is_zero(),
            ContractError::InvalidTributeFeeAmount {}
        );
        validate_native_denom(&tribute_fee_info.fee.denom)?;

        attrs.push(attr("tribute_fee", tribute_fee_info.fee.to_string()));
        attrs.push(attr(
            "tribute_fee_collector",
            &tribute_fee_info.fee_collector,
        ));

        config.tribute_fee_info = tribute_fee_info;
    }

    if let Some(rewards_limit) = rewards_limit {
        ensure!(
            REWARDS_AMOUNT_LIMITS.contains(&rewards_limit),
            ContractError::InvalidRewardsLimit {}
        );

        attrs.push(attr("rewards_limit", rewards_limit.to_string()));

        config.rewards_limit = rewards_limit;
    }

    if let Some(token_transfer_gas_limit) = token_transfer_gas_limit {
        ensure!(
            TOKEN_TRANSFER_GAS_LIMIT.contains(&token_transfer_gas_limit),
            ContractError::InvalidTokenTransferGasLimit {}
        );

        attrs.push(attr(
            "token_transfer_gas_limit",
            token_transfer_gas_limit.to_string(),
        ));

        config.token_transfer_gas_limit = token_transfer_gas_limit;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attrs))
}
