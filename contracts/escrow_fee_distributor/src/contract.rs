use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use cosmwasm_std::{
    attr, entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;
use cw_storage_plus::Bound;

use astroport_governance::escrow_fee_distributor::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use astroport_governance::utils::{get_period, CLAIM_LIMIT, MIN_CLAIM_LIMIT};
use astroport_governance::voting_escrow::{get_total_voting_power_at, get_voting_power_at};

use crate::astroport;
use crate::astroport::asset::addr_opt_validate;
use crate::error::ContractError;
use crate::state::{Config, CONFIG, OWNERSHIP_PROPOSAL, REWARDS_PER_WEEK};
use crate::utils::{calc_claim_amount, calculate_reward, transfer_token_amount};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-escrow-fee-distributor";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    if let Some(claim_many_limit) = msg.claim_many_limit {
        if claim_many_limit < MIN_CLAIM_LIMIT {
            return Err(StdError::generic_err(format!(
                "Accounts limit for claim operation cannot be less than {MIN_CLAIM_LIMIT} !"
            )));
        }
    }

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            astro_token: deps.api.addr_validate(&msg.astro_token)?,
            voting_escrow_addr: deps.api.addr_validate(&msg.voting_escrow_addr)?,
            is_claim_disabled: msg.is_claim_disabled.unwrap_or(false),
            claim_many_limit: msg.claim_many_limit.unwrap_or(CLAIM_LIMIT),
        },
    )?;

    Ok(Response::new())
}

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
///
/// * **ExecuteMsg::Claim { recipient }** Claims ASTRO fees from the distributor and sends them to the recipient.
///
/// * **ExecuteMsg::ClaimMany { receivers }** Perform multiple fee claims in a single transaction.
///
/// * **ExecuteMsg::Receive(msg)** Parse incoming messages from the ASTRO token.
///
/// * **ExecuteMsg::UpdateConfig { claim_many_limit, is_claim_disabled}** Updates general settings.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    match msg {
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => propose_new_owner(
            deps,
            info,
            env,
            owner,
            expires_in,
            config.owner,
            OWNERSHIP_PROPOSAL,
        )
        .map_err(Into::into),
        ExecuteMsg::DropOwnershipProposal {} => {
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
        ExecuteMsg::Claim {
            recipient,
            max_periods,
        } => claim(deps, env, info, recipient, max_periods),
        ExecuteMsg::ClaimMany { receivers } => claim_many(deps, env, receivers),
        ExecuteMsg::UpdateConfig {
            claim_many_limit,
            is_claim_disabled,
        } => update_config(deps, info, claim_many_limit, is_claim_disabled),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** CW20 message to process.
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    if info.sender != config.astro_token {
        return Err(ContractError::Unauthorized {});
    }

    let curr_period = get_period(env.block.time.seconds())?;

    REWARDS_PER_WEEK.update(deps.storage, curr_period, |period| -> StdResult<_> {
        if let Some(tokens_amount) = period {
            Ok(tokens_amount.checked_add(cw20_msg.amount)?)
        } else {
            Ok(cw20_msg.amount)
        }
    })?;

    Ok(Response::new())
}

/// Claims ASTRO staking rewards from this contract and sends them to the `recipient`.
///
/// * **recipient** address that will receive the ASTRO staking rewards.
pub fn claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: Option<String>,
    max_periods: Option<u64>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if config.is_claim_disabled {
        return Err(ContractError::ClaimDisabled {});
    }

    let recipient_addr =
        addr_opt_validate(deps.api, &recipient)?.unwrap_or_else(|| info.sender.clone());
    let current_period = get_period(env.block.time.seconds())?;

    let claim_amount = calc_claim_amount(
        deps,
        current_period,
        &info.sender,
        &config.voting_escrow_addr,
        max_periods,
    )?;

    let transfer_msg = transfer_token_amount(&config.astro_token, &recipient_addr, claim_amount)?;

    let response = Response::new()
        .add_attributes(vec![
            attr("action", "claim"),
            attr("address", recipient_addr.to_string()),
            attr("amount", claim_amount.to_string()),
        ])
        .add_messages(transfer_msg);

    Ok(response)
}

