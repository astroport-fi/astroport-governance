use astroport::asset::addr_validate_to_lower;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order, Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_storage_plus::{Bound, U64Key};
use std::convert::TryInto;

use astroport_governance::astro_voting_escrow::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, LockInfoResponse, MigrateMsg, QueryMsg, UsersResponse,
    VotingPowerResponse,
};

use crate::error::ContractError;
use crate::state::{
    Config, Lock, Point, CONFIG, HISTORY, LAST_SLOPE_CHANGE, LOCKED, SLOPE_CHANGES,
};
use crate::utils::{
    calc_boost, calc_voting_power, fetch_last_checkpoint, get_period, time_limits_check,
    xastro_token_check,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astro-voting-escrow";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const WEEK: u64 = 7 * 86400; // lock period is rounded down by week
pub const MAX_LOCK_TIME: u64 = 2 * 365 * 86400; // 2 years (104 weeks)

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        xastro_token_addr: addr_validate_to_lower(deps.api, &msg.deposit_token_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    let cur_period = get_period(env.block.time.seconds());
    let point = Point {
        power: Uint128::zero(),
        start: cur_period,
        end: 0,
        slope: Decimal::zero(),
    };
    HISTORY.save(
        deps.storage,
        (env.contract.address, U64Key::new(cur_period)),
        &point,
    )?;

    Ok(Response::default())
}

/// ## Description
/// Available the execute messages of the contract.
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
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ExtendLockTime { time } => extend_lock_time(deps, env, info, time),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Withdraw {} => withdraw(deps, env, info),
    }
}

fn checkpoint_total(
    deps: DepsMut,
    env: Env,
    add_voting_power: Option<Uint128>,
    old_slope: Decimal,
    new_slope: Decimal,
    new_end: Option<u64>,
) -> StdResult<()> {
    let cur_period = get_period(env.block.time.seconds());
    let cur_period_key = U64Key::new(cur_period);
    let contract_addr = env.contract.address;
    let add_voting_power = add_voting_power.unwrap_or_default();

    // get last checkpoint
    let last_checkpoint = fetch_last_checkpoint(deps.as_ref(), &contract_addr, &cur_period_key)?;
    let new_point = if let Some((_, point)) = last_checkpoint {
        let end = new_end.unwrap_or(cur_period);
        let scheduled_change_opt = SLOPE_CHANGES.may_load(deps.storage, cur_period_key.clone())?;
        let scheduled_change = if let Some(change) = scheduled_change_opt {
            let last_slope_change = LAST_SLOPE_CHANGE
                .may_load(deps.as_ref().storage)?
                .unwrap_or(0);
            if last_slope_change < cur_period {
                LAST_SLOPE_CHANGE.save(deps.storage, &cur_period)?;
                change
            } else {
                Decimal::zero()
            }
        } else {
            Decimal::zero()
        };

        Point {
            power: calc_voting_power(&point, cur_period) + add_voting_power,
            slope: point.slope - old_slope + new_slope - scheduled_change,
            start: cur_period,
            end,
        }
    } else {
        // this error can't happen since this if-branch is intended for checkpoint creation
        let end =
            new_end.ok_or_else(|| StdError::generic_err("Checkpoint initialization error"))?;
        Point {
            power: add_voting_power,
            slope: new_slope,
            start: cur_period,
            end,
        }
    };
    HISTORY.save(deps.storage, (contract_addr, cur_period_key), &new_point)
}

