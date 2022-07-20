use astroport_governance::astroport::asset::addr_validate_to_lower;
use astroport_governance::utils::{get_period, get_periods_count};
use astroport_governance::voting_escrow::{get_voting_power, get_voting_power_at};

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
use crate::state::{Config, Token, CONFIG, DELEGATED, RECEIVED};

use crate::helpers::DelegationHelper;
use astroport_nft::msg::{ExecuteMsg as ExecuteMsgNFT, InstantiateMsg as InstantiateMsgNFT};

// version info for migration info
const CONTRACT_NAME: &str = "voting-escrow-delegation";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Astroport NFT information.
const TOKEN_NAME: &str = "Astroport NFT";
const TOKEN_SYMBOL: &str = "ASTRO-NFT";

/// A `reply` call code ID used for sub-messages.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
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

    // Create an Astroport NFT token
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

/// ## Description
/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::CreateDelegation { percentage, cancel_time, expire_time, token_id, recipient}**
/// Delegates voting power in percent into other account.
///
/// * **ExecuteMsg::ExtendDelegation { percentage, cancel_time, expire_time, token_id, recipient}**
/// Extends a delegation already created with a new specified parameters
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let helper = DelegationHelper(env.contract.address.clone());

    match msg {
        ExecuteMsg::CreateDelegation {
            percent,
            expire_time,
            token_id,
            recipient,
        } => create_delegation(
            deps,
            env,
            info,
            &helper,
            percent,
            expire_time,
            token_id,
            recipient,
        ),
        ExecuteMsg::ExtendDelegation {
            percentage,
            expire_time,
            token_id,
            recipient,
        } => extend_delegation(
            deps,
            env,
            info,
            &helper,
            percentage,
            expire_time,
            token_id,
            recipient,
        ),
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

/// ## Description
/// Creates NFT token with specified parameters and connect it with delegated voting power
/// in percent into other account. Returns [`Response`] in case of success or
/// [`ContractError`] in case of errors.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
#[allow(clippy::too_many_arguments)]
pub fn create_delegation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    helper: &DelegationHelper,
    percent: Uint128,
    expire_time: u64,
    token_id: String,
    recipient: String,
) -> Result<Response, ContractError> {
    let recipient_addr = addr_validate_to_lower(deps.api, recipient)?;
    let user = info.sender;
    let cfg = CONFIG.load(deps.storage)?;

    // We can create only one NFT token for specify token ID
    if DELEGATED
        .may_load(deps.storage, (user.clone(), token_id.clone()))?
        .is_some()
    {
        return Err(ContractError::DelegateTokenAlreadyExists(token_id));
    }

    let mut balance = get_voting_power(&deps.querier, &cfg.voting_escrow_addr, &user)?;
    if balance.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let block_period = get_period(env.block.time.seconds())?;
    let exp_period = block_period + get_periods_count(expire_time);

    helper.checks_parameters(&deps, &cfg, &user, block_period, exp_period, percent, None)?;
    balance = helper.calc_new_balance(&deps, &user, balance, block_period)?;

    let token = helper.calc_delegate_bias_slope(balance, block_period, exp_period, percent)?;

    DELEGATED.update(
        deps.storage,
        (user, token_id.clone()),
        env.block.height,
        |_| -> StdResult<Token> { Ok(Token { ..token }) },
    )?;

    RECEIVED.update(
        deps.storage,
        (recipient_addr.clone(), token_id.clone()),
        env.block.height,
        |_| -> StdResult<Token> { Ok(Token { ..token }) },
    )?;

    Ok(Response::default()
        .add_attribute("action", "create_delegation")
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: cfg.nft_token_addr.to_string(),
            msg: to_binary(&ExecuteMsgNFT::Mint(MintMsg::<Extension> {
                token_id: token_id.clone(),
                owner: env.contract.address.to_string(),
                token_uri: None,
                extension: None,
            }))?,
            funds: vec![],
        }))
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: cfg.nft_token_addr.to_string(),
            msg: to_binary(&ExecuteMsgNFT::<Extension>::TransferNft {
                recipient: recipient_addr.to_string(),
                token_id,
            })?,
            funds: vec![],
        })))
}

