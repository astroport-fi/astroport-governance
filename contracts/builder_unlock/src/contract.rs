#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_storage_plus::Bound;

use astroport_governance::builder_unlock::msg::{
    AllocationResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg, SimulateWithdrawResponse,
    StateResponse,
};
use astroport_governance::builder_unlock::{AllocationParams, AllocationStatus, Config, Schedule};
use astroport_governance::{DEFAULT_LIMIT, MAX_LIMIT};

use crate::astroport::asset::addr_opt_validate;
use crate::astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use crate::contract::helpers::{compute_unlocked_amount, compute_withdraw_amount};
use crate::state::{CONFIG, OWNERSHIP_PROPOSAL, PARAMS, STATE, STATUS};

// Version and name used for contract migration.
const CONTRACT_NAME: &str = "builder-unlock";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters in the `msg` variable.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &Default::default())?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            astro_token: deps.api.addr_validate(&msg.astro_token)?,
            max_allocations_amount: msg.max_allocations_amount,
        },
    )?;
    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::Receive(cw20_msg)** Parse incoming messages coming from the ASTRO token contract.
///
/// * **ExecuteMsg::Withdraw** Withdraw unlocked ASTRO.
///
/// * **ExecuteMsg::TransferOwnership** Transfer contract ownership.
///
/// * **ExecuteMsg::ProposeNewReceiver** Propose a new receiver for a specific ASTRO unlock schedule.
///
/// * **ExecuteMsg::DropNewReceiver** Drop the proposal to change the receiver for an unlock schedule.
///
/// * **ExecuteMsg::ClaimReceiver**  Claim the position as a receiver for a specific unlock schedule.
///
/// * **ExecuteMsg::IncreaseAllocation** Increase ASTRO allocation for receiver.
///
/// * **ExecuteMsg::DecreaseAllocation** Decrease ASTRO allocation for receiver.
///
/// * **ExecuteMsg::TransferUnallocated** Transfer unallocated tokens.
///
/// * **ExecuteMsg::ProposeNewOwner** Creates a new request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership** Claims contract ownership.
///
/// * **ExecuteMsg::UpdateConfig** Update contract configuration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(cw20_msg) => execute_receive_cw20(deps, info, cw20_msg),
        ExecuteMsg::Withdraw {} => execute_withdraw(deps, env, info),
        ExecuteMsg::ProposeNewReceiver { new_receiver } => {
            execute_propose_new_receiver(deps, info, new_receiver)
        }
        ExecuteMsg::DropNewReceiver {} => execute_drop_new_receiver(deps, info),
        ExecuteMsg::ClaimReceiver { prev_receiver } => {
            execute_claim_receiver(deps, info, prev_receiver)
        }
        ExecuteMsg::IncreaseAllocation { receiver, amount } => {
            let config = CONFIG.load(deps.storage)?;
            if info.sender != config.owner {
                return Err(StdError::generic_err(
                    "Only the contract owner can increase allocations",
                ));
            }
            execute_increase_allocation(deps, &config, receiver, amount, None)
        }
        ExecuteMsg::DecreaseAllocation { receiver, amount } => {
            execute_decrease_allocation(deps, env, info, receiver, amount)
        }
        ExecuteMsg::TransferUnallocated { amount, recipient } => {
            execute_transfer_unallocated(deps, info, amount, recipient)
        }
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
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
        }
        ExecuteMsg::UpdateConfig {
            new_max_allocations_amount,
        } => update_config(deps, info, new_max_allocations_amount),
        ExecuteMsg::UpdateUnlockSchedules {
            new_unlock_schedules,
        } => update_unlock_schedules(deps, env, info, new_unlock_schedules),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** CW20 message to process.