fn checkpoint(
    deps: DepsMut,
    env: Env,
    addr: Addr,
    add_amount: Option<Uint128>,
    new_end: Option<u64>,
) -> StdResult<()> {
    let cur_period = get_period(env.block.time.seconds());
    let cur_period_key = U64Key::new(cur_period);
    let add_amount = add_amount.unwrap_or_default();
    let mut old_slope = Decimal::zero();
    let mut add_voting_power = Uint128::zero();

    // get last checkpoint
    let last_checkpoint = fetch_last_checkpoint(deps.as_ref(), &addr, &cur_period_key)?;
    let new_point = if let Some((_, point)) = last_checkpoint {
        let end = new_end.unwrap_or(point.end);
        let dt = end.saturating_sub(cur_period);
        let current_power = calc_voting_power(&point, cur_period);
        let new_slope = if dt != 0 {
            if end > point.end && add_amount.is_zero() {
                // this is extend_lock_time
                Decimal::from_ratio(current_power, dt)
            } else {
                // increase lock's amount or lock creation after withdrawal
                add_voting_power = add_amount * calc_boost(dt);
                Decimal::from_ratio(current_power + add_voting_power, dt)
            }
        } else {
            Decimal::zero()
        };

        // cancel previously scheduled slope change
        let end_period_key = U64Key::new(point.end);
        match SLOPE_CHANGES.may_load(deps.as_ref().storage, end_period_key.clone())? {
            // we do not need to schedule slope change in the past
            Some(old_scheduled_change) if point.end >= cur_period => SLOPE_CHANGES.save(
                deps.storage,
                end_period_key,
                &(old_scheduled_change - point.slope),
            )?,
            _ => (),
        }

        // we need to subtract it from total VP slope
        old_slope = point.slope;

        Point {
            power: current_power + add_voting_power,
            slope: new_slope,
            start: cur_period,
            end,
        }
    } else {
        // this error can't happen since this if-branch is intended for checkpoint creation
        let end =
            new_end.ok_or_else(|| StdError::generic_err("Checkpoint initialization error"))?;
        let dt = end - cur_period;
        add_voting_power = add_amount * calc_boost(dt);
        let slope = Decimal::from_ratio(add_voting_power, dt);
        Point {
            power: add_voting_power,
            slope,
            start: cur_period,
            end,
        }
    };

    // schedule slope change
    SLOPE_CHANGES.update(
        deps.storage,
        U64Key::new(new_point.end),
        |slope_opt| -> StdResult<Decimal> {
            if let Some(pslope) = slope_opt {
                Ok(pslope + new_point.slope)
            } else {
                Ok(new_point.slope)
            }
        },
    )?;

    HISTORY.save(deps.storage, (addr, cur_period_key), &new_point)?;
    checkpoint_total(
        deps,
        env,
        Some(add_voting_power),
        old_slope,
        new_point.slope,
        new_end,
    )
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise returns the [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **cw20_msg** is the object of type [`Cw20ReceiveMsg`].
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::CreateLock { time } => create_lock(deps, env, info, cw20_msg, time),
        Cw20HookMsg::ExtendLockAmount {} => extend_lock_amount(deps, env, info, cw20_msg),
    }
}

fn create_lock(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
    time: u64,
) -> Result<Response, ContractError> {
    xastro_token_check(deps.as_ref(), info.sender)?;
    time_limits_check(time)?;
    let amount = cw20_msg.amount;
    let user = addr_validate_to_lower(deps.as_ref().api, &cw20_msg.sender)?;
    let block_period = get_period(env.block.time.seconds());
    let end = block_period + get_period(time);

    LOCKED.update(deps.storage, user.clone(), |lock_opt| {
        if lock_opt.is_some() {
            return Err(ContractError::LockAlreadyExists {});
        }
        Ok(Lock {
            amount,
            start: block_period,
            end,
        })
    })?;

    checkpoint(deps, env, user, Some(amount), Some(end))?;

    Ok(Response::default().add_attribute("action", "create_lock"))
}

fn extend_lock_amount(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    xastro_token_check(deps.as_ref(), info.sender)?;
    let amount = cw20_msg.amount;
    let user = addr_validate_to_lower(deps.as_ref().api, &cw20_msg.sender)?;
    LOCKED.update(deps.storage, user.clone(), |lock_opt| {
        if let Some(mut lock) = lock_opt {
            if lock.end <= get_period(env.block.time.seconds()) {
                Err(ContractError::LockExpired {})
            } else {
                lock.amount += amount;
                Ok(lock)
            }
        } else {
            Err(ContractError::LockDoesntExist {})
        }
    })?;
    checkpoint(deps, env, user, Some(amount), None)?;

    Ok(Response::default().add_attribute("action", "extend_lock_amount"))
}

fn withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let sender = info.sender;
    let lock = LOCKED
        .may_load(deps.storage, sender.clone())?
        .ok_or(ContractError::LockDoesntExist {})?;

    if lock.end > get_period(env.block.time.seconds()) {
        Err(ContractError::LockHasNotExpired {})
    } else {
        let config = CONFIG.load(deps.storage)?;
        let transfer_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.xastro_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: sender.to_string(),
                amount: lock.amount,
            })?,
            funds: vec![],
        });
        LOCKED.remove(deps.storage, sender);

        Ok(Response::default()
            .add_message(transfer_msg)
            .add_attribute("action", "withdraw"))
    }
}

fn extend_lock_time(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    time: u64,
) -> Result<Response, ContractError> {
    let user = info.sender;
    let mut lock = LOCKED
        .load(deps.storage, user.clone())
        .map_err(|_| ContractError::LockDoesntExist {})?;

    // disabling ability to extend lock time by less than a week
    time_limits_check(time)?;

    if lock.end <= get_period(env.block.time.seconds()) {
        return Err(ContractError::LockExpired {});
    };

    let block_time = env.block.time.seconds();
    // should not exceed MAX_LOCK_TIME
    time_limits_check(lock.end * WEEK + time - block_time)?;
    lock.end = get_period(block_time) + get_period(time);
    LOCKED.save(deps.storage, user.clone(), &lock)?;

    checkpoint(deps, env, user, None, Some(lock.end))?;

    Ok(Response::default().add_attribute("action", "extend_lock_time"))
}

