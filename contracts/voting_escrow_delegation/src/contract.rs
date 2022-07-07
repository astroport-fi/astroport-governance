use astroport_governance::astroport::asset::addr_validate_to_lower;
use astroport_governance::utils::{get_period, get_periods_count};
use astroport_governance::voting_escrow::{get_lock_info, get_voting_power};
use astroport_nft::MinterResponse;
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
use crate::state::{Config, DelegateInfo, CONFIG, LOCKED};

use astroport_nft::msg::InstantiateMsg as InstantiateMsgNFT;

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
pub fn delegate_vx_astro(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    receiver: String,
    _percentage: Uint128,
    _cancel_time: u64,
    expire_time: u64,
    _id: Uint128,
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

    // vxASTRO delegation must be at least WEEK
    if (expire_period < block_period + 1) && (expire_period > delegator_lock.end) {
        return Err(ContractError::DelegationPeriodError {});
    }

    // delegate vxAstro to recipient

    Ok(Response::default().add_attribute("action", "delegate_vx_astro"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => {
            let config = CONFIG.load(deps.storage)?;
            to_binary(&Config {
                owner: config.owner,
                nft_token_addr: config.nft_token_addr,
                voting_escrow_addr: config.voting_escrow_addr,
            })
        }
        QueryMsg::AdjustedBalance { .. } => to_binary(&Uint128::zero()),
        QueryMsg::AdjustedBalanceAt { .. } => to_binary(&Uint128::zero()),
    }
}