fn execute_receive_cw20(
    deps: DepsMut,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    match from_binary(&cw20_msg.msg)? {
        ReceiveMsg::CreateAllocations { allocations } => execute_create_allocations(
            deps,
            cw20_msg.sender,
            info.sender,
            cw20_msg.amount,
            allocations,
        ),
        ReceiveMsg::IncreaseAllocation { user, amount } => {
            let config = CONFIG.load(deps.storage)?;

            if config.astro_token != info.sender {
                return Err(StdError::generic_err("Only ASTRO can be deposited"));
            }
            if deps.api.addr_validate(&cw20_msg.sender)? != config.owner {
                return Err(StdError::generic_err(
                    "Only the contract owner can increase allocations",
                ));
            }

            execute_increase_allocation(deps, &config, user, amount, Some(cw20_msg.amount))
        }
    }
}

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::Config {}** Return the contract configuration.
///
/// * **QueryMsg::State {}** Return the contract state (number of ASTRO that still need to be withdrawn).
///
/// * **QueryMsg::Allocation {}** Return the allocation details for a specific account.
///
/// * **QueryMsg::UnlockedTokens {}** Return the amount of unlocked ASTRO for a specific account.
///
/// * **QueryMsg::SimulateWithdraw {}** Return the result of a withdrawal simulation.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::Allocation { account } => to_binary(&query_allocation(deps, account)?),
        QueryMsg::UnlockedTokens { account } => {
            to_binary(&query_tokens_unlocked(deps, env, account)?)
        }
        QueryMsg::SimulateWithdraw { account, timestamp } => {
            to_binary(&query_simulate_withdraw(deps, env, account, timestamp)?)
        }
        QueryMsg::Allocations { start_after, limit } => {
            to_binary(&query_allocations(deps, start_after, limit)?)
        }
    }
}

/// Admin function facilitating the creation of new allocations.
///
/// * **creator** allocations creator (the contract admin).
///
/// * **deposit_token** token being deposited (should be ASTRO).
///
/// * **deposit_amount** tokens sent along with the call (should equal the sum of allocation amounts)
///
/// * **deposit_amount** new allocations being created.
fn execute_create_allocations(
    deps: DepsMut,
    creator: String,
    deposit_token: Addr,
    deposit_amount: Uint128,
    allocations: Vec<(String, AllocationParams)>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    if deps.api.addr_validate(&creator)? != config.owner {
        return Err(StdError::generic_err(
            "Only the contract owner can create allocations",
        ));
    }

    if deposit_token != config.astro_token {
        return Err(StdError::generic_err("Only ASTRO can be deposited"));
    }

    if deposit_amount
        != allocations
            .iter()
            .map(|params| params.1.amount)
            .sum::<Uint128>()
    {
        return Err(StdError::generic_err("ASTRO deposit amount mismatch"));
    }

    state.total_astro_deposited += deposit_amount;
    state.remaining_astro_tokens += deposit_amount;

    if state.total_astro_deposited > config.max_allocations_amount {
        return Err(StdError::generic_err(format!(
            "The total allocation for all recipients cannot exceed total ASTRO amount allocated to unlock (currently {} ASTRO)",
            config.max_allocations_amount,
        )));
    }

    for (user_unchecked, params) in allocations {
        params.validate(&user_unchecked)?;
        let user = deps.api.addr_validate(&user_unchecked)?;

        if PARAMS.has(deps.storage, &user) {
            return Err(StdError::generic_err(format!(
                "Allocation (params) already exists for {user}"
            )));
        }
        PARAMS.save(deps.storage, &user, &params)?;
        STATUS.save(deps.storage, &user, &AllocationStatus::new())?;
    }

    STATE.save(deps.storage, &state)?;
    Ok(Response::default())
}

/// Allow allocation recipients to withdraw unlocked ASTRO.
fn execute_withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    let params = PARAMS.load(deps.storage, &info.sender)?;

    if params.proposed_receiver.is_some() {
        return Err(StdError::generic_err(
            "You may not withdraw once you proposed new receiver!",
        ));
    }

    let mut status = STATUS.load(deps.storage, &info.sender)?;

    let SimulateWithdrawResponse { astro_to_withdraw } =
        compute_withdraw_amount(env.block.time.seconds(), &params, &status);

    if astro_to_withdraw.is_zero() {
        return Err(StdError::generic_err("No unlocked ASTRO to be withdrawn"));
    }

    status.astro_withdrawn += astro_to_withdraw;
    state.remaining_astro_tokens -= astro_to_withdraw;

    // SAVE :: state & allocation
    STATE.save(deps.storage, &state)?;

    // Update status
    STATUS.save(deps.storage, &info.sender, &status)?;

    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: config.astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: astro_to_withdraw,
            })?,
            funds: vec![],
        })
        .add_attribute("astro_withdrawn", astro_to_withdraw))
}

