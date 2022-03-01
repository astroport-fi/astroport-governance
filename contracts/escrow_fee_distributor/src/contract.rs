use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Attribute, Binary, Deps, DepsMut, Env, MessageInfo, Order,
    Response, StdError, StdResult, Uint128,
};

use crate::error::ContractError;
use crate::state::{Config, CONFIG, LAST_CLAIM_PERIOD, OWNERSHIP_PROPOSAL, REWARDS_PER_WEEK};

use crate::utils::transfer_token_amount;
use astroport::asset::addr_validate_to_lower;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};

use astroport_governance::escrow_fee_distributor::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use astroport_governance::utils::{get_period, CLAIM_LIMIT};

use astroport_governance::voting_escrow::{
    LockInfoResponse, QueryMsg as VotingQueryMsg, VotingPowerResponse,
};
use cw20::Cw20ReceiveMsg;

use cw2::set_contract_version;
use cw_storage_plus::{Bound, PrimaryKey, U64Key};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-escrow_fee_distributor";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default [`Response`] object if the operation was successful, otherwise returns
/// the [`StdResult`] if the contract was not created.
/// ## Params
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for
/// creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: addr_validate_to_lower(deps.api, &msg.owner)?,
            astro_token: addr_validate_to_lower(deps.api, &msg.astro_token)?,
            voting_escrow_addr: addr_validate_to_lower(deps.api, &msg.voting_escrow_addr)?,
            is_claim_disabled: msg.is_claim_disabled.unwrap_or(false),
            claim_many_limit: msg.claim_many_limit.unwrap_or(CLAIM_LIMIT),
        },
    )?;

    Ok(Response::new())
}

/// ## Description
/// Available the execute messages of the contract.
/// ## Queries
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a request to change ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Approves ownership.
///
/// * **ExecuteMsg::Claim { recipient }** Claims the tokens from distributor for transfer
/// to the recipient.
///
/// * **ExecuteMsg::ClaimMany { receivers }**  Make multiple fee claims in a single call.
///
/// * **ExecuteMsg::Receive(msg)** parse incoming message from the ASTRO token.
/// msg should have [`Cw20ReceiveMsg`] type.
///
/// * **ExecuteMsg::UpdateConfig { claim_many_limit, is_claim_disabled}** Updates
/// general settings. Returns an [`ContractError`] on failure or the following [`Config`]
/// data will be updated if successful.
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
        .map_err(|e| e.into()),
        ExecuteMsg::DropOwnershipProposal {} => {
            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(|e| e.into())
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
            .map_err(|e| e.into())
        }
        ExecuteMsg::Claim { recipient } => claim(deps, env, info, recipient),
        ExecuteMsg::ClaimMany { receivers } => claim_many(deps, env, receivers),
        ExecuteMsg::UpdateConfig {
            claim_many_limit,
            is_claim_disabled,
        } => update_config(deps, info, claim_many_limit, is_claim_disabled),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise returns the [`Response`] with the specified attributes if the operation was successful
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

    REWARDS_PER_WEEK.update(
        deps.storage,
        U64Key::new(curr_period),
        |period| -> StdResult<_> {
            if let Some(tokens_amount) = period {
                Ok(tokens_amount + cw20_msg.amount)
            } else {
                Ok(cw20_msg.amount)
            }
        },
    )?;

    Ok(Response::new())
}

