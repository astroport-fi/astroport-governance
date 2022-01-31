use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Attribute, Binary, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};
use std::cmp::{max, min};

use crate::error::ContractError;
use crate::state::{
    Config, DistributorInfo, CHECKPOINT_TOKEN, CLAIMED, CONFIG, DISTRIBUTOR_INFO,
    OWNERSHIP_PROPOSAL,
};
use crate::utils::{find_timestamp_period, find_timestamp_user_period, transfer_token_amount};
use astroport::asset::addr_validate_to_lower;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::querier::query_token_balance;
use astroport_governance::escrow_fee_distributor::{
    Claimed, ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, Point, QueryMsg,
    RecipientsPerWeekResponse,
};
//use astroport_governance_voting::astro_voting_escrow::QueryMsg as VotingQueryMsg;
use cw20::Cw20ExecuteMsg;

use cw2::set_contract_version;
use cw_storage_plus::U64Key;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-escrow-fee-distributor";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const WEEK: u64 = 7 * 86400;
const TOKEN_CHECKPOINT_DEADLINE: u64 = 86400;
const MAX_LIMIT_OF_CLAIM: u64 = 10;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default [`Response`] object if the operation was successful, otherwise returns
/// the [`StdResult`] if the contract was not created.
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for
/// creating a contract
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let t = msg
        .start_time
        .checked_div(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?
        .checked_mul(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: addr_validate_to_lower(deps.api, &msg.owner)?,
            token: addr_validate_to_lower(deps.api, &msg.token)?,
            voting_escrow: addr_validate_to_lower(deps.api, &msg.voting_escrow)?,
            emergency_return: addr_validate_to_lower(deps.api, &msg.emergency_return)?,
            start_time: t,
            last_token_time: t,
            time_cursor: t,
            can_checkpoint_token: false,
            is_killed: false,
            max_limit_accounts_of_claim: MAX_LIMIT_OF_CLAIM,
        },
    )?;

    Ok(Response::new())
}

/// ## Description
/// Available the execute messages of the contract.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a request to change ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Approves ownership.
///
/// * **ExecuteMsg::CheckpointTotalSupply {}** Update the vxAstro total supply checkpoint.
///
/// * **ExecuteMsg::Burn { token_address }** Receive tokens into the contract and trigger a token
/// checkpoint.
///
/// * **ExecuteMsg::KillMe {}** Kill the contract. Killing transfers the entire token balance to
/// the emergency return address and blocks the ability to claim or burn. The contract cannot be
/// unkilled.
///
/// * **ExecuteMsg::RecoverBalance { token_address }** Recover tokens from this contract,
/// tokens are sent to the emergency return address.
///
/// * **ExecuteMsg::ToggleAllowCheckpointToken {}** Enables or disables the ability to set
/// a checkpoint token.
///
/// * **ExecuteMsg::Claim { recipient }** Claims the tokens from distributor for transfer
/// to the recipient.
///
/// * **ExecuteMsg::ClaimMany { receivers }**  Make multiple fee claims in a single call.
///
/// * **ExecuteMsg::CheckpointToken {}** Calculates the total number of tokens to be distributed
/// in a given week.
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config: Config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(|e| e.into())
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

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
        ExecuteMsg::CheckpointTotalSupply {} => checkpoint_total_supply(deps, env),
        ExecuteMsg::Burn { token_address } => burn(deps, env, info, token_address),
        ExecuteMsg::KillMe {} => kill_me(deps.as_ref(), env, info),
        ExecuteMsg::RecoverBalance { token_address } => {
            recover_balance(deps.as_ref(), env, info, token_address)
        }
        ExecuteMsg::Claim { recipient } => claim(deps, env, info, recipient),
        ExecuteMsg::ClaimMany { receivers } => claim_many(deps, env, info, receivers),
        ExecuteMsg::CheckpointToken {} => checkpoint_token(deps, env, info),
        ExecuteMsg::UpdateConfig {
            max_limit_accounts_of_claim,
            can_checkpoint_token,
        } => update_config(
            deps,
            info,
            max_limit_accounts_of_claim,
            can_checkpoint_token,
        ),
    }
}

