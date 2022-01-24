use astroport::asset::addr_validate_to_lower;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order,
    Response, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_storage_plus::{Bound, U64Key};

use astroport_governance::astro_voting_escrow::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, VotingPowerResponse,
};

use crate::error::ContractError;
use crate::state::{Config, Lock, CONFIG, HISTORY, LOCKED};
use crate::utils::{cur_period, get_total_deposit, time_limits_check, xastro_token_check};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astro-voting-escrow";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const WEEK: u64 = 7 * 86400; // lock period is rounded down by week
pub const MAX_LOCK_TIME: u64 = 2 * 365 * 86400; // 2 years
pub const PRECISION: u8 = 18; // precision for floating point operations

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
        period: 0,
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
        ExecuteMsg::CheckpointTotal {} => {
            checkpoint(deps, env, None, None, None)?;
            Ok(Response::default().add_attribute("action", "checkpoint"))
        }
        ExecuteMsg::ExtendLockTime { time } => extend_lock_time(deps, env, info, time),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Withdraw {} => withdraw(deps, env, info),
    }
}

fn checkpoint(
    deps: DepsMut,
    env: Env,
    user: Option<Addr>,
    add_amount: Option<Uint128>,
    add_time: Option<u64>,
) -> StdResult<()> {
    let cur_period = U64Key::new(cur_period(env.block.time));
    let user = user.unwrap_or(env.contract.address);

    // get checkpoint in the current period
    let last_checkpoint = HISTORY
        .prefix(user.clone())
        .range(
            deps.as_ref().storage,
            None,
            Some(Bound::Inclusive(cur_period.wrapped.clone())),
            Order::Descending,
        )
        .last();
    let block_time = env.block.time.seconds();
    let new_lock = if let Some(storage_result) = last_checkpoint {
        let (_, Lock { amount, start, end }) = storage_result?;
        Lock {
            amount: amount + add_amount.unwrap_or_default(),
            end: end + add_time.unwrap_or(0),
            start: block_time,
        }
    } else {
        Lock {
            amount: add_amount.unwrap_or_default(),
            end: add_time.unwrap_or(MAX_LOCK_TIME), // TODO: should we fix it?
            start: block_time,
        }
    };
    HISTORY.save(deps.storage, (user, cur_period), &new_lock)
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
    LOCKED.update(deps.storage, user.clone(), |lock_opt| {
        if lock_opt.is_some() {
            return Err(ContractError::LockAlreadyExists {});
        }
        Ok(Lock {
            amount,
            start: env.block.time.seconds(),
            end: env.block.time.seconds() + time,
        })
    })?;
    checkpoint(deps, env, Some(user), Some(amount), Some(time))?;

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
            lock.amount += amount;
            Ok(lock)
        } else {
            Err(ContractError::LockDoesntExist {})
        }
    })?;
    checkpoint(deps, env, Some(user), Some(amount), None)?;

    Ok(Response::default().add_attribute("action", "extend_lock_amount"))
}

fn withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let sender = info.sender;
    let lock = LOCKED
        .may_load(deps.storage, sender.clone())?
        .ok_or(ContractError::LockDoesntExist {})?;

    if lock.end > env.block.time.seconds() {
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
        LOCKED.remove(deps.storage, sender.clone());

        checkpoint(deps, env, Some(sender), Some(Uint128::zero()), None)?;

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
    LOCKED.update(deps.storage, user.clone(), |lock_opt| {
        if let Some(mut lock) = lock_opt {
            lock.end += time;
            // should not exceed MAX_LOCK_TIME
            time_limits_check(lock.end - env.block.time.seconds())?;
            Ok(lock)
        } else {
            Err(ContractError::LockDoesntExist {})
        }
    })?;

    checkpoint(deps, env, Some(user), None, Some(time))?;

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
        QueryMsg::TotalVotingPower {} => to_binary(&get_total_voting_power(deps, env)?),
        QueryMsg::UserVotingPower { user } => to_binary(&get_user_voting_power(deps, env, user)?),
    }
}

fn get_total_voting_power(deps: Deps, env: Env) -> StdResult<VotingPowerResponse> {
    let _total_deposit = get_total_deposit(deps, env)?;
    let voting_power = Uint128::zero();
    Ok(VotingPowerResponse { voting_power })
}

fn get_user_voting_power(deps: Deps, env: Env, user: String) -> StdResult<VotingPowerResponse> {
    let user = addr_validate_to_lower(deps.api, &user)?;
    let lock = LOCKED.load(deps.storage, user)?;
    let slope = lock.amount.u128() as f32 / (lock.end - lock.start) as f32;
    let voting_power =
        lock.amount.u128() as f32 - slope * (env.block.time.seconds() - lock.start) as f32;
    Ok(VotingPowerResponse {
        voting_power: Uint128::from(voting_power as u128),
    })
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
