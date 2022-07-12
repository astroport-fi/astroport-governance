use astroport_governance::astroport::asset::addr_validate_to_lower;
use astroport_governance::utils::{get_period, get_periods_count};
use astroport_governance::voting_escrow::{get_lock_info, get_voting_power, get_voting_power_at};

use astroport_governance::U64Key;
use astroport_nft::{Extension, MintMsg};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Reply, ReplyOn, Response,
    StdResult, SubMsg, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;
use cw_utils::parse_reply_instantiate_data;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Config, Token, CONFIG, DELEGATED, NFT_TOKENS, RECEIVED};

use astroport_nft::msg::{ExecuteMsg as ExecuteMsgNFT, InstantiateMsg as InstantiateMsgNFT};

// version info for migration info
const CONTRACT_NAME: &str = "voting-escrow-delegation";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Astroport NFT information.
const TOKEN_NAME: &str = "Astroport NFT";
const TOKEN_SYMBOL: &str = "ASTRO-NFT";

/// A `reply` call code ID used for sub-messages.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        nft_token_addr: Addr::unchecked(""),
        voting_escrow_addr: addr_validate_to_lower(deps.api, &msg.voting_escrow_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    // Create the Astroport NFT delegate token
    let sub_msg = vec![SubMsg {
        msg: WasmMsg::Instantiate {
            admin: Some(String::from(config.owner)),
            code_id: msg.nft_token_code_id,
            msg: to_binary(&InstantiateMsgNFT {
                name: TOKEN_NAME.to_string(),
                symbol: TOKEN_SYMBOL.to_string(),
                minter: env.contract.address.to_string(),
            })?,
            funds: vec![],
            label: String::from("Astroport NFT token "),
        }
        .into(),
        id: INSTANTIATE_TOKEN_REPLY_ID,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    }];

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", info.sender)
        .add_submessages(sub_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::DelegateVotingPower { receiver, token_id } => {
            delegate_voting_power(deps, env, receiver, token_id)
        }
        ExecuteMsg::CreateDelegation {
            percentage,
            cancel_time,
            expire_time,
            id,
        } => create_delegation(deps, env, info, percentage, cancel_time, expire_time, id),
    }
}

/// ## Description
/// The entry point to the contract for processing replies from submessages. For now it only sets the xASTRO contract address.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`Reply`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if config.nft_token_addr != Addr::unchecked("") {
        return Err(ContractError::Unauthorized {});
    }

    let res = parse_reply_instantiate_data(msg)?;
    config.nft_token_addr = addr_validate_to_lower(deps.api, res.contract_address)?;

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new())
}

#[allow(clippy::too_many_arguments)]
pub fn create_delegation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    percentage: Uint128,
    cancel_time: u64,
    expire_time: u64,
    token_id: String,
) -> Result<Response, ContractError> {
    // We can create only one NFT token for specify token ID
    if NFT_TOKENS.has(deps.storage, token_id.clone()) {
        return Err(ContractError::DelegateTokenAlreadyExists(token_id));
    }

    let delegator = info.sender;
    let config = CONFIG.load(deps.storage)?;

    let mut delegator_balance =
        get_voting_power(&deps.querier, &config.voting_escrow_addr, &delegator)?;

    if delegator_balance.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let delegator_lock = get_lock_info(&deps.querier, &config.voting_escrow_addr, &delegator)?;
    let block_period = get_period(env.block.time.seconds())?;
    let expire_period = block_period + get_periods_count(expire_time);
    let cancel_period = block_period + get_periods_count(cancel_time);

    if cancel_period > expire_period {
        return Err(ContractError::CancelTimeWrong {});
    }

    // vxASTRO delegation must be at least WEEK and no more then lock end period
    if (expire_period <= block_period) || (expire_period > delegator_lock.end) {
        return Err(ContractError::DelegationPeriodError {});
    }

    if percentage.is_zero() || percentage.gt(&Uint128::new(100)) {
        return Err(ContractError::PercentageError {});
    }

    let total_delegated_vp = calc_delegated_vp(deps.as_ref(), &delegator, block_period)?;
    if delegator_balance <= total_delegated_vp {
        return Err(ContractError::DelegationVotingPowerNotAllowed {});
    }

    let new_delegate =
        calc_delegate_bias_slope(delegator_balance, block_period, expire_period, percentage)?;

    // let delegate_token_info = if let Some(delegated) = DELEGATED.may_load(
    //     deps.storage,
    //     (delegator.clone(), U64Key::from(block_period)),
    // )? {
    //     calc_delegate_bias_slope(
    //         delegator_balance - delegated.bias,
    //         block_period,
    //         expire_period,
    //         percentage,
    //     )?
    // } else {
    //     calc_delegate_bias_slope(delegator_balance, block_period, expire_period, percentage)?
    // };

    // create a new delegation NFT token
    NFT_TOKENS.save(deps.storage, token_id.clone(), &new_delegate)?;

    DELEGATED.update(
        deps.storage,
        (delegator.clone(), U64Key::new(block_period)),
        env.block.height,
        |token| -> StdResult<Token> {
            if let Some(mut token) = token {
                token.bias += new_delegate.bias;
                token.start = new_delegate.start;
                token.slope = new_delegate.slope;
                token.expire_period = new_delegate.expire_period;
                Ok(token)
            } else {
                Ok(Token { ..new_delegate })
            }
        },
    )?;

    Ok(Response::default()
        .add_attribute("action", "create_delegation")
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: config.nft_token_addr.to_string(),
            msg: to_binary(&ExecuteMsgNFT::Mint(MintMsg::<Extension> {
                token_id,
                owner: delegator.to_string(),
                token_uri: None,
                extension: None,
            }))?,
            funds: vec![],
        })))
}