/// ## Description
/// Update the vxAstro total supply checkpoint. The checkpoint is also updated by the first
/// claimant each new period week. This function may be called independently of a claim,
/// to reduce claiming gas costs. Returns the [`Response`] with the specified attributes if the
/// operation was successful, otherwise returns the [`ContractError`].
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **env** is the object of type [`Env`].
///
fn checkpoint_total_supply(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let rounded_timestamp = env
        .block
        .time
        .seconds()
        .checked_div(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?
        .checked_mul(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?;
    let mut t = config.time_cursor;
    // TODO: execute VotingEscrow.checkpoint()

    let mut distributor_info = DISTRIBUTOR_INFO.load(deps.storage)?;

    for _i in 1..20 {
        if t > rounded_timestamp {
            break;
        } else {
            let _period = find_timestamp_period(deps.as_ref(), config.voting_escrow.clone(), t)?;

            let pt = Point::default(); // TODO: query from VotingEscrow
                                       // let pt: Point = deps.querier.query_wasm_smart(
                                       //     &config.voting_escrow,
                                       //     &VotingQueryMsg::PointHistory { period },
                                       // )?;

            let dt: u64;
            if t > pt.ts {
                dt = t - pt.ts;
            } else {
                dt = 0;
            }

            *distributor_info
                .voting_supply_per_week
                .entry(t)
                .or_insert_with(|| Uint128::new(0)) += Uint128::from(max(
                pt.bias
                    - pt.slope
                        .checked_mul(dt as i128)
                        .ok_or_else(|| StdError::generic_err("Math operation error."))?,
                0,
            ) as u128);
        }
        t += WEEK;
    }

    config.time_cursor = t;
    CONFIG.save(deps.storage, &config)?;
    DISTRIBUTOR_INFO.save(deps.storage, &distributor_info)?;

    Ok(Response::new())
}

/// ## Description
/// Receive tokens into the contract and trigger a token checkpoint.
/// Returns the [`Response`] with the specified attributes if the operation was successful,
/// otherwise returns the [`ContractError`].
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **env** is the object of type [`Env`].
///
/// * **token_address** is the object of type [`String`]. Address of the coin being received.
///
fn burn(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_address: String,
) -> Result<Response, ContractError> {
    let token_addr = addr_validate_to_lower(deps.api, &token_address)?;
    let mut config: Config = CONFIG.load(deps.storage)?;

    if token_addr != config.token {
        return Err(ContractError::TokenAddressIsWrong {});
    }

    if config.is_killed {
        return Err(ContractError::ContractIsKilled {});
    }

    let balance = query_token_balance(&deps.querier, token_addr.clone(), info.sender.clone())?;

    let messages: Vec<CosmosMsg> = vec![CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
            owner: info.sender.to_string(),
            recipient: env.contract.address.to_string(),
            amount: balance,
        })?,
        funds: vec![],
    })];

    if config.can_checkpoint_token
        && (env.block.time.seconds() > config.last_token_time + TOKEN_CHECKPOINT_DEADLINE)
    {
        calc_checkpoint_token(deps.branch(), env, &mut config)?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attributes(vec![
            attr("action", "burn"),
            attr("amount", balance.to_string()),
        ])
        .add_messages(messages))
}

/// ## Description
/// Kill the contract. Killing transfers the entire token balance to the emergency return address
/// and blocks the ability to claim or burn. The contract cannot be unkilled.
/// Returns the [`Response`] with the specified attributes if the operation was successful,
/// otherwise returns the [`ContractError`].
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **env** is the object of type [`Env`].
///
fn kill_me(deps: Deps, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    config.is_killed = true;

    let current_balance =
        query_token_balance(&deps.querier, config.token.clone(), env.contract.address)?;

    let transfer_msg = transfer_token_amount(
        config.token.clone(),
        config.emergency_return.clone(),
        current_balance,
    )?;

    Ok(Response::new()
        .add_attributes(vec![
            attr("action", "kill_me"),
            attr("transferred_balance", current_balance.to_string()),
            attr("recipient", config.emergency_return.to_string()),
        ])
        .add_messages(transfer_msg))
}

