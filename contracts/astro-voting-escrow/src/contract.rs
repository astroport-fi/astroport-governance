use astroport::asset::addr_validate_to_lower;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order,
    Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_storage_plus::{Bound, U64Key};
use std::convert::TryInto;

use astroport_governance::astro_voting_escrow::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, UsersResponse,
    VotingPowerResponse,
};

use crate::error::ContractError;
use crate::state::{Config, Lock, Point, Slope, CONFIG, HISTORY, LOCKED, SLOPE_CHANGES};
use crate::utils::{calc_voting_power, get_period, time_limits_check, xastro_token_check};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astro-voting-escrow";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const WEEK: u64 = 7 * 86400; // lock period is rounded down by week
pub const MAX_LOCK_TIME: u64 = 2 * 365 * 86400; // 2 years

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        xastro_token_addr: addr_validate_to_lower(deps.api, &msg.deposit_token_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// ## Description
/// Available the execute messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
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
    add_amount: Option<Uint128>,
    old_slope: Slope,
    new_slope: Slope,
    new_end: Option<u64>,
) -> StdResult<()> {
    let cur_period = get_period(env.block.time.seconds());
    let cur_period_key = U64Key::new(cur_period);
    let contract_addr = env.contract.address;
    let add_amount = add_amount.unwrap_or_default();

    // get last checkpoint
    let last_checkpoint = HISTORY
        .prefix(contract_addr.clone())
        .range(
            deps.as_ref().storage,
            None,
            Some(Bound::Inclusive(cur_period_key.wrapped.clone())),
            Order::Ascending,
        )
        .last();
    let new_point = if let Some(storage_result) = last_checkpoint {
        let (_, point) = storage_result?;
        let end = new_end.unwrap_or(cur_period);
        let scheduled_change = SLOPE_CHANGES
            .may_load(deps.storage, cur_period_key.clone())?
            .unwrap_or(Slope(0));

        Point {
            power: calc_voting_power(&point, cur_period) + add_amount,
            slope: point.slope - old_slope + new_slope - scheduled_change,
            start: cur_period,
            end,
        }
    } else {
        // this error can't happen since this if-branch is intended for checkpoint creation
        let end =
            new_end.ok_or_else(|| StdError::generic_err("Checkpoint initialization error"))?;
        Point {
            power: add_amount,
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
    let mut old_slope = Slope(0);

    // get last checkpoint
    let last_checkpoint = HISTORY
        .prefix(addr.clone())
        .range(
            deps.as_ref().storage,
            None,
            Some(Bound::Inclusive(cur_period_key.wrapped.clone())),
            Order::Ascending,
        )
        .last();
    let new_point = if let Some(storage_result) = last_checkpoint {
        let (_, point) = storage_result?;
        let end = new_end.unwrap_or(point.end);
        let dt = end - cur_period;
        let current_power = calc_voting_power(&point, cur_period);
        let new_slope = if dt != 0 {
            let current_power = current_power.u128() as f32;
            if end > point.end {
                // this is extend_lock_time
                (current_power / dt as f32).into()
            } else {
                // increase lock's amount
                ((current_power + add_amount.u128() as f32) / dt as f32).into()
            }
        } else {
            Slope(0)
        };

        // cancel previously scheduled slope change
        let end_period_key = U64Key::new(point.end);
        if let Some(old_slope) =
            SLOPE_CHANGES.may_load(deps.as_ref().storage, end_period_key.clone())?
        {
            SLOPE_CHANGES.save(
                deps.storage,
                end_period_key,
                &(old_slope - point.slope.clone()),
            )?
        }

        // we need to subtract it from total VP slope
        old_slope = point.slope;

        Point {
            power: current_power + add_amount,
            slope: new_slope,
            start: cur_period,
            end,
        }
    } else {
        // this error can't happen since this if-branch is intended for checkpoint creation
        let end =
            new_end.ok_or_else(|| StdError::generic_err("Checkpoint initialization error"))?;
        let slope = (add_amount.u128() as f32 / (end - cur_period) as f32).into();
        Point {
            power: add_amount,
            slope,
            start: cur_period,
            end,
        }
    };

    // schedule slope change
    SLOPE_CHANGES.update(
        deps.storage,
        U64Key::new(new_point.end),
        |slope_opt| -> StdResult<Slope> {
            if let Some(pslope) = slope_opt {
                Ok(pslope + new_point.slope.clone())
            } else {
                Ok(new_point.slope.clone())
            }
        },
    )?;

    HISTORY.save(deps.storage, (addr, cur_period_key), &new_point)?;
    checkpoint_total(
        deps,
        env,
        Some(add_amount),
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
        Ok(Lock { power: amount, end })
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
                lock.power += amount;
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
                amount: lock.power,
            })?,
            funds: vec![],
        });
        LOCKED.remove(deps.storage, sender);

        // TODO: do we need checkpoint here?
        // checkpoint(deps, env, sender, Some(Uint128::zero()), None)?;

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
    // disabling ability to extend lock time by less than a week
    time_limits_check(time)?;
    let user = info.sender;
    let mut lock = LOCKED
        .load(deps.storage, user.clone())
        .map_err(|_| ContractError::LockDoesntExist {})?;
    if lock.end <= get_period(env.block.time.seconds()) {
        return Err(ContractError::LockExpired {});
    };
    // should not exceed MAX_LOCK_TIME
    time_limits_check(lock.end * WEEK + time - env.block.time.seconds())?;
    lock.end = get_period(env.block.time.seconds()) + get_period(time);
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
        QueryMsg::Users {} => get_all_users(deps, env),
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

    let last_checkpoints = HISTORY
        .prefix(user)
        .range(
            deps.storage,
            None,
            Some(Bound::Inclusive(period_key.wrapped)),
            Order::Ascending,
        )
        .last();

    let (_, point) =
        last_checkpoints.unwrap_or_else(|| Err(StdError::generic_err("User is not found")))?;

    // the point right in this period was found
    let voting_power = if point.start == period {
        point.power
    } else {
        // the point before this period was found thus we can calculate VP in the period we interested in
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

    let last_checkpoint = HISTORY
        .prefix(contract_addr)
        .range(
            deps.storage,
            None,
            Some(Bound::Inclusive(period_key.wrapped.clone())),
            Order::Ascending,
        )
        .last();

    let (_, point) =
        last_checkpoint.unwrap_or_else(|| Err(StdError::generic_err("Checkpoint is not found")))?;

    let voting_power = if point.start == period {
        point.power
    } else {
        let checkpoint_period_key = U64Key::new(point.start);
        let scheduled_slope_changes: Vec<_> = SLOPE_CHANGES
            .range(
                deps.storage,
                Some(Bound::Inclusive(checkpoint_period_key.wrapped)),
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
                start: recalc_period + 1,
                slope: init_point.slope - scheduled_change,
                ..init_point
            }
        }
        init_point.power
    };

    Ok(VotingPowerResponse { voting_power })
}

fn get_all_users(deps: Deps, env: Env) -> StdResult<Binary> {
    let keys: Vec<_> = LOCKED
        .keys(deps.storage, None, None, Order::Ascending)
        .filter_map(|key| {
            let addr = String::from_utf8(key).map_err(StdError::from).ok()?;
            if addr == env.contract.address.as_str() {
                None
            } else {
                Some(addr)
            }
        })
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