fn calc_delegated_vp(deps: Deps, delegator: &Addr, block_period: u64) -> StdResult<Uint128> {
    let old_delegates = DELEGATED
        .prefix(delegator.clone())
        .range(
            deps.storage,
            None,
            Some(Bound::inclusive(block_period.clone())),
            Order::Ascending,
        )
        .collect::<StdResult<Vec<_>>>()?;

    let mut total_delegated_vp = Uint128::zero();
    for old_delegate in old_delegates {
        if old_delegate.1.expire_period >= block_period {
            total_delegated_vp += old_delegate.1.bias;
        }
    }

    Ok(total_delegated_vp)
}

fn calc_delegate_vp(token_info: Token, block_period: u64) -> StdResult<Uint128> {
    let dt = Uint128::from(token_info.expire_period - block_period);
    Ok(token_info.bias - token_info.slope.checked_mul(dt)?)
}

fn calc_delegate_bias_slope(
    vp: Uint128,
    block_period: u64,
    expire_period: u64,
    percentage: Uint128,
) -> Result<Token, ContractError> {
    let delegated_vp = vp.multiply_ratio(percentage, Uint128::new(100));
    let dt = Uint128::from(expire_period - block_period);
    let slope = delegated_vp
        .checked_div(dt)
        .map_err(|e| ContractError::Std(e.into()))?;
    let bias = slope * dt;

    Ok(Token {
        bias,
        slope,
        start: block_period,
        expire_period,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn delegate_voting_power(
    deps: DepsMut,
    env: Env,
    receiver: String,
    token_id: String,
) -> Result<Response, ContractError> {
    let receiver_addr = addr_validate_to_lower(deps.api, receiver)?;
    let config = CONFIG.load(deps.storage)?;
    let block_period = get_period(env.block.time.seconds())?;

    let token_to_transfer = NFT_TOKENS.load(deps.storage, token_id.clone())?;

    RECEIVED.update(
        deps.storage,
        (receiver_addr.clone(), U64Key::new(block_period)),
        env.block.height,
        |token| -> StdResult<Token> {
            if let Some(mut token) = token {
                token.bias += token_to_transfer.bias;
                token.slope = token_to_transfer.slope;
                token.expire_period = token_to_transfer.expire_period;
                Ok(token)
            } else {
                Ok(Token {
                    ..token_to_transfer
                })
            }
        },
    )?;

    Ok(Response::default()
        .add_attribute("action", "delegate_voting_power")
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: config.nft_token_addr.to_string(),
            msg: to_binary(&ExecuteMsgNFT::<Extension>::TransferNft {
                recipient: receiver_addr.to_string(),
                token_id,
            })?,
            funds: vec![],
        })))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => {
            let config = CONFIG.load(deps.storage)?;
            to_binary(&Config {
                owner: config.owner,
                nft_token_addr: config.nft_token_addr,
                voting_escrow_addr: config.voting_escrow_addr,
            })
        }
        QueryMsg::AdjustedBalance { account } => {
            to_binary(&adjusted_balance(deps, env, account, None, None, None)?)
        }
        QueryMsg::AdjustedBalanceAt { account, timestamp } => to_binary(&adjusted_balance(
            deps,
            env,
            account,
            Some(timestamp),
            None,
            None,
        )?),
    }
}

/// ## Description
/// Returns a account balance with delegation.
///
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **account** is an object of type [`String`].
///
/// * **time** is an object of type [`Option<u64>`]. This is an optional field that specifies
/// the period for which the function returns voting power.
///
/// * **start_after** is an object of type [`Option<String>`]. This is an optional field
/// that specifies whether the function should return a list of NFT tokens starting from a
/// specific ID onward.
///
/// * **limit** is an object of type [`Option<u32>`]. This is the max amount of NFT tokens
/// to return.
fn adjusted_balance(
    deps: Deps,
    env: Env,
    account: String,
    time: Option<u64>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Uint128> {
    let account_addr = addr_validate_to_lower(deps.api, account)?;
    let config = CONFIG.load(deps.storage)?;

    let mut total_vp;
    if let Some(time) = time {
        total_vp = get_voting_power_at(
            &deps.querier,
            &config.voting_escrow_addr,
            &account_addr,
            time,
        )?;
    } else {
        total_vp = get_voting_power(&deps.querier, &config.voting_escrow_addr, &account_addr)?;
    }

    let block_period = get_period(time.unwrap_or_else(|| env.block.time.seconds()))?;
    total_vp -= calc_delegated_vp(deps, &account_addr, block_period)?;
    // if let Some(delegated) = DELEGATED.may_load(
    //     deps.storage,
    //     (account_addr.clone(), U64Key::from(block_period)),
    // )? {
    //     total_vp -= delegated.bias;
    // };

    let nft_helper = astroport_nft::helpers::Cw721Contract(config.nft_token_addr);
    let tokens_resp = nft_helper.tokens(&deps.querier, account_addr, start_after, limit)?;

    for token_id in tokens_resp.tokens {
        if let Some(token) = NFT_TOKENS.may_load(deps.storage, token_id)? {
            if token.start <= block_period && token.expire_period >= block_period {
                let calc_vp = calc_delegate_vp(token, block_period)?;
                total_vp += calc_vp;
            }
        }
    }

    Ok(total_vp)
}