/// Allows the current allocation receiver to propose a new receiver.
///
/// * **new_receiver** new proposed receiver for the allocation.
fn execute_propose_new_receiver(
    deps: DepsMut,
    info: MessageInfo,
    new_receiver: String,
) -> StdResult<Response> {
    let mut alloc_params = PARAMS.load(deps.storage, &info.sender)?;
    let new_receiver = deps.api.addr_validate(&new_receiver)?;

    match alloc_params.proposed_receiver {
        Some(proposed_receiver) => {
            return Err(StdError::generic_err(format!(
                "Proposed receiver already set to {proposed_receiver}"
            )));
        }
        None => {
            if PARAMS.has(deps.storage, &new_receiver) {
                return Err(StdError::generic_err(
                    "Invalid new_receiver. Proposed receiver already has an ASTRO allocation",
                ));
            }

            alloc_params.proposed_receiver = Some(new_receiver.clone());
            PARAMS.save(deps.storage, &info.sender, &alloc_params)?;
        }
    }

    Ok(Response::new()
        .add_attribute("action", "ProposeNewReceiver")
        .add_attribute("proposed_receiver", new_receiver))
}

/// Drop the new proposed receiver for a specific allocation.
fn execute_drop_new_receiver(deps: DepsMut, info: MessageInfo) -> StdResult<Response> {
    let mut alloc_params = PARAMS.load(deps.storage, &info.sender)?;

    match alloc_params.proposed_receiver {
        Some(proposed_receiver) => {
            alloc_params.proposed_receiver = None;
            PARAMS.save(deps.storage, &info.sender, &alloc_params)?;

            Ok(Response::new()
                .add_attribute("action", "DropNewReceiver")
                .add_attribute("dropped_proposed_receiver", proposed_receiver))
        }
        None => Err(StdError::generic_err("Proposed receiver not set")),
    }
}

/// Decrease an address' ASTRO allocation.
///
/// * **receiver** address that will have its allocation decreased.
///
/// * **amount** ASTRO amount to decrease the allocation by.
fn execute_decrease_allocation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    receiver: String,
    amount: Uint128,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(StdError::generic_err(
            "Only the contract owner can decrease allocations",
        ));
    }

    let receiver = deps.api.addr_validate(&receiver)?;

    let mut state = STATE.load(deps.storage)?;
    let mut params = PARAMS.load(deps.storage, &receiver)?;
    let mut status = STATUS.load(deps.storage, &receiver)?;

    let unlocked_amount = compute_unlocked_amount(
        env.block.time.seconds(),
        params.amount,
        &params.unlock_schedule,
        status.unlocked_amount_checkpoint,
    );
    let locked_amount = params.amount - unlocked_amount;

    if locked_amount < amount {
        return Err(StdError::generic_err(format!(
            "Insufficient amount of lock to decrease allocation, user has locked {locked_amount} ASTRO."
        )));
    }

    params.amount = params.amount.checked_sub(amount)?;
    status.unlocked_amount_checkpoint = unlocked_amount;
    state.unallocated_tokens = state.unallocated_tokens.checked_add(amount)?;
    state.remaining_astro_tokens = state.remaining_astro_tokens.checked_sub(amount)?;

    STATUS.save(deps.storage, &receiver, &status)?;
    PARAMS.save(deps.storage, &receiver, &params)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "execute_decrease_allocation"),
        attr("receiver", receiver),
        attr("amount", amount),
    ]))
}

