use std::marker::PhantomData;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_json_binary, Addr, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Reply, ReplyOn,
    Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw721::NftInfoResponse;
use cw721_base::helpers as cw721_helpers;
use cw721_base::msg::{ExecuteMsg as ExecuteMsgNFT, InstantiateMsg as InstantiateMsgNFT};
use cw721_base::{Extension, MintMsg};
use cw_utils::parse_reply_instantiate_data;

use astroport_governance::astroport::common::{
    claim_ownership, drop_ownership_proposal, propose_new_owner,
};
use astroport_governance::utils::{calc_voting_power, get_period, get_periods_count};
use astroport_governance::voting_escrow::{get_voting_power, get_voting_power_at, MAX_LIMIT};
use astroport_governance::voting_escrow_delegation::{
    Config, ExecuteMsg, InstantiateMsg, QueryMsg,
};

use crate::error::ContractError;
use crate::helpers::{
    calc_delegation, calc_extend_delegation, calc_not_delegated_vp, calc_total_delegated_vp,
    validate_parameters,
};
use crate::state::{CONFIG, DELEGATED, OWNERSHIP_PROPOSAL, TOKENS};

// Version info for contract migration.
const CONTRACT_NAME: &str = "voting-escrow-delegation";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Delegated voting power NFT information.
const TOKEN_NAME: &str = "Delegated VP NFT";
const TOKEN_SYMBOL: &str = "VP-NFT";

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        nft_addr: Addr::unchecked(""),
        voting_escrow_addr: deps.api.addr_validate(&msg.voting_escrow_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    // Create an Astroport NFT
    let sub_msg = SubMsg {
        msg: WasmMsg::Instantiate {
            admin: Some(config.owner.to_string()),
            code_id: msg.nft_code_id,
            msg: to_json_binary(&InstantiateMsgNFT {
                name: TOKEN_NAME.to_string(),
                symbol: TOKEN_SYMBOL.to_string(),
                minter: env.contract.address.to_string(),
            })?,
            funds: vec![],
            label: String::from("Delegated VP NFT"),
        }
        .into(),
        id: 1,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", config.owner)
        .add_submessage(sub_msg))
}

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::CreateDelegation { percentage, cancel_time, expire_time, token_id, recipient}**
/// Delegates voting power in percent into other account.
///
/// * **ExecuteMsg::ExtendDelegation { percentage, cancel_time, expire_time, token_id, recipient}**
/// Extends an already created delegation with a new specified parameters
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change
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
        ExecuteMsg::CreateDelegation {
            bps,
            expire_time,
            token_id,
            recipient,
        } => create_delegation(deps, env, info, bps, expire_time, token_id, recipient),
        ExecuteMsg::ExtendDelegation {
            bps,
            expire_time,
            token_id,
        } => extend_delegation(deps, env, info, bps, expire_time, token_id),
        ExecuteMsg::UpdateConfig { new_voting_escrow } => {
            update_config(deps, info, new_voting_escrow)
        }
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

/// The entry point to the contract for processing replies from sub-messages. For now it only
/// sets the NFT contract address.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if config.nft_addr != Addr::unchecked("") {
        return Err(ContractError::Unauthorized {});
    }

    let res = parse_reply_instantiate_data(msg)?;
    config.nft_addr = deps.api.addr_validate(res.contract_address.as_str())?;

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new())
}

/// Creates NFT with specified parameters and connect it with delegated voting power
/// in percent into other account.
///
/// * **percentage** is a percentage value to determine the amount of
/// voting power to delegate
///
/// * **expire_time** is a point in time, at least a day in the future, at which the value of the
/// voting power will reach 0.
///
/// * **token_id** is an NFT identifier.
///
/// * **recipient** is an account to receive the delegated voting power.
pub fn create_delegation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    bps: u16,
    expire_time: u64,
    token_id: String,
    recipient: String,
) -> Result<Response, ContractError> {
    let recipient_addr = deps.api.addr_validate(recipient.as_str())?;
    let delegator = info.sender;
    let cfg = CONFIG.load(deps.storage)?;
    let block_period = get_period(env.block.time.seconds())?;
    let exp_period = block_period + get_periods_count(expire_time);

    // We can create only one NFT for a specific token ID
    let nft_helper = cw721_helpers::Cw721Contract::<Empty, Empty>(
        cfg.nft_addr.clone(),
        PhantomData,
        PhantomData,
    );

    let nft_instance: StdResult<NftInfoResponse<Extension>> =
        nft_helper.nft_info(&deps.querier, &token_id);

    if nft_instance.is_ok() {
        return Err(ContractError::DelegationTokenAlreadyExists(token_id));
    }

    let vp = get_voting_power(&deps.querier, &cfg.voting_escrow_addr, &delegator)?;
    if vp.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    validate_parameters(
        &deps.querier,
        &cfg,
        &delegator,
        block_period,
        exp_period,
        bps,
        None,
    )?;

    let not_delegated_vp = calc_not_delegated_vp(deps.as_ref(), &delegator, vp, block_period)?;
    let delegation = calc_delegation(not_delegated_vp, block_period, exp_period, bps)?;

    DELEGATED.save(deps.storage, (&delegator, token_id.clone()), &delegation)?;
    TOKENS.save(deps.storage, token_id.clone(), &delegation)?;

    Ok(Response::default()
        .add_attributes(vec![
            attr("action", "create_delegation"),
            attr("recipient", recipient),
            attr("token_id", token_id.clone()),
            attr("expire_time", expire_time.to_string()),
            attr("bps", bps.to_string()),
            attr("delegated_voting_power", delegation.power.to_string()),
        ])
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: cfg.nft_addr.to_string(),
            msg: to_json_binary(&ExecuteMsgNFT::<Extension, Empty>::Mint(MintMsg::<
                Extension,
            > {
                token_id,
                owner: recipient_addr.to_string(),
                token_uri: None,
                extension: None,
            }))?,
            funds: vec![],
        })))
}

