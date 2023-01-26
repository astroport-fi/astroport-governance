use crate::astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    QueryRequest, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg, WasmQuery,
};
use cw2::{get_contract_version, set_contract_version, ContractVersion, CONTRACT};
use cw20::{
    BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg, Logo, LogoInfo,
    MarketingInfoResponse, MinterResponse, TokenInfoResponse,
};
use cw20_base::contract::{
    execute_update_marketing, execute_upload_logo, query_download_logo, query_marketing_info,
};
use cw20_base::state::{MinterData, TokenInfo, LOGO, MARKETING_INFO, TOKEN_INFO};

use crate::astroport::querier::query_token_balance;
use astroport_governance::utils::{
    get_period, get_periods_count, DecimalCheckedOps, EPOCH_START, WEEK,
};
use astroport_governance::voting_escrow::{
    BlacklistedVotersResponse, ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg,
    LockInfoResponse, MigrateMsg, QueryMsg, VotingPowerResponse, DEFAULT_LIMIT, MAX_LIMIT,
};

use crate::error::ContractError;
use crate::migration::v110::MigrationV110;
use crate::migration::Migration;
use crate::state::{
    Config, Lock, Point, BLACKLIST, CONFIG, HISTORY, LAST_SLOPE_CHANGE, LOCKED, OWNERSHIP_PROPOSAL,
};
use crate::utils::{
    adjust_vp_and_slope, blacklist_check, calc_coefficient, calc_early_withdraw_amount,
    calc_voting_power, cancel_scheduled_slope, fetch_last_checkpoint, fetch_slope_changes,
    schedule_slope_change, time_limits_check, validate_addresses, xastro_token_check,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astro-voting-escrow";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
/// Returns a default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the paramters used for creating a contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Accept values within [0,1] limit.
    if msg.max_exit_penalty > Decimal::one() {
        return Err(StdError::generic_err("Max exit penalty should be <= 1").into());
    }
    let slashed_fund_receiver = msg
        .slashed_fund_receiver
        .map(|addr| deps.api.addr_validate(&addr))
        .transpose()?;
    let deposit_token_addr = deps.api.addr_validate(&msg.deposit_token_addr)?;

    // Initialize early withdraw parameters
    let xastro_minter_resp: MinterResponse = deps
        .querier
        .query_wasm_smart(&deposit_token_addr, &Cw20QueryMsg::Minter {})?;
    let staking_config: crate::astroport::staking::ConfigResponse = deps.querier.query_wasm_smart(
        &xastro_minter_resp.minter,
        &crate::astroport::staking::QueryMsg::Config {},
    )?;

    let mut config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        guardian_addr: None,
        deposit_token_addr,
        max_exit_penalty: msg.max_exit_penalty,
        astro_addr: staking_config.deposit_token_addr,
        xastro_staking_addr: deps.api.addr_validate(&xastro_minter_resp.minter)?,
        slashed_fund_receiver,
    };
    if let Some(guardian_addr) = msg.guardian_addr {
        config.guardian_addr = Some(deps.api.addr_validate(&guardian_addr)?);
    }
    CONFIG.save(deps.storage, &config)?;

    let cur_period = get_period(env.block.time.seconds())?;
    let point = Point {
        power: Uint128::zero(),
        start: cur_period,
        end: 0,
        slope: Default::default(),
    };
    HISTORY.save(
        deps.storage,
        (env.contract.address.clone(), cur_period),
        &point,
    )?;
    BLACKLIST.save(deps.storage, &vec![])?;

    if let Some(marketing) = msg.marketing {
        let logo = if let Some(logo) = marketing.logo {
            LOGO.save(deps.storage, &logo)?;

            match logo {
                Logo::Url(url) => Some(LogoInfo::Url(url)),
                Logo::Embedded(_) => Some(LogoInfo::Embedded),
            }
        } else {
            None
        };

        let data = MarketingInfoResponse {
            project: marketing.project,
            description: marketing.description,
            marketing: marketing
                .marketing
                .map(|addr| deps.api.addr_validate(&addr))
                .transpose()?,
            logo,
        };
        MARKETING_INFO.save(deps.storage, &data)?;
    }

    // Store token info
    let data = TokenInfo {
        name: "Vote Escrowed xASTRO".to_string(),
        symbol: "vxASTRO".to_string(),
        decimals: 6,
        total_supply: Uint128::zero(),
        mint: Some(MinterData {
            minter: env.contract.address,
            cap: None,
        }),
    };

    TOKEN_INFO.save(deps.storage, &data)?;

    Ok(Response::default())
}