/// ## Description
/// Recover tokens from this contract, tokens are sent to the emergency return address.
/// Returns the [`Response`] with the specified attributes if the operation was successful,
/// otherwise returns the [`ContractError`].
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **env** is the object of type [`Env`].
///
/// * **token_address** is the object of type [`String`].
///
fn recover_balance(
    deps: Deps,
    env: Env,
    info: MessageInfo,
    token_address: String,
) -> Result<Response, ContractError> {
    let token_addr = addr_validate_to_lower(deps.api, &token_address)?;

    let config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if token_addr != config.token {
        return Err(ContractError::TokenAddressIsWrong {});
    }

    let current_balance =
        query_token_balance(&deps.querier, token_addr.clone(), env.contract.address)?;
    let transfer_msg =
        transfer_token_amount(token_addr, config.emergency_return.clone(), current_balance)?;

    Ok(Response::new()
        .add_attributes(vec![
            attr("action", "recover_balance"),
            attr("balance", current_balance.to_string()),
            attr("recipient", config.emergency_return.to_string()),
        ])
        .add_messages(transfer_msg))
}

/// ## Description
/// Update the token checkpoint. Returns the [`Response`] with the specified attributes if the
/// operation was successful, otherwise returns the [`ContractError`].
fn checkpoint_token(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner
        && (!config.can_checkpoint_token
            || env.block.time.seconds() < (config.last_token_time + TOKEN_CHECKPOINT_DEADLINE))
    {
        return Err(ContractError::CheckpointTokenIsNotAvailable {});
    }

    calc_checkpoint_token(deps.branch(), env, &mut config)?;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "checkpoint_token")]))
}

