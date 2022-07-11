use astroport_governance::astroport::asset::addr_validate_to_lower;
use astroport_governance::utils::get_period;
use astroport_governance::voting_escrow::{get_lock_info, get_voting_power, get_voting_power_at};

use astroport_nft::{Extension, MintMsg};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Reply, ReplyOn, Response, StdResult,
    SubMsg, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::parse_reply_instantiate_data;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Config, DelegateVP, Point, Token, CONFIG, TOKENS, TOTAL_DELEGATED_VP};

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
        ExecuteMsg::DelegateVxAstro {
            receiver,
            percentage,
            cancel_time,
            expire_time,
            id,
        } => delegate_vx_astro(
            deps,
            env,
            info,
            receiver,
            percentage,
            cancel_time,
            expire_time,
            id,
        ),
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
    if TOKENS.has(deps.storage, token_id.clone()) {
        return Err(ContractError::DelegateTokenAlreadyExists(token_id));
    }

    let delegator = info.sender;
    let config = CONFIG.load(deps.storage)?;

    let delegator_balance =
        get_voting_power(&deps.querier, &config.voting_escrow_addr, &delegator)?;

    if delegator_balance.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let delegator_lock = get_lock_info(&deps.querier, &config.voting_escrow_addr, &delegator)?;
    let block_period = get_period(env.block.time.seconds())?;
    let expire_period = get_period(expire_time)?;
    let cancel_period = get_period(cancel_time)?;

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

    let new_delegate;
    if let Some(delegated_balance) = TOTAL_DELEGATED_VP.may_load(deps.storage, delegator.clone())? {
        new_delegate = calc_bias_slope(
            delegator_balance - delegated_balance.delegated,
            expire_period - block_period,
        )?;
    } else {
        new_delegate = calc_bias_slope(delegator_balance, expire_period - block_period)?;
    }

    // create a new NFT delegation token
    TOKENS.save(
        deps.storage,
        token_id.clone(),
        &Token {
            bias: new_delegate.bias,
            slope: new_delegate.slope,
            percentage,
            start: block_period,
            expire_period,
            delegator: delegator.clone(),
        },
    )?;

    TOTAL_DELEGATED_VP.update(
        deps.storage,
        delegator.clone(),
        |delegate| -> StdResult<DelegateVP> {
            if let Some(mut delegate) = delegate {
                delegate.delegated +=
                    calc_delegate_vp(new_delegate, block_period, expire_period, percentage)?;
                Ok(delegate)
            } else {
                Ok(DelegateVP {
                    delegated: calc_delegate_vp(
                        new_delegate,
                        block_period,
                        expire_period,
                        percentage,
                    )?,
                })
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

fn calc_delegate_vp(
    point: Point,
    block_period: u64,
    expire_period: u64,
    percentage: Uint128,
) -> StdResult<Uint128> {
    let delegator_vp = point.bias
        - point
            .slope
            .checked_mul(Uint128::from(expire_period - block_period))?;
    Ok(delegator_vp.multiply_ratio(percentage, Uint128::new(100)))
}

fn calc_bias_slope(vp: Uint128, dt: u64) -> Result<Point, ContractError> {
    let slope = vp
        .checked_div(Uint128::from(dt))
        .map_err(|_| ContractError::ExpiredLockPeriod {})?;
    let bias = slope * Uint128::from(dt);

    Ok(Point { bias, slope })
}

#[allow(clippy::too_many_arguments)]
pub fn delegate_vx_astro(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    receiver: String,
    _percentage: Uint128,
    _cancel_time: u64,
    expire_time: u64,
    _id: String,
) -> Result<Response, ContractError> {
    let delegator = info.sender;
    let _receiver_addr = addr_validate_to_lower(deps.api, receiver)?;

    let config = CONFIG.load(deps.storage)?;
    let delegator_vp = get_voting_power(&deps.querier, &config.voting_escrow_addr, &delegator)?;

    if delegator_vp.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let delegator_lock = get_lock_info(&deps.querier, &config.voting_escrow_addr, &delegator)?;
    let block_period = get_period(env.block.time.seconds())?;
    let expire_period = get_period(expire_time)?;

    // vxASTRO delegation must be at least WEEK and no more then lock end period
    if (expire_period <= block_period) || (expire_period > delegator_lock.end) {
        return Err(ContractError::DelegationPeriodError {});
    }

    // check if token not exists

    // delegate vxAstro to recipient

    Ok(Response::default().add_attribute("action", "delegate_vx_astro"))
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

    let nft_helper = astroport_nft::helpers::Cw721Contract(config.nft_token_addr);
    let tokens = nft_helper.tokens(&deps.querier, account_addr, start_after, limit)?;

    let block_period = get_period(time.unwrap_or_else(|| env.block.time.seconds()))?;

    for token_id in tokens.tokens {
        let token = TOKENS.load(deps.storage, token_id)?;
        if token.start >= block_period && token.expire_period > block_period {
            total_vp += calc_delegate_vp(
                Point {
                    bias: token.bias,
                    slope: token.slope,
                },
                block_period,
                token.expire_period,
                token.percentage,
            )?;
        }
    }

    Ok(total_vp)
}