/// ## Description
/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::ExtendLockTime { time }** Increase a staker's lock time.
///
/// * **ExecuteMsg::Receive(msg)** Parse incoming messages coming from the xASTRO token contract.
///
/// * **ExecuteMsg::Withdraw {}** Withdraw all xASTRO from a lock position if the lock has expired.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change contract ownership.
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
        ExecuteMsg::ExtendLockTime { time } => extend_lock_time(deps, env, info, time),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Withdraw {} => withdraw(deps, env, info),
        ExecuteMsg::WithdrawEarly {} => withdraw_early(deps, env, info),
        ExecuteMsg::EarlyWithdrawCallback {
            precallback_astro,
            slashed_funds_receiver,
        } => withdraw_early_callback(
            deps.as_ref(),
            env,
            info,
            precallback_astro,
            slashed_funds_receiver,
        ),
        ExecuteMsg::ConfigureEarlyWithdrawal {
            max_penalty,
            slashed_fund_receiver,
        } => configure_early_withdrawal(deps, info, max_penalty, slashed_fund_receiver),
        ExecuteMsg::ProposeNewOwner {
            new_owner,
            expires_in,
        } => {
            let config: Config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                new_owner,
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
        ExecuteMsg::UpdateBlacklist {
            append_addrs,
            remove_addrs,
        } => update_blacklist(deps, env, info, append_addrs, remove_addrs),
        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => execute_update_marketing(deps, env, info, project, description, marketing)
            .map_err(|e| e.into()),
        ExecuteMsg::UploadLogo(logo) => {
            execute_upload_logo(deps, env, info, logo).map_err(|e| e.into())
        }
        ExecuteMsg::UpdateConfig { new_guardian } => {
            execute_update_config(deps, info, new_guardian)
        }
    }
}

/// ## Description
/// Checkpoint the total voting power (total supply of vxASTRO).
/// This function fetches the last available vxASTRO checkpoint, recalculates passed periods since the checkpoint and until now,
/// applies slope changes and saves all recalculated periods in [`HISTORY`].
/// The function returns Ok(()) in case of success or [`StdError`]
/// in case of a serialization/deserialization error.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **add_voting_power** is an object of type [`Option<Uint128>`]. This is an amount of vxASTRO to add to the total.
///
/// * **reduce_power** is an object of type [`Option<Uint128>`]. This is an amount of vxASTRO to subtract from the total.
///
/// * **old_slope** is an object of type [`Decimal`]. This is the old slope applied to the total voting power (vxASTRO supply).
///
/// * **new_slope** is an object of type [`Decimal`]. This is the new slope to be applied to the total voting power (vxASTRO supply).
fn checkpoint_total(
    deps: DepsMut,
    env: Env,
    add_voting_power: Option<Uint128>,
    reduce_power: Option<Uint128>,
    old_slope: Uint128,
    new_slope: Uint128,
) -> StdResult<()> {
    let cur_period = get_period(env.block.time.seconds())?;
    let cur_period_key = cur_period;
    let contract_addr = env.contract.address;
    let add_voting_power = add_voting_power.unwrap_or_default();

    // Get last checkpoint
    let last_checkpoint = fetch_last_checkpoint(deps.as_ref(), &contract_addr, cur_period_key)?;
    let new_point = if let Some((_, mut point)) = last_checkpoint {
        let last_slope_change = LAST_SLOPE_CHANGE
            .may_load(deps.as_ref().storage)?
            .unwrap_or(0);
        if last_slope_change < cur_period {
            let scheduled_slope_changes =
                fetch_slope_changes(deps.as_ref(), last_slope_change, cur_period)?;
            // Recalculating passed points
            for (recalc_period, scheduled_change) in scheduled_slope_changes {
                point = Point {
                    power: calc_voting_power(&point, recalc_period),
                    start: recalc_period,
                    slope: point.slope - scheduled_change,
                    ..point
                };
                HISTORY.save(deps.storage, (contract_addr.clone(), recalc_period), &point)?
            }

            LAST_SLOPE_CHANGE.save(deps.storage, &cur_period)?
        }

        let new_power = (calc_voting_power(&point, cur_period) + add_voting_power)
            .saturating_sub(reduce_power.unwrap_or_default());

        Point {
            power: new_power,
            slope: point.slope - old_slope + new_slope,
            start: cur_period,
            ..point
        }
    } else {
        Point {
            power: add_voting_power,
            slope: new_slope,
            start: cur_period,
            end: 0, // we don't use 'end' in total voting power calculations
        }
    };
    HISTORY.save(deps.storage, (contract_addr, cur_period_key), &new_point)
}

