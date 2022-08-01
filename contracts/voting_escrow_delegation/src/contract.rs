use astroport_governance::astroport::asset::addr_validate_to_lower;
use astroport_governance::utils::{get_period, get_periods_count};
use astroport_governance::voting_escrow::{get_voting_power, get_voting_power_at, MAX_LIMIT};

use crate::error::ContractError;
use crate::state::{Config, Token, CONFIG, DELEGATED, OWNERSHIP_PROPOSAL, TOKENS};
use astroport_governance::astroport::common::{
    claim_ownership, drop_ownership_proposal, propose_new_owner,
};
use astroport_governance::voting_escrow_delegation::{ExecuteMsg, InstantiateMsg, QueryMsg};

use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Reply, ReplyOn,
    Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::parse_reply_instantiate_data;

use crate::helpers::DelegationHelper;
use cw721_base::helpers as cw721_helpers;
use cw721_base::msg::{ExecuteMsg as ExecuteMsgNFT, InstantiateMsg as InstantiateMsgNFT};
use cw721_base::{Extension, MintMsg};

// version info for migration info
const CONTRACT_NAME: &str = "voting-escrow-delegation";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Astroport NFT information.
const TOKEN_NAME: &str = "Astroport NFT";
const TOKEN_SYMBOL: &str = "ASTRO-NFT";

/// A `reply` call code ID used for sub-messages.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;

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
        nft_addr: Addr::unchecked(""),
        voting_escrow_addr: addr_validate_to_lower(deps.api, &msg.voting_escrow_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    // Create an Astroport NFT token
    let sub_msg = vec![SubMsg {
        msg: WasmMsg::Instantiate {
            admin: Some(String::from(config.owner)),
            code_id: msg.nft_code_id,
            msg: to_binary(&InstantiateMsgNFT {
                name: TOKEN_NAME.to_string(),
                symbol: TOKEN_SYMBOL.to_string(),
                minter: env.contract.address.to_string(),
            })?,
            funds: vec![],
            label: String::from("Astroport NFT"),
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
    let helper = DelegationHelper(env.contract.address.clone());

    match msg {
        ExecuteMsg::CreateDelegation {
            percentage,
            expire_time,
            token_id,
            recipient,
        } => create_delegation(
            deps,
            env,
            info,
            &helper,
            percentage,
            expire_time,
            token_id,
            recipient,
        ),
        ExecuteMsg::ExtendDelegation {
            percentage,
            expire_time,
            token_id,
        } => extend_delegation(deps, env, info, &helper, percentage, expire_time, token_id),
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
    config.nft_addr = addr_validate_to_lower(deps.api, res.contract_address)?;

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new())
}

/// Creates NFT token with specified parameters and connect it with delegated voting power
/// in percent into other account. Returns [`Response`] in case of success or
/// [`ContractError`] in case of errors.
///
/// ## Params
/// * **helper** is an object of type [`DelegationHelper`] which describes support
/// delegation functions.
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
#[allow(clippy::too_many_arguments)]
pub fn create_delegation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    helper: &DelegationHelper,
    percentage: Uint128,
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

    let balance = get_voting_power(&deps.querier, &cfg.voting_escrow_addr, &user)?;
    if balance.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let block_period = get_period(env.block.time.seconds())?;
    let exp_period = block_period + get_periods_count(expire_time);

    helper.validates_parameters(
        &deps,
        &cfg,
        &user,
        block_period,
        exp_period,
        percentage,
        None,
    )?;

    let new_balance = helper.calc_new_balance(&deps, &user, balance, block_period)?;
    let delegation = helper.calc_delegate_vp(new_balance, block_period, exp_period, percentage)?;

    DELEGATED.save(
        deps.storage,
        (user, token_id.clone()),
        &delegation,
        env.block.height,
    )?;

    TOKENS.save(
        deps.storage,
        token_id.clone(),
        &delegation,
        env.block.height,
    )?;

    Ok(Response::default()
        .add_attribute("action", "create_delegation")
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: cfg.nft_addr.to_string(),
            msg: to_binary(&ExecuteMsgNFT::Mint(MintMsg::<Extension> {
                token_id: token_id.clone(),
                owner: env.contract.address.to_string(),
                token_uri: None,
                extension: None,
            }))?,
            funds: vec![],
        }))
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: cfg.nft_addr.to_string(),
            msg: to_binary(&ExecuteMsgNFT::<Extension>::TransferNft {
                recipient: recipient_addr.to_string(),
                token_id,
            })?,
            funds: vec![],
        })))
}

