use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Attribute, Binary, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Order, Response, StdError, StdResult, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::state::{
    Config, CHECKPOINT_TOKEN, CLAIMED, CONFIG, OWNERSHIP_PROPOSAL, TIME_CURSOR_OF, TOKENS_PER_WEEK,
    VOTING_SUPPLY_PER_WEEK,
};
use crate::utils::{get_period, transfer_token_amount};
use astroport::asset::addr_validate_to_lower;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::querier::query_token_balance;
use astroport_governance::escrow_fee_distributor::{
    Claimed, ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, MAX_LIMIT_OF_CLAIM,
    TOKEN_CHECKPOINT_DEADLINE, WEEK,
};
use astroport_governance_voting::astro_voting_escrow::{
    LockInfoResponse, QueryMsg as VotingQueryMsg, VotingPowerResponse,
};
use cw20::Cw20ExecuteMsg;

use cw2::set_contract_version;
use cw_storage_plus::{Bound, Map, U64Key};

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

    let t = msg.start_time / WEEK * WEEK; // week alignment

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
            token_last_balance: Uint128::new(0),
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
            if config.is_killed {
                return Err(ContractError::ContractIsKilled {});
            }

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
            if config.is_killed {
                return Err(ContractError::ContractIsKilled {});
            }

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(|e| e.into())
        }
        ExecuteMsg::ClaimOwnership {} => {
            let config: Config = CONFIG.load(deps.storage)?;
            if config.is_killed {
                return Err(ContractError::ContractIsKilled {});
            }

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
        ExecuteMsg::KillMe {} => kill_me(deps, env, info),
        ExecuteMsg::RecoverBalance { token_address } => {
            recover_balance(deps.as_ref(), env, info, token_address)
        }
        ExecuteMsg::Claim { recipient } => claim(deps, env, info, recipient),
        ExecuteMsg::ClaimMany { receivers } => claim_many(deps, env, receivers),
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
fn checkpoint_total_supply(mut deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;
    calc_checkpoint_total_supply(deps.branch(), env, &mut config)?;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new())
}