/// ## Description
/// Checkpoint a user's voting power (vxASTRO balance).
/// This function fetches the user's last available checkpoint, calculates the user's current voting power, applies slope changes based on
/// `add_amount` and `new_end` parameters, schedules slope changes for total voting power and saves the new checkpoint for the current
/// period in [`HISTORY`] (using the user's address).
/// If a user already checkpointed themselves for the current period, then this function uses the current checkpoint as the latest
/// available one. The function returns Ok(()) in case of success or [`StdError`] in case of a serialization/deserialization error.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **addr** is an object of type [`Addr`]. This is the staker for which we checkpoint the voting power.
///
/// * **add_amount** is an object of type [`Option<Uint128>`]. This is an amount of vxASTRO to add to the staker's balance.
///
/// * **new_end** is an object of type [`Option<u64>`]. This is a new lock time for the staker's vxASTRO position.
fn checkpoint(
    mut deps: DepsMut,
    env: Env,
    addr: Addr,
    add_amount: Option<Uint128>,
    new_end: Option<u64>,
) -> StdResult<()> {
    let cur_period = get_period(env.block.time.seconds())?;
    let cur_period_key = cur_period;
    let add_amount = add_amount.unwrap_or_default();
    let mut old_slope = Default::default();
    let mut add_voting_power = Uint128::zero();

    // Get the last user checkpoint
    let last_checkpoint = fetch_last_checkpoint(deps.as_ref(), &addr, cur_period_key)?;
    let new_point = if let Some((_, point)) = last_checkpoint {
        let end = new_end.unwrap_or(point.end);
        let dt = end.saturating_sub(cur_period);
        let current_power = calc_voting_power(&point, cur_period);
        let new_slope = if dt != 0 {
            if end > point.end && add_amount.is_zero() {
                // This is extend_lock_time. Recalculating user's voting power
                let mut lock = LOCKED.load(deps.storage, addr.clone())?;
                let mut new_voting_power = calc_coefficient(dt).checked_mul_uint128(lock.amount)?;
                let slope = adjust_vp_and_slope(&mut new_voting_power, dt)?;
                // new_voting_power should always be >= current_power. saturating_sub is used for extra safety
                add_voting_power = new_voting_power.saturating_sub(current_power);
                lock.last_extend_lock_period = cur_period;
                LOCKED.save(deps.storage, addr.clone(), &lock, env.block.height)?;
                slope
            } else {
                // This is an increase in the user's lock amount
                let raw_add_voting_power = calc_coefficient(dt).checked_mul_uint128(add_amount)?;
                let mut new_voting_power = current_power.checked_add(raw_add_voting_power)?;
                let slope = adjust_vp_and_slope(&mut new_voting_power, dt)?;
                // new_voting_power should always be >= current_power. saturating_sub is used for extra safety
                add_voting_power = new_voting_power.saturating_sub(current_power);
                slope
            }
        } else {
            Uint128::zero()
        };

        // Cancel the previously scheduled slope change
        cancel_scheduled_slope(deps.branch(), point.slope, point.end)?;

        // We need to subtract the slope point from the total voting power slope
        old_slope = point.slope;

        Point {
            power: current_power + add_voting_power,
            slope: new_slope,
            start: cur_period,
            end,
        }
    } else {
        // This error can't happen since this if-branch is intended for checkpoint creation
        let end =
            new_end.ok_or_else(|| StdError::generic_err("Checkpoint initialization error"))?;
        let dt = end - cur_period;
        add_voting_power = calc_coefficient(dt).checked_mul_uint128(add_amount)?;
        let slope = adjust_vp_and_slope(&mut add_voting_power, dt)?;
        Point {
            power: add_voting_power,
            slope,
            start: cur_period,
            end,
        }
    };

    // Schedule a slope change
    schedule_slope_change(deps.branch(), new_point.slope, new_point.end)?;

    HISTORY.save(deps.storage, (addr, cur_period_key), &new_point)?;
    checkpoint_total(
        deps,
        env,
        Some(add_voting_power),
        None,
        old_slope,
        new_point.slope,
    )
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then a [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 message to process.
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    xastro_token_check(deps.as_ref(), info.sender)?;
    let sender = deps.api.addr_validate(&cw20_msg.sender)?;
    blacklist_check(deps.as_ref(), &sender)?;

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::CreateLock { time } => create_lock(deps, env, sender, cw20_msg.amount, time),
        Cw20HookMsg::ExtendLockAmount {} => deposit_for(deps, env, cw20_msg.amount, sender),
        Cw20HookMsg::DepositFor { user } => {
            let addr = deps.api.addr_validate(&user)?;
            blacklist_check(deps.as_ref(), &addr)?;
            deposit_for(deps, env, cw20_msg.amount, addr)
        }
    }
}

/// ## Description
/// Creates a lock for the user that lasts for the specified time duration (in seconds).
/// Checks that the user is locking xASTRO tokens.
/// Checks that the lock time is within [`WEEK`]..[`MAX_LOCK_TIME`].
/// Creates a lock if it doesn't exist and triggers a [`checkpoint`] for the staker.
/// If a lock already exists, then a [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user** is an object of type [`Addr`]. This is the staker for which we create a lock position.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of xASTRO deposited in the lock position.
///
/// * **time** is an object of type [`u64`]. This is the duration of the lock.
fn create_lock(
    deps: DepsMut,
    env: Env,
    user: Addr,
    amount: Uint128,
    time: u64,
) -> Result<Response, ContractError> {
    time_limits_check(time)?;

    let block_period = get_period(env.block.time.seconds())?;
    let end = block_period + get_periods_count(time);

    LOCKED.update(deps.storage, user.clone(), env.block.height, |lock_opt| {
        if lock_opt.is_some() && !lock_opt.unwrap().amount.is_zero() {
            return Err(ContractError::LockAlreadyExists {});
        }
        Ok(Lock {
            amount,
            start: block_period,
            end,
            last_extend_lock_period: block_period,
        })
    })?;

    checkpoint(deps, env, user, Some(amount), Some(end))?;

    Ok(Response::default().add_attribute("action", "create_lock"))
}