/// ## Description
/// Claims the amount from FeeDistributor for transfer to the recipient. Returns the [`Response`] with
/// specified attributes if operation was successful, otherwise returns the [`ContractError`].
/// ## Params
/// * **recipient** Sets the recipient for claim.
pub fn claim(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: Option<String>,
) -> Result<Response, ContractError> {
    let recipient_addr = addr_validate_to_lower(
        deps.api,
        &recipient.unwrap_or_else(|| info.sender.to_string()),
    )?;

    let config: Config = CONFIG.load(deps.storage)?;

    if config.is_claim_disabled {
        return Err(ContractError::ClaimDisabled {});
    }

    let claim_amount = calc_claim_amount(deps.branch(), env, info.sender, config.clone())?;

    let mut transfer_msg = vec![];
    if !claim_amount.is_zero() {
        transfer_msg =
            transfer_token_amount(config.astro_token, recipient_addr.clone(), claim_amount)?;
    };

    let response = Response::new()
        .add_attributes(vec![
            attr("action", "claim"),
            attr("address", recipient_addr.to_string()),
            attr("amount", claim_amount.to_string()),
        ])
        .add_messages(transfer_msg);

    Ok(response)
}

/// ## Description
/// Make multiple fee claims in a single call. Returns the [`Response`] with
/// specified attributes if operation was successful, otherwise returns the [`ContractError`].
/// ## Params
/// * **receivers** is vector field of type [`String`]. Sets the receivers for claim.
fn claim_many(
    mut deps: DepsMut,
    env: Env,
    receivers: Vec<String>,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;

    if config.is_claim_disabled {
        return Err(ContractError::ClaimDisabled {});
    }

    if receivers.len() > config.claim_many_limit as usize {
        return Err(ContractError::ClaimLimitExceeded {});
    }

    let mut claim_total_amount = Uint128::zero();
    let mut transfer_msg = vec![];

    for receiver in receivers {
        let receiver_addr = addr_validate_to_lower(deps.api, &receiver)?;
        let claim_amount = calc_claim_amount(
            deps.branch(),
            env.clone(),
            receiver_addr.clone(),
            config.clone(),
        )?;

        if !claim_amount.is_zero() {
            transfer_msg.extend(transfer_token_amount(
                config.astro_token.clone(),
                receiver_addr,
                claim_amount,
            )?);
            claim_total_amount += claim_amount;
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

/// ## Description
/// Calculation amount of claim for specified account.
fn calc_claim_amount(deps: DepsMut, env: Env, account: Addr, config: Config) -> StdResult<Uint128> {
    let user_lock_info: LockInfoResponse = deps.querier.query_wasm_smart(
        &config.voting_escrow_addr,
        &VotingQueryMsg::LockInfo {
            user: account.to_string(),
        },
    )?;

    let mut claim_period = LAST_CLAIM_PERIOD
        .may_load(deps.storage, account.clone())?
        .unwrap_or(user_lock_info.start);

    let current_period = get_period(env.block.time.seconds())?;
    let lock_end_period = user_lock_info.end;
    let mut claim_amount: Uint128 = Default::default();

    loop {
        // user cannot claim for current period
        if claim_period >= current_period {
            break;
        }

        // user cannot claim higher than his max lock period
        if claim_period > lock_end_period {
            break;
        }

        let user_voting_power: VotingPowerResponse = deps.querier.query_wasm_smart(
            &config.voting_escrow_addr,
            &VotingQueryMsg::UserVotingPowerAtPeriod {
                user: account.to_string(),
                period: claim_period,
            },
        )?;

        let total_voting_power: VotingPowerResponse = deps.querier.query_wasm_smart(
            &config.voting_escrow_addr,
            &VotingQueryMsg::TotalVotingPowerAtPeriod {
                period: claim_period,
            },
        )?;

        if !user_voting_power.voting_power.is_zero() && !total_voting_power.voting_power.is_zero() {
            claim_amount = claim_amount.checked_add(calculate_reward(
                deps.as_ref(),
                claim_period,
                user_voting_power.voting_power,
                total_voting_power.voting_power,
            )?)?;
        }

        claim_period += 1;
    }

    LAST_CLAIM_PERIOD.save(deps.storage, account, &claim_period)?;

    Ok(claim_amount)
}

/// ## Description
/// Returns user amount for specified period
fn calculate_reward(
    deps: Deps,
    period: u64,
    user_vp: Uint128,
    total_vp: Uint128,
) -> StdResult<Uint128> {
    let rewards_per_week = REWARDS_PER_WEEK
        .may_load(deps.storage, U64Key::from(period))?
        .unwrap_or_default();

    Ok(user_vp.multiply_ratio(rewards_per_week, total_vp))
}

/// ## Description
/// Updates general settings. Returns an [`ContractError`] on failure or the following [`Config`]
/// data will be updated if successful.
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    claim_many_limit: Option<u64>,
    is_claim_disabled: Option<bool>,
) -> Result<Response, ContractError> {
    let mut attributes = vec![attr("action", "update_config")];
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(is_claim_disabled) = is_claim_disabled {
        config.is_claim_disabled = is_claim_disabled;
        attributes.push(Attribute::new(
            "is_claim_disabled",
            is_claim_disabled.to_string(),
        ));
    };

    if let Some(claim_many_limit) = claim_many_limit {
        config.claim_many_limit = claim_many_limit;
        attributes.push(Attribute::new(
            "claim_many_limit",
            claim_many_limit.to_string(),
        ));
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

/// ## Description
/// Available the query messages of the contract.
/// ## Queries
/// * **QueryMsg::UserReward { user, timestamp }** Returns the reward amount by
/// the timestamp.
///
/// * **QueryMsg::Config {}** Returns the base controls configs that contains in the [`Config`]
/// object.
///
/// * **QueryMsg::AvailableRewardPerWeek { start_after, limit }** Returns a vector of the amount
/// of rewards for the week with the specified parameters
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::UserReward { user, timestamp } => {
            to_binary(&query_user_reward(deps, env, user, timestamp)?)
        }
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::AvailableRewardPerWeek { start_from, limit } => {
            to_binary(&query_available_reward_per_week(deps, start_from, limit)?)
        }
    }
}

//settings for pagination
/// The maximum limit for reading pairs from a [`PAIRS`]
const MAX_LIMIT: u64 = 30;

/// The default limit for reading pairs from a [`PAIRS`]
const DEFAULT_LIMIT: u64 = 10;

/// ## Description
/// Returns a vector of the amount of rewards for the week with the specified parameters
fn query_available_reward_per_week(
    deps: Deps,
    start_from: Option<u64>,
    limit: Option<u64>,
) -> StdResult<Vec<Uint128>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = if let Some(timestamp) = start_from {
        let bound = U64Key::from(get_period(timestamp)?).joined_key();
        Some(Bound::Inclusive(bound))
    } else {
        None
    };

    REWARDS_PER_WEEK
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|week| Ok(week?.1))
        .collect::<StdResult<Vec<_>>>()
}