fn calc_checkpoint_total_supply(mut deps: DepsMut, env: Env, config: &mut Config) -> StdResult<()> {
    let rounded_timestamp = env.block.time.seconds() / WEEK * WEEK; // week alignment
    let mut time_cursor = config.time_cursor;

    loop {
        if time_cursor > rounded_timestamp {
            break;
        } else {
            let total_voting_power_per_week: VotingPowerResponse = deps.querier.query_wasm_smart(
                &config.voting_escrow,
                &VotingQueryMsg::TotalVotingPowerAt { time: time_cursor },
            )?;

            let current_period = get_period(time_cursor);
            save_or_update_state_config(
                deps.branch(),
                &VOTING_SUPPLY_PER_WEEK,
                current_period,
                total_voting_power_per_week.voting_power,
            )?;
        }
        time_cursor += WEEK;
    }

    config.time_cursor = time_cursor;
    Ok(())
}
/// ## Description
/// Receive tokens into the contract and trigger a token checkpoint.
/// Returns the [`Response`] with the specified attributes if the operation was successful,
/// otherwise returns the [`ContractError`].
///
/// ## Params
/// * **token_address** is the object of type [`String`]. Address of the coin being received.
fn burn(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_address: String,
) -> Result<Response, ContractError> {
    let token_addr = addr_validate_to_lower(deps.api, &token_address)?;
    let mut config: Config = CONFIG.load(deps.storage)?;

    if config.is_killed {
        return Err(ContractError::ContractIsKilled {});
    }

    if token_addr != config.token {
        return Err(ContractError::TokenAddressIsWrong {});
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
fn kill_me(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
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

    CONFIG.save(deps.storage, &config)?;

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
/// * **token_address** Address of the coin being recover.
fn recover_balance(
    deps: Deps,
    env: Env,
    info: MessageInfo,
    token_address: String,
) -> Result<Response, ContractError> {
    let token_addr = addr_validate_to_lower(deps.api, &token_address)?;

    let config: Config = CONFIG.load(deps.storage)?;

    if config.is_killed {
        return Err(ContractError::ContractIsKilled {});
    }

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

    if config.is_killed {
        return Err(ContractError::ContractIsKilled {});
    }

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

fn save_or_update_state_config(
    deps: DepsMut,
    config: &Map<U64Key, Uint128>,
    week_cursor: u64,
    amount: Uint128,
) -> StdResult<()> {
    config.update(
        deps.storage,
        U64Key::from(week_cursor),
        |cursor| -> StdResult<_> {
            if let Some(cursor_value) = cursor {
                Ok(cursor_value + amount)
            } else {
                Ok(amount)
            }
        },
    )?;

    Ok(())
}

/// ## Description
/// Calculates the total number of tokens to be distributed in a given week.
fn calc_checkpoint_token(mut deps: DepsMut, env: Env, config: &mut Config) -> StdResult<()> {
    let distributor_balance = query_token_balance(
        &deps.querier,
        config.token.clone(),
        env.contract.address.clone(),
    )?;

    let to_distribute = distributor_balance.checked_sub(config.token_last_balance)?;
    let mut last_token_time = config.last_token_time;

    let since_last = env.block.time.seconds() - last_token_time;

    config.last_token_time = env.block.time.seconds();

    let mut current_week = last_token_time / WEEK * WEEK; // week alignment

    let mut actual_distribute_amount = Uint128::zero();
    loop {
        let next_week = current_week + WEEK;
        let current_period = get_period(current_week);
        let amount_per_week: Uint128;

        if env.block.time.seconds() < next_week {
            if since_last == 0 && env.block.time.seconds() == last_token_time {
                amount_per_week = to_distribute;
                actual_distribute_amount += to_distribute;
            } else {
                amount_per_week = to_distribute
                    .checked_mul(Uint128::from(env.block.time.seconds() - last_token_time))?
                    .checked_div(Uint128::from(since_last))?;

                actual_distribute_amount += amount_per_week;
            }

            save_or_update_state_config(
                deps.branch(),
                &TOKENS_PER_WEEK,
                current_period,
                amount_per_week,
            )?;
            break;
        } else if since_last == 0 && next_week == last_token_time {
            amount_per_week = to_distribute;
            actual_distribute_amount += amount_per_week;
        } else {
            amount_per_week = to_distribute
                .checked_mul(Uint128::from(next_week) - Uint128::from(last_token_time))?
                .checked_div(Uint128::from(since_last))?;
            actual_distribute_amount += amount_per_week;
        }

        save_or_update_state_config(
            deps.branch(),
            &TOKENS_PER_WEEK,
            current_period,
            amount_per_week,
        )?;

        last_token_time = next_week;
        current_week = next_week;
    }

    config.token_last_balance =
        distributor_balance.checked_sub(to_distribute.checked_sub(actual_distribute_amount)?)?;
    CHECKPOINT_TOKEN.save(
        deps.storage,
        U64Key::new(env.block.time.seconds()),
        &actual_distribute_amount,
    )?;

    Ok(())
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

    let mut config: Config = CONFIG.load(deps.storage)?;

    if config.is_killed {
        return Err(ContractError::ContractIsKilled {});
    }

    if env.block.time.seconds() >= config.time_cursor {
        calc_checkpoint_total_supply(deps.branch(), env.clone(), &mut config)?;
    }

    let mut last_token_time = config.last_token_time;

    if config.can_checkpoint_token
        && (env.block.time.seconds() > last_token_time + TOKEN_CHECKPOINT_DEADLINE)
    {
        calc_checkpoint_token(deps.branch(), env.clone(), &mut config)?;
        last_token_time = env.block.time.seconds();
    }

    last_token_time = last_token_time / WEEK * WEEK; // week alignment

    let claim_amount = calc_claim_amount(
        deps.branch(),
        config.clone(),
        recipient_addr.clone(),
        last_token_time,
    )?;

    let mut transfer_msg = vec![];
    if !claim_amount.is_zero() {
        transfer_msg =
            transfer_token_amount(config.token.clone(), recipient_addr.clone(), claim_amount)?;
        config.token_last_balance -= claim_amount;
    };

    CONFIG.save(deps.storage, &config)?;

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
    let mut config: Config = CONFIG.load(deps.storage)?;
    if config.is_killed {
        return Err(ContractError::ContractIsKilled {});
    }

    if env.block.time.seconds() >= config.time_cursor {
        calc_checkpoint_total_supply(deps.branch(), env.clone(), &mut config)?;
    }

    let mut last_token_time = config.last_token_time;

    if config.can_checkpoint_token
        && (env.block.time.seconds() > last_token_time + TOKEN_CHECKPOINT_DEADLINE)
    {
        calc_checkpoint_token(deps.branch(), env.clone(), &mut config)?;
        last_token_time = env.block.time.seconds();
    }

    last_token_time = last_token_time
        .checked_div(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?
        .checked_mul(WEEK)
        .ok_or_else(|| StdError::generic_err("Timestamp calculation error."))?;

    let mut total = Uint128::zero();
    let mut transfer_msg = vec![];

    for receiver in receivers {
        let receiver_addr = addr_validate_to_lower(deps.api, &receiver)?;
        let claim_amount = calc_claim_amount(
            deps.branch(),
            config.clone(),
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
        config.token_last_balance -= total;
    }

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
fn calc_claim_amount(
    deps: DepsMut,
    config: Config,
    addr: Addr,
    last_token_time: u64,
) -> StdResult<Uint128> {
    let user_lock_info: LockInfoResponse = deps.querier.query_wasm_smart(
        &config.voting_escrow,
        &VotingQueryMsg::LockInfo {
            user: addr.to_string(),
        },
    )?;

    if user_lock_info.end == 0 {
        // No lock = no fees
        return Ok(Uint128::zero());
    }

    let start_time = config.start_time;
    let mut week_cursor: u64;
    if let Some(w_cursor) = TIME_CURSOR_OF.may_load(deps.storage, addr.clone())? {
        week_cursor = w_cursor;
    } else {
        week_cursor = 0;
    }

    if week_cursor < start_time {
        week_cursor = start_time;
    }

    if week_cursor >= last_token_time {
        return Ok(Uint128::zero());
    }

    let mut to_distribute: Uint128 = Default::default();
    loop {
        if week_cursor >= last_token_time {
            break;
        }

        let current_period = get_period(week_cursor);
        if current_period >= user_lock_info.end {
            break;
        }

        let user_voting_power: VotingPowerResponse = deps.querier.query_wasm_smart(
            &config.voting_escrow,
            &VotingQueryMsg::UserVotingPowerAt {
                user: addr.to_string(),
                time: week_cursor,
            },
        )?;

        if user_voting_power.voting_power > Uint128::zero() {
            if let Some(voting_supply_per_week) = VOTING_SUPPLY_PER_WEEK
                .may_load(deps.as_ref().storage, U64Key::from(current_period))?
            {
                if let Some(tokens_per_week) =
                    TOKENS_PER_WEEK.may_load(deps.storage, U64Key::from(current_period))?
                {
                    to_distribute = to_distribute.checked_add(
                        user_voting_power
                            .voting_power
                            .checked_mul(tokens_per_week)?
                            .checked_div(voting_supply_per_week)?,
                    )?;
                }
            }
        }

        week_cursor += WEEK;
    }

    TIME_CURSOR_OF.save(deps.storage, addr.clone(), &week_cursor)?;

    CLAIMED.save(
        deps.storage,
        &vec![Claimed {
            recipient: addr,
            amount: to_distribute,
            claim_period: get_period(week_cursor),
            max_period: user_lock_info.end,
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

    if config.is_killed {
        return Err(ContractError::ContractIsKilled {});
    }

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
/// ## Queries
/// * **QueryMsg::Config {}** Returns the base controls configs that contains in the [`Config`] object.
///
/// * **QueryMsg::AstroRecipientsPerWeek {}** Returns the list of accounts who will get ASTRO fees
/// every week in the [`RecipientsPerWeekResponse`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::FetchUserBalanceByTimestamp { user, timestamp } => {
            Ok(to_binary(&query_user_balance(deps, env, user, timestamp)?)?)
        }
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps)?)?),
        QueryMsg::VotingSupplyPerWeek { start_after, limit } => Ok(to_binary(
            &query_voting_supply_per_week(deps, start_after, limit)?,
        )?),
        QueryMsg::FeeTokensPerWeek {} => Ok(to_binary(&query_tokens_per_week(deps)?)?),
    }
}

//settings for pagination
/// The maximum limit for reading pairs from a [`PAIRS`]
const MAX_LIMIT: u64 = 30;

/// The default limit for reading pairs from a [`PAIRS`]
const DEFAULT_LIMIT: u64 = 10;

fn query_voting_supply_per_week(
    deps: Deps,
    start_after: Option<u64>,
    limit: Option<u64>,
) -> StdResult<Vec<Uint128>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start;
    if let Some(start_after) = start_after {
        start = Some(Bound::Exclusive(U64Key::from(start_after).wrapped));
    } else {
        start = None;
    }

    Ok(VOTING_SUPPLY_PER_WEEK
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (_, voting_supply) = item.unwrap();
            voting_supply
        })
        .collect())
}

fn query_tokens_per_week(deps: Deps) -> StdResult<Vec<Uint128>> {
    let mut result: Vec<Uint128> = vec![];
    for x in TOKENS_PER_WEEK.keys(deps.storage, None, None, Order::Ascending) {
        let val = TOKENS_PER_WEEK.load(deps.storage, U64Key::from(x))?;
        result.push(val);
    }
    Ok(result)
}

/// ## Description
/// Returns the user fee amount by the timestamp
fn query_user_balance(deps: Deps, _env: Env, user: String, timestamp: u64) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let user_voting_power: VotingPowerResponse = deps.querier.query_wasm_smart(
        &config.voting_escrow,
        &VotingQueryMsg::UserVotingPowerAt {
            user,
            time: timestamp,
        },
    )?;

    let mut user_fee_balance = Uint128::zero();
    let current_period = get_period(timestamp);

    if !user_voting_power.voting_power.is_zero() {
        if let Some(voting_supply_per_week) =
            VOTING_SUPPLY_PER_WEEK.may_load(deps.storage, U64Key::from(current_period))?
        {
            if let Some(tokens_per_week) =
                TOKENS_PER_WEEK.may_load(deps.storage, U64Key::from(current_period))?
            {
                user_fee_balance = user_fee_balance.checked_add(
                    user_voting_power
                        .voting_power
                        .checked_mul(tokens_per_week)?
                        .checked_div(voting_supply_per_week)?,
                )?;
            }
        }
    }

    Ok(user_fee_balance)
}

/// ## Description
/// Returns information about the vesting configs in the [`ConfigResponse`] object.
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
/// Used for migration of contract. Returns the default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