/// ## Description
/// Deposits an 'amount' of xASTRO tokens into 'user''s lock.
/// Checks that the user is transferring and locking xASTRO.
/// Triggers a [`checkpoint`] for the user.
/// If the user does not have a lock, then a [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of xASTRO to deposit.
///
/// * **user** is an object of type [`Addr`]. This is the user who's lock amount will increase.
fn deposit_for(
    deps: DepsMut,
    env: Env,
    amount: Uint128,
    user: Addr,
) -> Result<Response, ContractError> {
    LOCKED.update(
        deps.storage,
        user.clone(),
        env.block.height,
        |lock_opt| match lock_opt {
            Some(mut lock) if !lock.amount.is_zero() => {
                if lock.end <= get_period(env.block.time.seconds())? {
                    Err(ContractError::LockExpired {})
                } else {
                    lock.amount += amount;
                    Ok(lock)
                }
            }
            _ => Err(ContractError::LockDoesNotExist {}),
        },
    )?;
    checkpoint(deps, env, user, Some(amount), None)?;

    Ok(Response::default().add_attribute("action", "deposit_for"))
}

/// ## Description
/// Withdraws the whole amount of locked xASTRO from a specific user lock.
/// If the user lock doesn't exist or if it has not yet expired, then a [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`]. This is the withdrawal message coming from a user.
fn withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let sender = info.sender;
    // 'LockDoesNotExist' is thrown either when a lock does not exist in LOCKED or when a lock exists but lock.amount == 0
    let mut lock = LOCKED
        .may_load(deps.storage, sender.clone())?
        .filter(|lock| !lock.amount.is_zero())
        .ok_or(ContractError::LockDoesNotExist {})?;

    let cur_period = get_period(env.block.time.seconds())?;
    if lock.end > cur_period {
        Err(ContractError::LockHasNotExpired {})
    } else {
        let config = CONFIG.load(deps.storage)?;
        let transfer_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.deposit_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: sender.to_string(),
                amount: lock.amount,
            })?,
            funds: vec![],
        });
        lock.amount = Uint128::zero();
        LOCKED.save(deps.storage, sender.clone(), &lock, env.block.height)?;

        // We need to checkpoint and eliminate the slope influence on a future lock
        HISTORY.save(
            deps.storage,
            (sender, cur_period),
            &Point {
                power: Uint128::zero(),
                start: cur_period,
                end: cur_period,
                slope: Default::default(),
            },
        )?;

        Ok(Response::default()
            .add_message(transfer_msg)
            .add_attribute("action", "withdraw"))
    }
}

/// ## Description
/// Sets a max exit penalty and a slashed funds receiver. Can be called by the owner only.
fn configure_early_withdrawal(
    deps: DepsMut,
    info: MessageInfo,
    max_exit_penalty: Option<Decimal>,
    slashed_fund_receiver: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    // Accept values within [0,1] limit
    if let Some(max_exit_penalty) = max_exit_penalty {
        if max_exit_penalty > Decimal::one() {
            return Err(StdError::generic_err("Max exit penalty should be <= 1").into());
        } else {
            config.max_exit_penalty = max_exit_penalty;
        }
    }
    if let Some(slashed_fund_receiver) = slashed_fund_receiver {
        config.slashed_fund_receiver = Some(deps.api.addr_validate(&slashed_fund_receiver)?);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default().add_attribute("action", "configure_early_withdrawal"))
}

/// ## Description
/// Withdraws stacked funds with penalty before the lock expires.
/// The penalty is calculated as min(max_exit_penalty, time_left_until_unlock / MAX_LOCK_TIME).
/// Slashed funds are sent to the slashed funds receiver address.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`]. This is the withdrawal message coming from a user.
fn withdraw_early(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let sender = info.sender;
    // 'LockDoesntExist' is either a lock does not exist in LOCKED or a lock exits but lock.amount == 0
    let mut lock = LOCKED
        .may_load(deps.storage, sender.clone())?
        .filter(|lock| !lock.amount.is_zero())
        .ok_or(ContractError::LockDoesNotExist {})?;

    let cur_period = get_period(env.block.time.seconds())?;
    if lock.end <= cur_period {
        return Err(ContractError::LockExpired {});
    }

    let config = CONFIG.load(deps.storage)?;

    let (slashed_amount, return_amount) =
        calc_early_withdraw_amount(config.max_exit_penalty, lock.end - cur_period, lock.amount);

    let slashed_funds_receiver = config
        .slashed_fund_receiver
        .clone()
        .ok_or(ContractError::EarlyWithdrawNotAvailable {})?;

    let mut transfer_msgs = vec![];
    if !return_amount.is_zero() {
        let transfer_msg = SubMsg::new(WasmMsg::Execute {
            contract_addr: config.deposit_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: sender.to_string(),
                amount: return_amount,
            })?,
            funds: vec![],
        });
        transfer_msgs.push(transfer_msg);
    }
    if !slashed_amount.is_zero() {
        let send_msg = SubMsg::new(WasmMsg::Execute {
            contract_addr: config.deposit_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: config.xastro_staking_addr.to_string(),
                amount: slashed_amount,
                msg: to_binary(&crate::astroport::staking::Cw20HookMsg::Leave {})?,
            })?,
            funds: vec![],
        });
        transfer_msgs.push(send_msg);

        let precallback_astro = query_token_balance(
            &deps.querier,
            config.astro_addr,
            env.contract.address.clone(),
        )?;
        let callback_msg = SubMsg::new(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::EarlyWithdrawCallback {
                precallback_astro,
                slashed_funds_receiver,
            })?,
            funds: vec![],
        });
        transfer_msgs.push(callback_msg);
    }

    lock.amount = Uint128::zero();
    LOCKED.save(deps.storage, sender.clone(), &lock, env.block.height)?;

    let cur_period_key = cur_period;
    let last_checkpoint = fetch_last_checkpoint(deps.as_ref(), &sender, cur_period_key)?;

    // If a user has voting power they must have checkpoint.
    let (_, point) = last_checkpoint.ok_or_else(|| {
        StdError::generic_err(format!(
            "There is no previous checkpoint for user: {}",
            sender.as_str()
        ))
    })?;
    // We need to checkpoint with zero power and zero slope
    HISTORY.save(
        deps.storage,
        (sender, cur_period_key),
        &Point {
            power: Uint128::zero(),
            slope: Default::default(),
            start: cur_period,
            end: cur_period,
        },
    )?;

    let cur_power = calc_voting_power(&point, cur_period);
    if !cur_power.is_zero() {
        cancel_scheduled_slope(deps.branch(), point.slope, point.end)?;
        // We need to checkpoint total VP and eliminate the slope influence on a future lock
        checkpoint_total(
            deps,
            env,
            None,
            Some(cur_power),
            point.slope,
            Default::default(),
        )?
    }

    Ok(Response::default()
        .add_submessages(transfer_msgs)
        .add_attribute("action", "withdraw_early"))
}