/// # Description
/// Describes all query messages.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`QueryMsg`].
///
/// ## Queries
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::TotalVotingPower {} => to_binary(&get_total_voting_power(deps, env, None)?),
        QueryMsg::UserVotingPower { user } => {
            to_binary(&get_user_voting_power(deps, env, user, None)?)
        }
        QueryMsg::TotalVotingPowerAt { time } => {
            to_binary(&get_total_voting_power(deps, env, Some(time))?)
        }
        QueryMsg::UserVotingPowerAt { user, time } => {
            to_binary(&get_user_voting_power(deps, env, user, Some(time))?)
        }
        QueryMsg::Users {} => get_all_users(deps),
        QueryMsg::LockInfo { user } => to_binary(&get_user_lock_info(deps, user)?),
    }
}

fn get_user_lock_info(deps: Deps, user: String) -> StdResult<LockInfoResponse> {
    let addr = addr_validate_to_lower(deps.api, &user)?;
    if let Some(lock) = LOCKED.may_load(deps.storage, addr)? {
        let resp = LockInfoResponse {
            amount: lock.amount,
            boost: calc_boost(lock.end - lock.start),
            start: lock.start,
            end: lock.end,
        };
        Ok(resp)
    } else {
        Err(StdError::generic_err("User is not found"))
    }
}

fn get_user_voting_power(
    deps: Deps,
    env: Env,
    user: String,
    time: Option<u64>,
) -> StdResult<VotingPowerResponse> {
    let user = addr_validate_to_lower(deps.api, &user)?;
    let period = get_period(time.unwrap_or_else(|| env.block.time.seconds()));
    let period_key = U64Key::new(period);

    let last_checkpoint = fetch_last_checkpoint(deps, &user, &period_key)?;

    let (_, point) = last_checkpoint.ok_or_else(|| StdError::generic_err("User is not found"))?;

    // the point right in this period was found
    let voting_power = if point.start == period {
        point.power
    } else {
        // the point before this period was found thus we can calculate VP in the period
        // we are interested in
        calc_voting_power(&point, period)
    };

    Ok(VotingPowerResponse { voting_power })
}

fn get_total_voting_power(
    deps: Deps,
    env: Env,
    time: Option<u64>,
) -> StdResult<VotingPowerResponse> {
    let contract_addr = env.contract.address.clone();
    let period = get_period(time.unwrap_or_else(|| env.block.time.seconds()));
    let period_key = U64Key::new(period);

    let last_checkpoint = fetch_last_checkpoint(deps, &contract_addr, &period_key)?;

    let point = last_checkpoint.map_or(
        Point {
            power: Uint128::zero(),
            start: period,
            end: period,
            slope: Decimal::zero(),
        },
        |(_, point)| point,
    );

    let voting_power = if point.start == period {
        point.power
    } else {
        let checkpoint_period_key = U64Key::new(point.start);
        let scheduled_slope_changes: Vec<_> = SLOPE_CHANGES
            .range(
                deps.storage,
                Some(Bound::Exclusive(checkpoint_period_key.wrapped)),
                Some(Bound::Inclusive(period_key.wrapped)),
                Order::Ascending,
            )
            .filter_map(|item| {
                let (period_serialized, lock) = item.ok()?;
                let period_bytes: [u8; 8] = period_serialized.try_into().unwrap();
                Some((u64::from_be_bytes(period_bytes), lock))
            })
            .collect();
        let mut init_point = point;
        for (recalc_period, scheduled_change) in scheduled_slope_changes {
            init_point = Point {
                power: calc_voting_power(&init_point, recalc_period),
                start: recalc_period,
                slope: init_point.slope - scheduled_change,
                ..init_point
            }
        }
        calc_voting_power(&init_point, period)
    };

    Ok(VotingPowerResponse { voting_power })
}

fn get_all_users(deps: Deps) -> StdResult<Binary> {
    // TODO: change to *at behavior bc we need to know all locked users in particular period
    let keys: Vec<_> = LOCKED
        .keys(deps.storage, None, None, Order::Ascending)
        .filter_map(|key| String::from_utf8(key).map_err(StdError::from).ok())
        .collect();
    to_binary(&UsersResponse { users: keys })
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is the object of type [`Deps`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_msg** is the object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