/// Extends a previously created delegation by a new specified parameters.
///
/// * **percentage** is a percentage value to determine the amount of voting power to delegate.
///
/// * **expire_time** is a point in time, at least a day in the future, at which the value of the
/// voting power will reach 0.
///
/// * **token_id** is an NFT identifier.
///
/// * **recipient** is an account to receive the delegated voting power.
pub fn extend_delegation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    bps: u16,
    expire_time: u64,
    token_id: String,
) -> Result<Response, ContractError> {
    let delegator = info.sender;
    let cfg = CONFIG.load(deps.storage)?;

    let old_delegation = DELEGATED.load(deps.storage, (&delegator, token_id.clone()))?;

    let vp = get_voting_power(&deps.querier, &cfg.voting_escrow_addr, &delegator)?;
    if vp.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let block_period = get_period(env.block.time.seconds())?;
    let exp_period = block_period + get_periods_count(expire_time);

    validate_parameters(
        &deps.querier,
        &cfg,
        &delegator,
        block_period,
        exp_period,
        bps,
        Some(&old_delegation),
    )?;

    let new_delegation = calc_extend_delegation(
        deps.as_ref(),
        &delegator,
        vp,
        &old_delegation,
        block_period,
        exp_period,
        bps,
    )?;

    DELEGATED.save(
        deps.storage,
        (&delegator, token_id.clone()),
        &new_delegation,
    )?;
    TOKENS.save(deps.storage, token_id.clone(), &new_delegation)?;

    Ok(Response::default().add_attributes(vec![
        attr("action", "extend_delegation"),
        attr("token_id", token_id),
        attr("expire_time", expire_time.to_string()),
        attr("bps", bps.to_string()),
        attr("delegated_voting_power", new_delegation.power.to_string()),
    ]))
}

/// Updates contract parameters.
///
/// * **new_voting_escrow** is a new address of Voting Escrow contract.
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_voting_escrow: Option<String>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(new_voting_escrow) = new_voting_escrow {
        cfg.voting_escrow_addr = deps.api.addr_validate(&new_voting_escrow)?;
    }

    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::default().add_attribute("action", "execute_update_config"))
}

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::Config {}** Fetch contract config
///
/// * **QueryMsg::AdjustedBalance { account, timestamp }** Adjusted voting power balance after
/// accounting for delegations.
///
/// * **QueryMsg::AlreadyDelegatedVP { account, timestamp }** Returns the amount of delegated
/// voting power according to the given parameters.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::AdjustedBalance { account, timestamp } => {
            to_json_binary(&adjusted_balance(deps, env, account, timestamp)?)
        }
        QueryMsg::DelegatedVotingPower { account, timestamp } => {
            to_json_binary(&delegated_vp(deps, env, account, timestamp)?)
        }
    }
}

/// Returns an adjusted voting power balance after accounting for delegations.
///
/// * **account** is an address of the account to return adjusted balance.
///
/// * **timestamp** is a point in time, at least a day in the future, at which the value of
/// the voting power will reach 0.
fn adjusted_balance(
    deps: Deps,
    env: Env,
    account: String,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    let account = deps.api.addr_validate(account.as_str())?;
    let config = CONFIG.load(deps.storage)?;

    let mut current_vp = if let Some(timestamp) = timestamp {
        get_voting_power_at(
            &deps.querier,
            &config.voting_escrow_addr,
            &account,
            timestamp,
        )?
    } else {
        get_voting_power(&deps.querier, &config.voting_escrow_addr, &account)?
    };

    let block_period = get_period(timestamp.unwrap_or_else(|| env.block.time.seconds()))?;
    let total_delegated_vp = calc_total_delegated_vp(deps, &account, block_period)?;

    // we must to subtract the delegated voting power
    current_vp = current_vp.checked_sub(total_delegated_vp)?;

    let nft_helper =
        cw721_helpers::Cw721Contract::<Empty, Empty>(config.nft_addr, PhantomData, PhantomData);

    let mut account_tokens = vec![];
    let mut start_after = None;

    // we need to take all tokens for specified account
    loop {
        let tokens = nft_helper
            .tokens(&deps.querier, account.clone(), start_after, Some(MAX_LIMIT))?
            .tokens;
        if tokens.is_empty() {
            break;
        }
        start_after = tokens.last().cloned();
        account_tokens.extend(tokens);
    }

    for token_id in account_tokens {
        let token = TOKENS.load(deps.storage, token_id)?;

        if token.start <= block_period && token.expire_period > block_period {
            current_vp += calc_voting_power(token.slope, token.power, token.start, block_period);
        }
    }

    Ok(current_vp)
}

/// Returns an amount of delegated voting power.
///
/// * **account** is an address of the account to return adjusted balance.
///
/// * **timestamp** is an optional field that specifies the period for which the function
/// returns voting power.
fn delegated_vp(
    deps: Deps,
    env: Env,
    account: String,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    let account = deps.api.addr_validate(account.as_str())?;
    let block_period = get_period(timestamp.unwrap_or_else(|| env.block.time.seconds()))?;

    calc_total_delegated_vp(deps, &account, block_period)
}