/// ## Description
/// A callback after early withdraw. Can be called only by the contract itself.
/// This is intended for transferring converted xASTRO (in form of ASTRO) to the slashed funds receiver.
/// ## Parameters
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **precallback_astro** is a contracts' ASTRO balance before a callback
///
/// * **slashed_funds_receiver** is a slashed funds receiver address
fn withdraw_early_callback(
    deps: Deps,
    env: Env,
    info: MessageInfo,
    precallback_astro: Uint128,
    slashed_funds_receiver: Addr,
) -> Result<Response, ContractError> {
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    let config = CONFIG.load(deps.storage)?;

    let current_astro_balance = query_token_balance(
        &deps.querier,
        config.astro_addr.clone(),
        env.contract.address,
    )?;
    let return_astro_amount = current_astro_balance.saturating_sub(precallback_astro);

    if !return_astro_amount.is_zero() {
        // Check that slashed_funds_receiver is an escrow_fee_distributor contract
        // otherwise transfer return_astro_amount to slashed_funds_receiver address
        let version: StdResult<ContractVersion> =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
                contract_addr: slashed_funds_receiver.to_string(),
                key: CONTRACT.as_slice().into(),
            }));
        let transfer_msg = match version {
            Ok(ContractVersion { contract, .. })
                if contract.eq("astroport-escrow-fee-distributor") =>
            {
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: config.astro_addr.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Send {
                        contract: slashed_funds_receiver.to_string(),
                        amount: return_astro_amount,
                        msg: to_binary(&{})?,
                    })?,
                    funds: vec![],
                })
            }
            _ => SubMsg::new(WasmMsg::Execute {
                contract_addr: config.astro_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: slashed_funds_receiver.to_string(),
                    amount: return_astro_amount,
                })?,
                funds: vec![],
            }),
        };

        Ok(Response::new().add_submessage(transfer_msg))
    } else {
        Err(StdError::generic_err("Failed to unstake ASTRO").into())
    }
}

/// ## Description
/// Increase the current lock time for a staker by a specified time period.
/// Evaluates that the `time` is within [`WEEK`]..[`MAX_LOCK_TIME`]
/// and then it triggers a [`checkpoint`].
/// If the user lock doesn't exist or if it expired, then a [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful
///
/// ## Note
/// The time is added to the lock's `end`.
/// For example, at period 0, the user has their xASTRO locked for 3 weeks.
/// In 1 week, they increase their lock time by 10 weeks, thus the unlock period becomes 13 weeks.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **time** is an object of type [`u64`]. This is the increase in lock time applied to the staker's position.
fn extend_lock_time(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    time: u64,
) -> Result<Response, ContractError> {
    let user = info.sender;
    blacklist_check(deps.as_ref(), &user)?;
    let mut lock = LOCKED
        .may_load(deps.storage, user.clone())?
        .filter(|lock| !lock.amount.is_zero())
        .ok_or(ContractError::LockDoesNotExist {})?;

    // Disable the ability to extend the lock time by less than a week
    time_limits_check(time)?;

    if lock.end <= get_period(env.block.time.seconds())? {
        return Err(ContractError::LockExpired {});
    };

    // Should not exceed MAX_LOCK_TIME
    time_limits_check(EPOCH_START + lock.end * WEEK + time - env.block.time.seconds())?;
    lock.end += get_periods_count(time);
    LOCKED.save(deps.storage, user.clone(), &lock, env.block.height)?;

    checkpoint(deps, env, user, None, Some(lock.end))?;

    Ok(Response::default().add_attribute("action", "extend_lock_time"))
}