/// Extends a previously created delegation by a new specified parameters. Returns [`Response`] in
/// case of success or [`ContractError`] in case of errors.
///
/// ## Params
/// * **helper** is an object of type [`DelegationHelper`] which describes support delegation functions.
///
/// * **percentage** is a percentage value to determine the amount of voting power to delegate.
///
/// * **expire_time** is a point in time, at least a day in the future, at which the value of the
/// voting power will reach 0.
///
/// * **token_id** is an NFT identifier.
///
/// * **recipient** is an account to receive the delegated voting power.
#[allow(clippy::too_many_arguments)]
pub fn extend_delegation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    helper: &DelegationHelper,
    percentage: Uint128,
    expire_time: u64,
    token_id: String,
) -> Result<Response, ContractError> {
    let user = info.sender;
    let cfg = CONFIG.load(deps.storage)?;

    let old_delegation = DELEGATED.load(deps.storage, (user.clone(), token_id.clone()))?;

    let balance = get_voting_power(&deps.querier, &cfg.voting_escrow_addr, &user)?;
    if balance.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let block_period = get_period(env.block.time.seconds())?;
    let exp_period = block_period + get_periods_count(expire_time);

    helper.validates_parameters(
        &deps,
        &cfg,
        &user,
        block_period,
        exp_period,
        percentage,
        Some(&old_delegation),
    )?;

    let new_balance =
        helper.calc_extend_balance(&deps, &user, balance, &old_delegation, block_period)?;
    let new_delegation =
        helper.calc_delegate_vp(new_balance, block_period, exp_period, percentage)?;

    DELEGATED.update(
        deps.storage,
        (user, token_id.clone()),
        env.block.height,
        |_| -> StdResult<Token> { Ok(Token { ..new_delegation }) },
    )?;

    TOKENS.update(
        deps.storage,
        token_id,
        env.block.height,
        |_| -> StdResult<Token> { Ok(Token { ..new_delegation }) },
    )?;

    Ok(Response::default().add_attribute("action", "extend_delegation"))
}

/// Updates contract parameters.
///
/// ## Params
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
        cfg.voting_escrow_addr = addr_validate_to_lower(deps.api, &new_voting_escrow)?;
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
    let helper = DelegationHelper(env.contract.address.clone());

    match msg {
        QueryMsg::Config {} => {
            let config = CONFIG.load(deps.storage)?;
            to_binary(&Config {
                owner: config.owner,
                nft_addr: config.nft_addr,
                voting_escrow_addr: config.voting_escrow_addr,
            })
        }
        QueryMsg::AdjustedBalance { account, timestamp } => {
            to_binary(&adjusted_balance(deps, env, &helper, account, timestamp)?)
        }
        QueryMsg::AlreadyDelegatedVP { account, timestamp } => to_binary(&already_delegated_vp(
            deps, env, &helper, account, timestamp,
        )?),
    }
}

/// Returns an adjusted voting power balance after accounting for delegations. Returns [`Response`]
/// in case of success or [`StdError`] in case of errors.
///
/// ## Params
/// * **helper** is an object of type [`DelegationHelper`] which describes support
/// delegation functions.
///
/// * **account** is an address of the account to return adjusted balance.
///
/// * **timestamp** is a point in time, at least a day in the future, at which the value of
/// the voting power will reach 0.
fn adjusted_balance(
    deps: Deps,
    env: Env,
    helper: &DelegationHelper,
    account: String,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    let account = addr_validate_to_lower(deps.api, account)?;
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
    let total_delegated_vp = helper.calc_total_delegated_vp(deps, &account, block_period)?;

    // we must to subtract the delegated voting power
    if current_vp >= total_delegated_vp {
        current_vp -= total_delegated_vp;
    } else {
        // to be sure that we did not delegate more than we had
        current_vp = Uint128::zero();
    }

    let nft_helper = cw721_helpers::Cw721Contract(config.nft_addr);
    let mut account_tokens = nft_helper
        .tokens(&deps.querier, account.clone(), None, Some(MAX_LIMIT))?
        .tokens;

    // we need to take all tokens
    if account_tokens.len().eq(&(MAX_LIMIT as usize)) {
        loop {
            let mut tokens_resp = nft_helper.tokens(
                &deps.querier,
                account.clone(),
                account_tokens.last().cloned(),
                Some(MAX_LIMIT),
            )?;

            if tokens_resp.tokens.is_empty() {
                break;
            } else {
                account_tokens.append(&mut tokens_resp.tokens);
            }
        }
    }

    for token_id in account_tokens {
        if let Some(token) = TOKENS.may_load(deps.storage, token_id)? {
            if token.start <= block_period && token.expire_period >= block_period {
                let calc_vp = helper.calc_vp(&token, block_period)?;
                current_vp += calc_vp;
            }
        }
    }

    Ok(current_vp)
}

/// Returns an amount of delegated voting power.
///
/// ## Params
/// * **helper** is an object of type [`DelegationHelper`] which describes support
/// delegation functions.
///
/// * **account** is an address of the account to return adjusted balance.
///
/// * **timestamp** is an optional field that specifies the period for which the function
/// returns voting power.
fn already_delegated_vp(
    deps: Deps,
    env: Env,
    helper: &DelegationHelper,
    account: String,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    let account = addr_validate_to_lower(deps.api, account)?;
    let block_period = get_period(timestamp.unwrap_or_else(|| env.block.time.seconds()))?;

    helper.calc_total_delegated_vp(deps, &account, block_period)
}