/// ## Description
/// Calculates the total number of tokens to be distributed in a given week.
fn calc_checkpoint_token(deps: DepsMut, env: Env, config: &mut Config) -> StdResult<()> {
    let mut distributor_info: DistributorInfo = DISTRIBUTOR_INFO.load(deps.storage)?;

    let distributor_balance =
        query_token_balance(&deps.querier, config.token.clone(), env.contract.address)?;
    let to_distribute = distributor_balance.checked_sub(distributor_info.token_last_balance)?;

    distributor_info.token_last_balance = distributor_balance;
    let mut last_token_time = config.last_token_time;

    let since_last = env
        .block
        .time
        .seconds()
        .checked_sub(last_token_time)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?;

    config.last_token_time = env.block.time.seconds();

    let mut current_week = last_token_time
        .checked_div(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?
        .checked_mul(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?;

    for _i in 1..20 {
        let next_week = current_week + WEEK;
        if env.block.time.seconds() < next_week {
            if since_last == 0 && env.block.time.seconds() == last_token_time {
                *distributor_info
                    .tokens_per_week
                    .entry(current_week)
                    .or_insert_with(|| Uint128::new(0)) += to_distribute;
            } else {
                *distributor_info
                    .tokens_per_week
                    .entry(current_week)
                    .or_insert_with(|| Uint128::new(0)) += to_distribute
                    .checked_mul(
                        Uint128::from(env.block.time.seconds()) - Uint128::from(last_token_time),
                    )?
                    .checked_div(Uint128::from(since_last))?;
            }
        } else if since_last == 0 && next_week == last_token_time {
            *distributor_info
                .tokens_per_week
                .entry(current_week)
                .or_insert_with(|| Uint128::new(0)) += to_distribute;
        } else {
            *distributor_info
                .tokens_per_week
                .entry(current_week)
                .or_insert_with(|| Uint128::new(0)) += to_distribute
                .checked_mul(Uint128::from(next_week) - Uint128::from(last_token_time))?
                .checked_div(Uint128::from(since_last))?;
        }

        last_token_time = next_week;
        current_week = next_week;
    }

    DISTRIBUTOR_INFO.save(deps.storage, &distributor_info)?;
    CHECKPOINT_TOKEN.save(
        deps.storage,
        U64Key::new(env.block.time.seconds()),
        &to_distribute,
    )?;

    Ok(())
}

/// ## Description
/// Claims the amount from FeeDistributor for transfer to the recipient. Returns the [`Response`] with
/// specified attributes if operation was successful, otherwise returns the [`ContractError`].
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **recipient** is an [`Option`] field of type [`String`]. Sets the recipient for claim.
///
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
    if config.is_killed {
        return Err(ContractError::ContractIsKilled {});
    }

    if env.block.time.seconds() >= config.time_cursor {
        checkpoint_total_supply(deps.branch(), env.clone())?;
    }

    let mut last_token_time = config.last_token_time;

    if config.can_checkpoint_token
        && (env.block.time.seconds() > last_token_time + TOKEN_CHECKPOINT_DEADLINE)
    {
        checkpoint_token(deps.branch(), env.clone(), info)?;
        last_token_time = env.block.time.seconds();
    }

    last_token_time = last_token_time
        .checked_div(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?
        .checked_mul(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?;

    let mut distributor_info: DistributorInfo = DISTRIBUTOR_INFO.load(deps.storage)?;
    let claim_amount = calc_claim_amount(
        deps.branch(),
        config.clone(),
        &mut distributor_info,
        recipient_addr.clone(),
        last_token_time,
    )?;

    let mut transfer_msg = vec![];
    if !claim_amount.is_zero() {
        transfer_msg = transfer_token_amount(config.token, recipient_addr.clone(), claim_amount)?;
        distributor_info.token_last_balance -= claim_amount;
    };

    DISTRIBUTOR_INFO.save(deps.storage, &distributor_info)?;

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
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **receivers** is vector field of type [`String`]. Sets the receivers for claim.
///
fn claim_many(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    receivers: Vec<String>,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    if config.is_killed {
        return Err(ContractError::ContractIsKilled {});
    }

    if env.block.time.seconds() >= config.time_cursor {
        checkpoint_total_supply(deps.branch(), env.clone())?;
    }

    let mut last_token_time = config.last_token_time;

    if config.can_checkpoint_token
        && (env.block.time.seconds() > last_token_time + TOKEN_CHECKPOINT_DEADLINE)
    {
        checkpoint_token(deps.branch(), env.clone(), info)?;
        last_token_time = env.block.time.seconds();
    }

    last_token_time = last_token_time
        .checked_div(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?
        .checked_mul(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?;

    let mut total = Uint128::zero();
    let mut transfer_msg = vec![];

    let mut distributor_info: DistributorInfo = DISTRIBUTOR_INFO.load(deps.storage)?;

    for receiver in receivers {
        let receiver_addr = addr_validate_to_lower(deps.api, &receiver)?;
        let claim_amount = calc_claim_amount(
            deps.branch(),
            config.clone(),
            &mut distributor_info,
            receiver_addr.clone(),
            last_token_time,
        )?;

        if !claim_amount.is_zero() {
            transfer_msg.extend(transfer_token_amount(
                config.token.clone(),
                receiver_addr,
                claim_amount,
            )?);
            total += claim_amount;
        };
    }

    if !total.is_zero() {
        distributor_info.token_last_balance -= total;
    }

    DISTRIBUTOR_INFO.save(deps.storage, &distributor_info)?;

    let response = Response::new()
        .add_attributes(vec![
            attr("action", "claim_many"),
            attr("amount", total.to_string()),
        ])
        .add_messages(transfer_msg);

    Ok(response)
}

/// ## Description
/// Calculation amount of claim.
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **config** is the object of type [`Config`].
///
/// * **distributor_info** is the object of type [`DistributorInfo`].
///
/// * **addr** is the object of type [`Addr`].
///
/// * **last_token_time** is the object of type [`u64`].
///
fn calc_claim_amount(
    deps: DepsMut,
    config: Config,
    distributor_info: &mut DistributorInfo,
    addr: Addr,
    last_token_time: u64,
) -> StdResult<Uint128> {
    // Minimal user period is 0 (if user had no point)
    let mut user_period: u64;
    let mut to_distribute = Uint128::zero();

    let max_user_period: u64 = 10; // TODO get from VotingEscrow(ve).user_point_epoch(addr)
    let start_time = config.start_time;

    if max_user_period == 0 {
        // No lock = no fees
        return Ok(Uint128::zero());
    }

    let mut week_cursor: u64;
    if let Some(w_cursor) = distributor_info.time_cursor_of.get(&addr) {
        week_cursor = *w_cursor;
    } else {
        week_cursor = 0;
    }

    if week_cursor == 0 {
        user_period = find_timestamp_user_period(
            deps.as_ref(),
            config.voting_escrow,
            addr.clone(),
            start_time,
            max_user_period,
        )?;
    } else if let Some(period) = distributor_info.user_period_of.get(&addr) {
        user_period = *period;
        if user_period == 0 {
            user_period = 1;
        }
    } else {
        user_period = 1;
    }

    let mut user_point = Point::default(); // TODO:
                                           // let user_point: Point = deps.querier.query_wasm_smart(
                                           //     &config.voting_escrow,
                                           //     &VotingQueryMsg::UserPointHistory { addr, user_period },
                                           // )?;

    if week_cursor == 0 {
        week_cursor = (user_point.ts + WEEK - 1)
            .checked_div(WEEK * WEEK)
            .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?;
    }

    if week_cursor >= last_token_time {
        return Ok(Uint128::zero());
    }

    if week_cursor < start_time {
        week_cursor = start_time;
    }

    let mut old_user_point = Point::default();

    // iterate over weeks
    for _i in 1..50 {
        if week_cursor >= last_token_time {
            break;
        }

        if week_cursor >= user_point.ts && user_period <= max_user_period {
            user_period += 1;
            old_user_point = user_point.clone();

            if user_period > max_user_period {
                user_point = Point::default();
            } else {
                user_point = Point {
                    bias: 0,
                    slope: 0,
                    ts: 0,
                    blk: 1,
                }; // TODO: // let user_point: Point = deps.querier.query_wasm_smart(
                   //     &config.voting_escrow,
                   //     &VotingQueryMsg::UserPointHistory { addr, user_period },
                   // )?;
            }
        } else {
            // Calc
            // + i * 2 is for rounding errors
            let dt = week_cursor
                .checked_sub(old_user_point.ts)
                .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?;
            let balance_of = max(old_user_point.bias - dt as i128 * old_user_point.slope, 0);

            if balance_of == 0 && user_period > max_user_period {
                break;
            }

            if balance_of > 0 {
                if let Some(voting_supply_per_week) =
                    distributor_info.voting_supply_per_week.get(&week_cursor)
                {
                    if let Some(tokens_per_week) =
                        distributor_info.tokens_per_week.get(&week_cursor)
                    {
                        to_distribute += Uint128::from(balance_of as u128)
                            .checked_mul(*tokens_per_week)?
                            .checked_div(*voting_supply_per_week)?;
                    }
                }
            }

            week_cursor += WEEK;
        }
    }

    user_period = min(max_user_period, user_period - 1);

    *distributor_info
        .user_period_of
        .entry(addr.clone())
        .or_insert_with(|| 0) = user_period;
    *distributor_info
        .time_cursor_of
        .entry(addr.clone())
        .or_insert_with(|| 0) = week_cursor;

    CLAIMED.save(
        deps.storage,
        &vec![Claimed {
            recipient: addr,
            amount: to_distribute,
            claim_period: user_period,
            max_period: max_user_period,
        }],
    )?;

    Ok(to_distribute)
}

/// ## Description
/// Updates general settings. Returns an [`ContractError`] on failure or the following [`Config`]
/// data will be updated if successful.
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    max_limit_accounts_of_claim: Option<u64>,
    can_checkpoint_token: Option<bool>,
) -> Result<Response, ContractError> {
    let mut attributes = vec![attr("action", "set_config")];
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(can_checkpoint_token) = can_checkpoint_token {
        config.can_checkpoint_token = can_checkpoint_token;
        attributes.push(Attribute::new(
            "can_checkpoint_token",
            can_checkpoint_token.to_string(),
        ));
    };

    if let Some(max_limit_accounts_of_claim) = max_limit_accounts_of_claim {
        config.max_limit_accounts_of_claim = max_limit_accounts_of_claim;
        attributes.push(Attribute::new(
            "max_limit_accounts_of_claim",
            max_limit_accounts_of_claim.to_string(),
        ));
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

/// ## Description
/// Available the query messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the base controls configs that contains in the [`Config`] object.
///
/// * **QueryMsg::AstroRecipientsPerWeek {}** Returns the list of accounts who will get ASTRO fees
/// every week in the [`RecipientsPerWeekResponse`] object.
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::FetchUserBalanceByTimestamp { user, timestamp } => {
            Ok(to_binary(&query_user_balance(deps, env, user, timestamp)?)?)
        }
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps)?)?),
        QueryMsg::AstroRecipientsPerWeek {} => Ok(to_binary(&query_recipients_per_weeks(deps)?)?),
    }
}

/// ## Description
/// Returns information about the vesting configs in the [`ConfigResponse`] object.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
fn query_user_balance(deps: Deps, _env: Env, user: String, timestamp: u64) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let user_addr = addr_validate_to_lower(deps.api, &user)?;
    let max_user_epoch: u64 = 100; // TODO: use query below when it will be created
                                   // let max_user_epoch = deps.querier.query_wasm_smart(
                                   //     &config.voting_escrow,
                                   //     &VotingQueryMsg::UserPointEpoch {
                                   //         user: user.to_string(),
                                   //     },
                                   // )?;

    let _epoch = find_timestamp_user_period(
        deps,
        config.voting_escrow,
        user_addr,
        timestamp,
        max_user_epoch,
    )?;

    let pt = Point::default(); // TODO: use query below when it will be created
                               // let pt: Point = deps.querier.query_wasm_smart(
                               //     &voting_escrow,
                               //     &VotingQueryMsg::UserPointHistory {
                               //         user: user.to_string(),
                               //         mid: epoch,
                               //     },
                               // )?;

    let resp = max(
        pt.bias
            - pt.slope
                .checked_mul((timestamp - pt.ts) as i128)
                .ok_or_else(|| StdError::generic_err("Calculation error."))?,
        0,
    );

    Ok(Uint128::from(resp as u128))
}

/// ## Description
/// Returns information about the vesting configs in the [`ConfigResponse`] object.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;

    let resp = ConfigResponse {
        owner: config.owner,
        token: config.token,
        voting_escrow: config.voting_escrow,
        emergency_return: config.emergency_return,
        start_time: config.start_time,
        last_token_time: config.last_token_time,
        time_cursor: config.time_cursor,
        can_checkpoint_token: config.can_checkpoint_token,
        is_killed: config.is_killed,
        max_limit_accounts_of_claim: config.max_limit_accounts_of_claim,
    };

    Ok(resp)
}

/// ## Description
/// Returns the list of accounts who will get ASTRO fees every week in the
/// [`RecipientsPerWeekResponse`] object.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
pub fn query_recipients_per_weeks(_deps: Deps) -> StdResult<RecipientsPerWeekResponse> {
    Ok(RecipientsPerWeekResponse { recipients: vec![] })
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_msg** is the object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