/// ## Description
/// Update the staker blacklist. Whitelists addresses specified in 'remove_addrs'
/// and blacklists new addresses specified in 'append_addrs'. Nullifies staker voting power and
/// cancels their contribution in the total voting power (total vxASTRO supply).
/// Returns a [`ContractError`] in case of a (de/ser)ialization or address validation error.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **append_addrs** is an [`Option`] containing a [`Vec<String>`]. This is the array of addresses to blacklist.
///
/// * **remove_addrs** is an [`Option`] containing a [`Vec<String>`]. This is the array of addresses to whitelist.
fn update_blacklist(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    append_addrs: Option<Vec<String>>,
    remove_addrs: Option<Vec<String>>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    // Permission check
    if info.sender != config.owner && Some(info.sender) != config.guardian_addr {
        return Err(ContractError::Unauthorized {});
    }
    let append_addrs = append_addrs.unwrap_or_default();
    let remove_addrs = remove_addrs.unwrap_or_default();
    let blacklist = BLACKLIST.load(deps.storage)?;
    let append: Vec<_> = validate_addresses(deps.as_ref(), &append_addrs)?
        .into_iter()
        .filter(|addr| !blacklist.contains(addr))
        .collect();
    let remove: Vec<_> = validate_addresses(deps.as_ref(), &remove_addrs)?
        .into_iter()
        .filter(|addr| blacklist.contains(addr))
        .collect();

    if append.is_empty() && remove.is_empty() {
        return Err(StdError::generic_err("Append and remove arrays are empty").into());
    }

    let cur_period = get_period(env.block.time.seconds())?;
    let cur_period_key = cur_period;
    let mut reduce_total_vp = Uint128::zero(); // accumulator for decreasing total voting power
    let mut old_slopes = Uint128::zero(); // accumulator for old slopes

    for addr in append.iter() {
        let last_checkpoint = fetch_last_checkpoint(deps.as_ref(), addr, cur_period_key)?;
        if let Some((_, point)) = last_checkpoint {
            // We need to checkpoint with zero power and zero slope
            HISTORY.save(
                deps.storage,
                (addr.clone(), cur_period_key),
                &Point {
                    power: Uint128::zero(),
                    slope: Default::default(),
                    start: cur_period,
                    end: cur_period,
                },
            )?;

            let cur_power = calc_voting_power(&point, cur_period);
            // User's contribution is already zero. Skipping them
            if cur_power.is_zero() {
                continue;
            }

            // User's contribution in the total voting power calculation
            reduce_total_vp += cur_power;
            old_slopes += point.slope;
            cancel_scheduled_slope(deps.branch(), point.slope, point.end)?;
        }
    }

    if !reduce_total_vp.is_zero() || !old_slopes.is_zero() {
        // Trigger a total voting power recalculation
        checkpoint_total(
            deps.branch(),
            env.clone(),
            None,
            Some(reduce_total_vp),
            old_slopes,
            Default::default(),
        )?;
    }

    for addr in remove.iter() {
        let lock_opt = LOCKED.may_load(deps.storage, addr.clone())?;
        if let Some(Lock { amount, end, .. }) = lock_opt {
            checkpoint(
                deps.branch(),
                env.clone(),
                addr.clone(),
                Some(amount),
                Some(end),
            )?;
        }
    }

    BLACKLIST.update(deps.storage, |blacklist| -> StdResult<Vec<Addr>> {
        let mut updated_blacklist: Vec<_> = blacklist
            .into_iter()
            .filter(|addr| !remove.contains(addr))
            .collect();
        updated_blacklist.extend(append);
        Ok(updated_blacklist)
    })?;

    let mut attrs = vec![("action", "update_blacklist")];
    let append_joined = append_addrs.join(",");
    if !append_addrs.is_empty() {
        attrs.push(("added_addresses", append_joined.as_str()))
    }
    let remove_joined = remove_addrs.join(",");
    if !remove_addrs.is_empty() {
        attrs.push(("removed_addresses", remove_joined.as_str()))
    }

    Ok(Response::default().add_attributes(attrs))
}

/// ## Description
/// Updates contract parameters.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **new_guardian** is an optional object of type [`String`].
fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_guardian: Option<String>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    if cfg.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(new_guardian) = new_guardian {
        cfg.guardian_addr = Some(deps.api.addr_validate(&new_guardian)?);
    }

    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::default().add_attribute("action", "execute_update_config"))
}