/// Increase an address' ASTRO allocation.
///
/// * **receiver** address that will have its allocation incrased.
///
/// * **amount** ASTRO amount to increase the allocation by.
///
/// * **deposit_amount** is amount of ASTRO to increase the allocation by using CW20 Receive.
fn execute_increase_allocation(
    deps: DepsMut,
    config: &Config,
    receiver: String,
    amount: Uint128,
    deposit_amount: Option<Uint128>,
) -> StdResult<Response> {
    let receiver = deps.api.addr_validate(&receiver)?;

    match PARAMS.may_load(deps.storage, &receiver)? {
        Some(mut params) => {
            let mut state = STATE.load(deps.storage)?;

            if let Some(deposit_amount) = deposit_amount {
                state.total_astro_deposited =
                    state.total_astro_deposited.checked_add(deposit_amount)?;
                state.unallocated_tokens = state.unallocated_tokens.checked_add(deposit_amount)?;

                if state.total_astro_deposited > config.max_allocations_amount {
                    return Err(StdError::generic_err(format!(
                        "The total allocation for all recipients cannot exceed total ASTRO amount allocated to unlock (currently {} ASTRO)",
                        config.max_allocations_amount,
                    )));
                }
            }

            if state.unallocated_tokens < amount {
                return Err(StdError::generic_err(format!(
                    "Insufficient unallocated ASTRO to increase allocation. Contract has: {} unallocated ASTRO.",
                    state.unallocated_tokens
                )));
            }

            params.amount = params.amount.checked_add(amount)?;
            state.unallocated_tokens = state.unallocated_tokens.checked_sub(amount)?;
            state.remaining_astro_tokens = state.remaining_astro_tokens.checked_add(amount)?;

            PARAMS.save(deps.storage, &receiver, &params)?;
            STATE.save(deps.storage, &state)?;
        }
        None => {
            return Err(StdError::generic_err("Proposed receiver not set"));
        }
    }

    Ok(Response::new()
        .add_attribute("action", "execute_increase_allocation")
        .add_attribute("amount", amount)
        .add_attribute("receiver", receiver))
}

/// Transfer unallocated ASTRO tokens to a recipient.
///
/// * **amount** amount ASTRO to transfer.
///
/// * **recipient** transfer recipient.
fn execute_transfer_unallocated(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
    recipient: Option<String>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    if config.owner != info.sender {
        return Err(StdError::generic_err(
            "Only contract owner can transfer unallocated ASTRO.",
        ));
    }

    let mut state = STATE.load(deps.storage)?;

    if state.unallocated_tokens < amount {
        return Err(StdError::generic_err(format!(
            "Insufficient unallocated ASTRO to transfer. Contract has: {} unallocated ASTRO.",
            state.unallocated_tokens
        )));
    }

    state.unallocated_tokens = state.unallocated_tokens.checked_sub(amount)?;
    state.total_astro_deposited = state.total_astro_deposited.checked_sub(amount)?;

    let recipient = addr_opt_validate(deps.api, &recipient)?.unwrap_or_else(|| info.sender.clone());
    let msg = WasmMsg::Execute {
        contract_addr: config.astro_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: recipient.to_string(),
            amount,
        })?,
        funds: vec![],
    };

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "execute_transfer_unallocated")
        .add_attribute("amount", amount)
        .add_message(msg))
}