/// Make multiple ASTRO fee claims in a single call.
///
/// * **receivers** list of addresses that will receive the claimed ASTRO.
fn claim_many(
    mut deps: DepsMut,
    env: Env,
    receivers: Vec<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if config.is_claim_disabled {
        return Err(ContractError::ClaimDisabled {});
    }

    if receivers.len() > config.claim_many_limit as usize {
        return Err(ContractError::ClaimLimitExceeded {});
    }

    let mut claim_total_amount = Uint128::zero();
    let mut transfer_msg = vec![];
    let current_period = get_period(env.block.time.seconds())?;

    for receiver in receivers {
        let receiver_addr = deps.api.addr_validate(&receiver)?;
        let claim_amount = calc_claim_amount(
            deps.branch(),
            current_period,
            &receiver_addr,
            &config.voting_escrow_addr,
            None,
        )?;

        if !claim_amount.is_zero() {
            transfer_msg.extend(transfer_token_amount(
                &config.astro_token,
                &receiver_addr,
                claim_amount,
            )?);
            claim_total_amount = claim_total_amount.checked_add(claim_amount)?;
        };
    }

    let response = Response::new()
        .add_attributes(vec![
            attr("action", "claim_many"),
            attr("amount", claim_total_amount.to_string()),
        ])
        .add_messages(transfer_msg);

    Ok(response)
}

/// Updates general contract settings.
///
/// * **claim_many_limit** max amount of rewards slots to claim in one transaction.
///
/// * **is_claim_disabled** whether reward claims are disabled or not.
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    claim_many_limit: Option<u64>,
    is_claim_disabled: Option<bool>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let mut attributes = vec![attr("action", "update_config")];

    if let Some(is_claim_disabled) = is_claim_disabled {
        if config.is_claim_disabled == is_claim_disabled {
            return Err(ContractError::Std(StdError::generic_err(format!(
                "Parameter is_claim_disabled is already {}!",
                config.is_claim_disabled
            ))));
        }
        config.is_claim_disabled = is_claim_disabled;
        attributes.push(attr("is_claim_disabled", is_claim_disabled.to_string()));
    };

    if let Some(claim_many_limit) = claim_many_limit {
        if claim_many_limit < MIN_CLAIM_LIMIT {
            return Err(StdError::generic_err(format!(
                "Accounts limit for claim operation cannot be less than {MIN_CLAIM_LIMIT} !"
            ))
            .into());
        }

        config.claim_many_limit = claim_many_limit;
        attributes.push(attr("claim_many_limit", claim_many_limit.to_string()));
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::UserReward { user, timestamp }** Returns the amount of ASTRO rewards a user can claim at a specific timestamp.
///
/// * **QueryMsg::Config {}** Returns the contract configuration.
///
/// * **QueryMsg::AvailableRewardPerWeek { start_after, limit }** Returns a vector with total amounts
/// of ASTRO distributed as rewards every week to stakers.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::UserReward { user, timestamp } => {
            to_json_binary(&query_user_reward(deps, user, timestamp)?)
        }
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::AvailableRewardPerWeek { start_after, limit } => {
            to_json_binary(&query_available_reward_per_week(deps, start_after, limit)?)
        }
    }
}

/// Pagination settings
/// The maximum limit for reading pairs from [`PAIRS`].
const MAX_LIMIT: u64 = 30;

/// The default limit for reading pairs from [`PAIRS`].
const DEFAULT_LIMIT: u64 = 10;

/// Returns a vector of weekly rewards for current vxASTRO stakers.
///
/// * **start_after** timestamp from which to start querying.
///
/// * **limit** max amount of entries to return.
fn query_available_reward_per_week(
    deps: Deps,
    start_after: Option<u64>,
    limit: Option<u64>,
) -> StdResult<Vec<Uint128>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = if let Some(timestamp) = start_after {
        Some(Bound::exclusive(get_period(timestamp)?))
    } else {
        None
    };

    REWARDS_PER_WEEK
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|week| Ok(week?.1))
        .collect()
}

/// Returns the amount of rewards a user accrued at a specific timestamp.
///
/// * **user** user for which we return the amount of rewards.
///
/// * **timestamp** timestamp at which we fetch the user's reward amount.
fn query_user_reward(deps: Deps, user: String, timestamp: u64) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let user_voting_power =
        get_voting_power_at(&deps.querier, &config.voting_escrow_addr, user, timestamp)?;
    let total_voting_power =
        get_total_voting_power_at(&deps.querier, &config.voting_escrow_addr, timestamp)?;

    if !total_voting_power.is_zero() {
        let current_period = get_period(timestamp)?;
        calculate_reward(
            deps.storage,
            current_period,
            user_voting_power,
            total_voting_power,
        )
    } else {
        Ok(Uint128::zero())
    }
}

/// Returns the contract configuration.
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;

    let resp = ConfigResponse {
        owner: config.owner,
        astro_token: config.astro_token,
        voting_escrow_addr: config.voting_escrow_addr,
        is_claim_disabled: config.is_claim_disabled,
        claim_many_limit: config.claim_many_limit,
    };

    Ok(resp)
}

/// Manages contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Err(ContractError::MigrationError {})
}