#[allow(clippy::too_many_arguments)]
pub fn extend_delegation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    helper: &DelegationHelper,
    percent: Uint128,
    expire_time: u64,
    token_id: String,
    recipient: String,
) -> Result<Response, ContractError> {
    // We can extend only exists NFT token
    // if !NFT_TOKENS.has(deps.storage, token_id.clone()) {
    //     return Err(ContractError::DelegateTokenNotFound(token_id));
    // }

    let recipient_addr = addr_validate_to_lower(deps.api, recipient)?;
    let user = info.sender;
    let cfg = CONFIG.load(deps.storage)?;

    // TODO: do we need to check if NFT token exists with token_id?
    // We can create only one NFT token for specify token ID
    let old_delegate = DELEGATED.load(deps.storage, (user.clone(), token_id.clone()))?;

    let mut balance = get_voting_power(&deps.querier, &cfg.voting_escrow_addr, &user)?;
    if balance.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let block_period = get_period(env.block.time.seconds())?;
    let exp_period = block_period + get_periods_count(expire_time);

    helper.checks_parameters(
        &deps,
        &cfg,
        &user,
        block_period,
        exp_period,
        percent,
        Some(&old_delegate),
    )?;
    balance = helper.calc_extend_balance(&deps, &user, balance, &old_delegate, block_period)?;

    let new_delegate =
        helper.calc_delegate_bias_slope(balance, block_period, exp_period, percent)?;

    DELEGATED.update(
        deps.storage,
        (user, token_id.clone()),
        env.block.height,
        |_| -> StdResult<Token> { Ok(Token { ..new_delegate }) },
    )?;

    RECEIVED.update(
        deps.storage,
        (recipient_addr, token_id),
        env.block.height,
        |_| -> StdResult<Token> { Ok(Token { ..new_delegate }) },
    )?;

    Ok(Response::default().add_attribute("action", "extend_delegation"))
}

/// # Description
/// Expose available contract queries.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
/// ## Queries
/// * **QueryMsg::UserInfo { user }** Fetch user information
///
/// * **QueryMsg::TuneInfo** Fetch last tuning information
///
/// * **QueryMsg::Config** Fetch contract config
///
/// * **QueryMsg::PoolInfo { pool_addr }** Fetch pool's voting information at the current period.
///
/// * **QueryMsg::PoolInfoAtPeriod { pool_addr, period }** Fetch pool's voting information at a specified period.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let helper = DelegationHelper(env.contract.address.clone());

    match msg {
        QueryMsg::Config {} => {
            let config = CONFIG.load(deps.storage)?;
            to_binary(&Config {
                owner: config.owner,
                nft_token_addr: config.nft_token_addr,
                voting_escrow_addr: config.voting_escrow_addr,
            })
        }
        QueryMsg::AdjustedBalance { account } => to_binary(&adjusted_balance(
            deps, env, &helper, account, None, None, None,
        )?),
        QueryMsg::AdjustedBalanceAt { account, timestamp } => to_binary(&adjusted_balance(
            deps,
            env,
            &helper,
            account,
            Some(timestamp),
            None,
            None,
        )?),
        QueryMsg::AlreadyDelegatedVP { account, timestamp } => to_binary(&already_delegated_vp(
            deps, env, &helper, account, timestamp,
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
    helper: &DelegationHelper,
    account: String,
    time: Option<u64>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Uint128> {
    let account_addr = addr_validate_to_lower(deps.api, account)?;
    let config = CONFIG.load(deps.storage)?;

    let mut current_vp;
    if let Some(time) = time {
        current_vp = get_voting_power_at(
            &deps.querier,
            &config.voting_escrow_addr,
            &account_addr,
            time,
        )?;
    } else {
        current_vp = get_voting_power(&deps.querier, &config.voting_escrow_addr, &account_addr)?;
    }

    let block_period = get_period(time.unwrap_or_else(|| env.block.time.seconds()))?;
    let total_delegated_vp = helper.calc_total_delegated_vp(deps, &account_addr, block_period)?;

    // we must to subtract the delegated voting power
    if current_vp >= total_delegated_vp {
        current_vp -= total_delegated_vp;
    } else {
        // TODO: to be sure that we did not delegate more than we had
        current_vp = Uint128::zero();
    }

    let nft_helper = astroport_nft::helpers::Cw721Contract(config.nft_token_addr);
    let tokens_resp = nft_helper.tokens(&deps.querier, account_addr.clone(), start_after, limit)?;

    for token_id in tokens_resp.tokens {
        if let Some(token) =
            DELEGATED.may_load(deps.storage, (account_addr.clone(), token_id.clone()))?
        {
            if token.start <= block_period && token.expire_period >= block_period {
                let calc_vp = helper.calc_delegate_vp(&token, block_period)?;
                current_vp += calc_vp;
            }
        }

        if let Some(token) = RECEIVED.may_load(deps.storage, (account_addr.clone(), token_id))? {
            if token.start <= block_period && token.expire_period >= block_period {
                let calc_vp = helper.calc_delegate_vp(&token, block_period)?;
                current_vp += calc_vp;
            }
        }
    }

    Ok(current_vp)
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
fn already_delegated_vp(
    deps: Deps,
    env: Env,
    helper: &DelegationHelper,
    account: String,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    let account_addr = addr_validate_to_lower(deps.api, account)?;
    let block_period = get_period(timestamp.unwrap_or_else(|| env.block.time.seconds()))?;

    helper.calc_total_delegated_vp(deps, &account_addr, block_period)
}