/// ## Description
/// Expose available contract queries.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::TotalVotingPower {}** Fetch the total voting power (vxASTRO supply) at the current block.
///
/// * **QueryMsg::UserVotingPower { user }** Fetch the user's voting power (vxASTRO balance) at the current block.
///
/// * **QueryMsg::TotalVotingPowerAt { time }** Fetch the total voting power (vxASTRO supply) at a specified timestamp.
///
/// * **QueryMsg::UserVotingPowerAt { time }** Fetch the user's voting power (vxASTRO balance) at a specified timestamp.
///
/// * **QueryMsg::LockInfo { user }** Fetch a user's lock information.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::CheckVotersAreBlacklisted { voters } => {
            to_binary(&check_voters_are_blacklisted(deps, voters)?)
        }
        QueryMsg::BlacklistedVoters { start_after, limit } => {
            to_binary(&get_blacklisted_voters(deps, start_after, limit)?)
        }
        QueryMsg::TotalVotingPower {} => to_binary(&get_total_voting_power(deps, env, None)?),
        QueryMsg::UserVotingPower { user } => {
            to_binary(&get_user_voting_power(deps, env, user, None)?)
        }
        QueryMsg::TotalVotingPowerAt { time } => {
            to_binary(&get_total_voting_power(deps, env, Some(time))?)
        }
        QueryMsg::TotalVotingPowerAtPeriod { period } => {
            to_binary(&get_total_voting_power_at_period(deps, env, period)?)
        }
        QueryMsg::UserVotingPowerAt { user, time } => {
            to_binary(&get_user_voting_power(deps, env, user, Some(time))?)
        }
        QueryMsg::UserVotingPowerAtPeriod { user, period } => {
            to_binary(&get_user_voting_power_at_period(deps, user, period)?)
        }
        QueryMsg::LockInfo { user } => to_binary(&get_user_lock_info(deps, env, user)?),
        QueryMsg::EarlyWithdrawAmount { user } => {
            to_binary(&get_early_withdraw_amount(deps, env, user)?)
        }
        QueryMsg::UserDepositAtHeight { user, height } => {
            to_binary(&get_user_deposit_at_height(deps, user, height)?)
        }
        QueryMsg::Config {} => {
            let config = CONFIG.load(deps.storage)?;
            to_binary(&ConfigResponse {
                owner: config.owner.to_string(),
                guardian_addr: config.guardian_addr,
                deposit_token_addr: config.deposit_token_addr.to_string(),
                max_exit_penalty: config.max_exit_penalty,
                slashed_fund_receiver: config.slashed_fund_receiver.map(|addr| addr.to_string()),
                astro_addr: config.astro_addr.to_string(),
                xastro_staking_addr: config.xastro_staking_addr.to_string(),
            })
        }
        QueryMsg::Balance { address } => to_binary(&get_user_balance(deps, env, address)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps, env)?),
        QueryMsg::MarketingInfo {} => to_binary(&query_marketing_info(deps)?),
        QueryMsg::DownloadLogo {} => to_binary(&query_download_logo(deps)?),
    }
}

/// ## Description
/// Checks if specified addresses are blacklisted. Returns a [`Response`] with the specified
/// attributes if the operation was successful, otherwise then a [`StdError`] is returned.
///
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **voters** is a list of type [`String`]. Specifies addresses to check if they are blacklisted.
pub fn check_voters_are_blacklisted(
    deps: Deps,
    voters: Vec<String>,
) -> StdResult<BlacklistedVotersResponse> {
    let black_list = BLACKLIST.load(deps.storage)?;

    for voter in voters {
        let voter_addr = deps.api.addr_validate(voter.as_str())?;
        if !black_list.contains(&voter_addr) {
            return Ok(BlacklistedVotersResponse::VotersNotBlacklisted { voter });
        }
    }

    Ok(BlacklistedVotersResponse::VotersBlacklisted {})
}

/// ## Description
/// Returns a list of blacklisted voters.
///
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **start_after** is an object of type [`Option<String>`]. This is an optional field
/// that specifies whether the function should return a list of voters starting from a
/// specific address onward.
///
/// * **limit** is an object of type [`Option<u32>`]. This is the max amount of voters
/// addresses to return.
pub fn get_blacklisted_voters(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<Addr>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let mut black_list = BLACKLIST.load(deps.storage)?;

    if black_list.is_empty() {
        return Ok(vec![]);
    }

    black_list.sort();

    let mut start_index = Default::default();
    if let Some(start_after) = start_after {
        let start_addr = deps.api.addr_validate(start_after.as_str())?;
        start_index = black_list
            .iter()
            .position(|addr| *addr == start_addr)
            .ok_or_else(|| {
                StdError::generic_err(format!(
                    "The {} address is not blacklisted",
                    start_addr.as_str()
                ))
            })?
            + 1; // start from the next element of the slice
    }

    // validate end index of the slice
    let end_index = (start_index + limit).min(black_list.len());

    Ok(black_list[start_index..end_index].to_vec())
}

/// ## Description
/// Return a user's lock information using a [`LockInfoResponse`] struct.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **user** is an object of type [`String`]. This is the address of the user for which we return lock information.
fn get_user_lock_info(deps: Deps, env: Env, user: String) -> StdResult<LockInfoResponse> {
    let addr = deps.api.addr_validate(&user)?;
    if let Some(lock) = LOCKED.may_load(deps.storage, addr.clone())? {
        let cur_period = get_period(env.block.time.seconds())?;
        let slope = fetch_last_checkpoint(deps, &addr, cur_period)?
            .map(|(_, point)| point.slope)
            .unwrap_or_default();
        let resp = LockInfoResponse {
            amount: lock.amount,
            coefficient: calc_coefficient(lock.end - lock.last_extend_lock_period),
            start: lock.start,
            end: lock.end,
            slope,
        };
        Ok(resp)
    } else {
        Err(StdError::generic_err("User is not found"))
    }
}

/// ## Description
/// Return early withdraw amount for a given user.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user** is an object of type [`String`].
fn get_early_withdraw_amount(deps: Deps, env: Env, user: String) -> StdResult<Uint128> {
    let user = deps.api.addr_validate(&user)?;
    let lock = LOCKED.may_load(deps.storage, user)?;

    let cur_period = get_period(env.block.time.seconds())?;
    match lock {
        None => Ok(Uint128::zero()),
        Some(lock) if lock.end <= cur_period => Ok(lock.amount),
        Some(lock) => {
            let config = CONFIG.load(deps.storage)?;

            let (_, return_amount) = calc_early_withdraw_amount(
                config.max_exit_penalty,
                lock.end - cur_period,
                lock.amount,
            );

            Ok(return_amount)
        }
    }
}