/// Allows a newly proposed allocation receiver to claim the ownership of that allocation.
///
/// * **prev_receiver** this is the previous receiver for the allocation.
fn execute_claim_receiver(
    deps: DepsMut,
    info: MessageInfo,
    prev_receiver: String,
) -> StdResult<Response> {
    let prev_receiver_addr = deps.api.addr_validate(&prev_receiver)?;
    let mut alloc_params = PARAMS.load(deps.storage, &prev_receiver_addr)?;

    match alloc_params.proposed_receiver {
        Some(proposed_receiver) => {
            if proposed_receiver == info.sender {
                if let Some(sender_params) = PARAMS.may_load(deps.storage, &info.sender)? {
                    return Err(StdError::generic_err(format!(
                        "The proposed receiver already has an ASTRO allocation of {} ASTRO, that ends at {}",
                        sender_params.amount,
                        sender_params.unlock_schedule.start_time + sender_params.unlock_schedule.duration + sender_params.unlock_schedule.cliff,
                    )));
                }

                // Transfers allocation parameters
                // 1. Save the allocation for the new receiver
                alloc_params.proposed_receiver = None;
                PARAMS.save(deps.storage, &info.sender, &alloc_params)?;
                // 2. Remove the allocation info from the previous owner
                PARAMS.remove(deps.storage, &prev_receiver_addr);
                // Transfers Allocation Status
                let status = STATUS.load(deps.storage, &prev_receiver_addr)?;

                STATUS.save(deps.storage, &info.sender, &status)?;
                STATUS.remove(deps.storage, &prev_receiver_addr)
            } else {
                return Err(StdError::generic_err(format!(
                    "Proposed receiver mismatch, actual proposed receiver : {proposed_receiver}"
                )));
            }
        }
        None => {
            return Err(StdError::generic_err("Proposed receiver not set"));
        }
    }

    Ok(Response::new().add_attributes(vec![
        attr("action", "ClaimReceiver"),
        attr("prev_receiver", prev_receiver),
        attr("receiver", info.sender),
    ]))
}

/// Updates builder unlock contract parameters.
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_max_allocations_amount: Uint128,
) -> StdResult<Response> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(StdError::generic_err(
            "Only the contract owner can change config",
        ));
    }

    let state = STATE.load(deps.storage)?;

    if new_max_allocations_amount < state.total_astro_deposited {
        return Err(StdError::generic_err(format!(
            "The new max allocations amount {} can not be less than currently deposited {}",
            new_max_allocations_amount, state.total_astro_deposited,
        )));
    }

    config.max_allocations_amount = new_max_allocations_amount;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute("new_max_allocations_amount", new_max_allocations_amount))
}

/// Updates builder unlock schedules for specified accounts.
fn update_unlock_schedules(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_unlock_schedules: Vec<(String, Schedule)>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(StdError::generic_err(
            "Only the contract owner can change config",
        ));
    }

    for (account, new_schedule) in new_unlock_schedules {
        let account_addr = deps.api.addr_validate(&account)?;
        let mut params = PARAMS.load(deps.storage, &account_addr)?;

        let mut status = STATUS.load(deps.storage, &account_addr)?;

        let unlocked_amount_checkpoint = compute_unlocked_amount(
            env.block.time.seconds(),
            params.amount,
            &params.unlock_schedule,
            status.unlocked_amount_checkpoint,
        );

        if unlocked_amount_checkpoint > status.unlocked_amount_checkpoint {
            status.unlocked_amount_checkpoint = unlocked_amount_checkpoint;
            STATUS.save(deps.storage, &account_addr, &status)?;
        }

        params.update_schedule(new_schedule, &account)?;
        PARAMS.save(deps.storage, &account_addr, &params)?;
    }

    Ok(Response::new().add_attribute("action", "update_unlock_schedules"))
}

/// Return the global distribution state.
pub fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(StateResponse {
        total_astro_deposited: state.total_astro_deposited,
        remaining_astro_tokens: state.remaining_astro_tokens,
        unallocated_astro_tokens: state.unallocated_tokens,
    })
}

/// Return information about a specific allocation.
///
/// * **account** account whose allocation we query.
fn query_allocation(deps: Deps, account: String) -> StdResult<AllocationResponse> {
    let account_checked = deps.api.addr_validate(&account)?;

    Ok(AllocationResponse {
        params: PARAMS
            .may_load(deps.storage, &account_checked)?
            .unwrap_or_default(),
        status: STATUS
            .may_load(deps.storage, &account_checked)?
            .unwrap_or_default(),
    })
}

