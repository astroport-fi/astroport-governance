use astroport::asset::addr_validate_to_lower;
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Reply, ReplyOn, Response, StdError, StdResult, Storage, SubMsg, Timestamp, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};

use astroport_governance::astro_voting_escrow::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, VotingPowerResponse,
};

use crate::error::ContractError;
use crate::state::{Config, History, Lock, Point, CONFIG, HISTORY, LOCKED};
use crate::utils::{change_balance, ChangeBalanceOp};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astro-voting-escrow";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const WEEK: u64 = 7 * 86400; // lock period is rounded down by week
const MAX_LOCK_TIME: u128 = 2 * 365 * 86400; // 2 years
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
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let first_point = Point::new(env.block.time, env.block.height);
    HISTORY.save(
        deps.storage,
        &History {
            main_points: vec![first_point],
        },
    )?;

    let config = Config {
        period: 0,
        xastro_token_addr: addr_validate_to_lower(deps.api, &msg.deposit_token_addr)?,
        balance: Uint128::zero(),
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
        ExecuteMsg::CheckpointTotal {} => checkpoint(None),
        ExecuteMsg::CheckpointUser { user } => {
            let user_addr = addr_validate_to_lower(deps.api, &user)?;
            checkpoint(Some(user_addr))
        }
        ExecuteMsg::ExtendLockTime { time } => extend_lock_time(deps, info, time),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
    }
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
        Cw20HookMsg::CreateLock { time } => create_lock(deps, info, cw20_msg.amount, time),
        Cw20HookMsg::ExtendLockAmount {} => extend_lock_amount(deps, info, cw20_msg.amount),
        Cw20HookMsg::Withdraw {} => withdraw(deps, env, info),
    }
}

fn checkpoint(user: Option<Addr>) -> Result<Response, ContractError> {
    todo!()
}

fn create_lock(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
    time: Timestamp,
) -> Result<Response, ContractError> {
    LOCKED.update(deps.storage, info.sender, |lock_opt| {
        if lock_opt.is_some() {
            return Err(ContractError::LockAlreadyExists {});
        }
        Ok(Lock { amount, time })
    })?;
    change_balance(deps, ChangeBalanceOp::Add, amount)?;

    Ok(Response::default().add_attribute("action", "create_lock"))
}

fn extend_lock_amount(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    LOCKED.update(deps.storage, info.sender, |lock_opt| {
        if let Some(mut lock) = lock_opt {
            lock.amount += amount;
            Ok(lock)
        } else {
            Err(ContractError::LockDoesntExist {})
        }
    })?;
    change_balance(deps, ChangeBalanceOp::Add, amount)?;

    Ok(Response::default().add_attribute("action", "extend_lock_amount"))
}

fn withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let sender = info.sender;
    let lock = LOCKED
        .may_load(deps.storage, sender.clone())?
        .ok_or_else(|| ContractError::LockDoesntExist {})?;

    if lock.time < env.block.time {
        Err(ContractError::LockWasNotExpired {})
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
        change_balance(deps, ChangeBalanceOp::Sub, lock.amount)?;

        Ok(Response::default()
            .add_message(transfer_msg)
            .add_attribute("action", "withdraw"))
    }
}

fn extend_lock_time(
    deps: DepsMut,
    info: MessageInfo,
    time: Timestamp,
) -> Result<Response, ContractError> {
    LOCKED.update(deps.storage, info.sender, |lock_opt| {
        if let Some(mut lock) = lock_opt {
            if lock.time < time {
                lock.time = time;
                Ok(lock)
            } else {
                Err(ContractError::LockTimeDecreaseError {})
            }
        } else {
            Err(ContractError::LockDoesntExist {})
        }
    })?;

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
    }
}

fn get_total_voting_power(deps: Deps, _env: Env) -> StdResult<VotingPowerResponse> {
    let config = CONFIG.load(deps.storage)?;
    let voting_power = Decimal::from_ratio(config.balance, MAX_LOCK_TIME);
    Ok(VotingPowerResponse { voting_power })
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