/// ## Description
/// Return a user's staked xASTRO amount at a given block height.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **user** is an object of type String. This is the address of the user for which we return lock information.
///
/// * **block_height** is an object of type u64. This is the block height at which we return the staked xASTRO amount.
fn get_user_deposit_at_height(deps: Deps, user: String, block_height: u64) -> StdResult<Uint128> {
    let addr = deps.api.addr_validate(&user)?;
    let locked_opt = LOCKED.may_load_at_height(deps.storage, addr, block_height)?;
    if let Some(lock) = locked_opt {
        Ok(lock.amount)
    } else {
        Ok(Uint128::zero())
    }
}

/// ## Description
/// Calculates a user's voting power at a given timestamp.
/// If time is None, then it calculates the user's voting power at the current block.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user** is an object of type String. This is the user/staker for which we fetch the current voting power (vxASTRO balance).
///
/// * **time** is an [`Option`] of type [`u64`]. This is the timestamp at which to fetch the user's voting power (vxASTRO balance).
fn get_user_voting_power(
    deps: Deps,
    env: Env,
    user: String,
    time: Option<u64>,
) -> StdResult<VotingPowerResponse> {
    let period = get_period(time.unwrap_or_else(|| env.block.time.seconds()))?;
    get_user_voting_power_at_period(deps, user, period)
}

/// ## Description
/// Calculates a user's voting power at a given period number.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **user** is an object of type String. This is the user/staker for which we fetch the current voting power (vxASTRO balance).
///
/// * **period** is [`u64`]. This is the period number at which to fetch the user's voting power (vxASTRO balance).
fn get_user_voting_power_at_period(
    deps: Deps,
    user: String,
    period: u64,
) -> StdResult<VotingPowerResponse> {
    let user = deps.api.addr_validate(&user)?;
    let period_key = period;

    let last_checkpoint = fetch_last_checkpoint(deps, &user, period_key)?;

    if let Some(point) = last_checkpoint.map(|(_, point)| point) {
        // The voting power point at the specified `time` was found
        let voting_power = if point.start == period {
            point.power
        } else {
            // The point before the intended period was found, thus we can calculate the user's voting power for the period we want
            calc_voting_power(&point, period)
        };
        Ok(VotingPowerResponse { voting_power })
    } else {
        // User not found
        Ok(VotingPowerResponse {
            voting_power: Uint128::zero(),
        })
    }
}

/// ## Description
/// Calculates a user's voting power at the current block.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user** is an object of type [`String`]. This is the user/staker for which we fetch the current voting power (vxASTRO balance).
fn get_user_balance(deps: Deps, env: Env, user: String) -> StdResult<BalanceResponse> {
    let vp_response = get_user_voting_power(deps, env, user, None)?;
    Ok(BalanceResponse {
        balance: vp_response.voting_power,
    })
}

/// ## Description
/// Calculates the total voting power (total vxASTRO supply) at the given timestamp.
/// If `time` is None, then it calculates the total voting power at the current block.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **time** is an [`Option`] of type [`u64`]. This is the timestamp at which we fetch the total voting power (vxASTRO supply).
fn get_total_voting_power(
    deps: Deps,
    env: Env,
    time: Option<u64>,
) -> StdResult<VotingPowerResponse> {
    let period = get_period(time.unwrap_or_else(|| env.block.time.seconds()))?;
    get_total_voting_power_at_period(deps, env, period)
}

/// ## Description
/// Calculates the total voting power (total vxASTRO supply) at the given period number.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **period** is [`u64`]. This is the period number at which we fetch the total voting power (vxASTRO supply).
fn get_total_voting_power_at_period(
    deps: Deps,
    env: Env,
    period: u64,
) -> StdResult<VotingPowerResponse> {
    let period_key = period;

    let last_checkpoint = fetch_last_checkpoint(deps, &env.contract.address, period_key)?;

    let point = last_checkpoint.map_or(
        Point {
            power: Uint128::zero(),
            start: period,
            end: period,
            slope: Default::default(),
        },
        |(_, point)| point,
    );

    let voting_power = if point.start == period {
        point.power
    } else {
        let scheduled_slope_changes = fetch_slope_changes(deps, point.start, period)?;
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

/// ## Description
/// Fetch the vxASTRO token information, such as the token name, symbol, decimals and total supply (total voting power).
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
fn query_token_info(deps: Deps, env: Env) -> StdResult<TokenInfoResponse> {
    let info = TOKEN_INFO.load(deps.storage)?;
    let total_vp = get_total_voting_power(deps, env, None)?;
    let res = TokenInfoResponse {
        name: info.name,
        symbol: info.symbol,
        decimals: info.decimals,
        total_supply: total_vp.voting_power,
    };
    Ok(res)
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(mut deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_str() {
        "voting-escrow" => match contract_version.version.as_ref() {
            "1.0.0" => {
                // 1.0.0 -> 1.1.0
                MigrationV110::migrate(deps.branch(), env, msg)?;
            }
            "1.1.0" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