/// Return information about a specific allocation.
///
/// * **start_after** account from which to start querying.
///
/// * **limit** max amount of entries to return.
fn query_allocations(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<(Addr, AllocationParams)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let default_start;

    let start = if let Some(start_after) = start_after {
        default_start = deps.api.addr_validate(&start_after)?;
        Some(Bound::exclusive(&default_start))
    } else {
        None
    };

    PARAMS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .collect()
}

/// Return the total amount of unlocked tokens for a specific account.
///
/// * **account** account whose unlocked token amount we query.
fn query_tokens_unlocked(deps: Deps, env: Env, account: String) -> StdResult<Uint128> {
    let account_checked = deps.api.addr_validate(&account)?;

    let params = PARAMS.load(deps.storage, &account_checked)?;
    let status = STATUS.load(deps.storage, &account_checked)?;

    Ok(compute_unlocked_amount(
        env.block.time.seconds(),
        params.amount,
        &params.unlock_schedule,
        status.unlocked_amount_checkpoint,
    ))
}

/// Simulate a token withdrawal.
///
/// * **account** account for which we simulate a withdrawal.
///
/// * **timestamp** timestamp where we assume the account would withdraw.
fn query_simulate_withdraw(
    deps: Deps,
    env: Env,
    account: String,
    timestamp: Option<u64>,
) -> StdResult<SimulateWithdrawResponse> {
    let account_checked = deps.api.addr_validate(&account)?;

    let params = PARAMS.load(deps.storage, &account_checked)?;
    let status = STATUS.load(deps.storage, &account_checked)?;

    Ok(compute_withdraw_amount(
        timestamp.unwrap_or_else(|| env.block.time.seconds()),
        &params,
        &status,
    ))
}

//----------------------------------------------------------------------------------------
// Helper Functions
//----------------------------------------------------------------------------------------

mod helpers {
    use cosmwasm_std::Uint128;

    use astroport_governance::builder_unlock::msg::SimulateWithdrawResponse;
    use astroport_governance::builder_unlock::{AllocationParams, AllocationStatus, Schedule};

    /// Computes number of tokens that are now unlocked for a given allocation
    pub fn compute_unlocked_amount(
        timestamp: u64,
        amount: Uint128,
        schedule: &Schedule,
        unlock_checkpoint: Uint128,
    ) -> Uint128 {
        // Tokens haven't begun unlocking
        if timestamp < schedule.start_time + schedule.cliff {
            unlock_checkpoint
        } else if (timestamp < schedule.start_time + schedule.duration) && schedule.duration != 0 {
            // If percent_at_cliff is set, then this amount should be unlocked at cliff.
            // The rest of tokens are vested linearly between cliff and end_time
            let unlocked_amount = if let Some(percent_at_cliff) = schedule.percent_at_cliff {
                let amount_at_cliff = amount * percent_at_cliff;

                amount_at_cliff
                    + amount.saturating_sub(amount_at_cliff).multiply_ratio(
                        timestamp - schedule.start_time - schedule.cliff,
                        schedule.duration - schedule.cliff,
                    )
            } else {
                // Tokens unlock linearly between start time and end time
                amount.multiply_ratio(timestamp - schedule.start_time, schedule.duration)
            };

            if unlocked_amount > unlock_checkpoint {
                unlocked_amount
            } else {
                unlock_checkpoint
            }
        }
        // After end time, all tokens are fully unlocked
        else {
            amount
        }
    }

    /// Computes number of tokens that are withdrawable for a given allocation
    pub fn compute_withdraw_amount(
        timestamp: u64,
        params: &AllocationParams,
        status: &AllocationStatus,
    ) -> SimulateWithdrawResponse {
        // "Unlocked" amount
        let astro_unlocked = compute_unlocked_amount(
            timestamp,
            params.amount,
            &params.unlock_schedule,
            status.unlocked_amount_checkpoint,
        );

        // Withdrawal amount is unlocked amount minus the amount already withdrawn
        let astro_withdrawable = astro_unlocked - status.astro_withdrawn;

        SimulateWithdrawResponse {
            astro_to_withdraw: astro_withdrawable,
        }
    }
}