/// ## Description
/// Returns the user rewards amount by the timestamp
fn query_user_reward(deps: Deps, _env: Env, user: String, timestamp: u64) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let user_voting_power: VotingPowerResponse = deps.querier.query_wasm_smart(
        &config.voting_escrow_addr,
        &VotingQueryMsg::UserVotingPowerAt {
            user,
            time: timestamp,
        },
    )?;

    let total_voting_power: VotingPowerResponse = deps.querier.query_wasm_smart(
        &config.voting_escrow_addr,
        &VotingQueryMsg::TotalVotingPowerAt { time: timestamp },
    )?;

    let current_period = get_period(timestamp)?;

    if !total_voting_power.voting_power.is_zero() {
        Ok(calculate_reward(
            deps,
            current_period,
            user_voting_power.voting_power,
            total_voting_power.voting_power,
        )?)
    } else {
        Ok(Uint128::zero())
    }
}

/// ## Description
/// Returns information about the distributor configs in the [`ConfigResponse`] object.
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;

    let resp = ConfigResponse {
        owner: config.owner,
        astro_token: config.astro_token,
        voting_escrow_addr: config.voting_escrow_addr,
        is_claim_disabled: config.is_claim_disabled,
        claim_many_limit: config.claim_many_limit,
    };

    Ok(resp)
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
