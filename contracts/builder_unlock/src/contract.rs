#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use astroport_governance::builder_unlock::msg::{
    AllocationResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg, SimulateWithdrawResponse,
    StateResponse,
};
use astroport_governance::builder_unlock::{AllocationParams, AllocationStatus, Config};

use crate::state::{CONFIG, PARAMS, STATE, STATUS};

// Version and name used for contract migration.
const CONTRACT_NAME: &str = "builder-unlock";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in the `msg` variable.
/// Returns a [`Response`] with the specified attributes if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`]
///
/// * **_info** is an object of type [`MessageInfo`]
///
/// * **msg**  is a message of type [`InstantiateMsg`] which contains the parameters used for creating a contract.
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
            owner: deps.api.addr_validate(&msg.owner)?,
            astro_token: deps.api.addr_validate(&msg.astro_token)?,
        },
    )?;
    Ok(Response::default())
}

/// ## Description
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
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(cw20_msg) => execute_receive_cw20(deps, env, info, cw20_msg),
        ExecuteMsg::Withdraw {} => execute_withdraw(deps, env, info),
        ExecuteMsg::TransferOwnership { new_owner } => {
            execute_transfer_ownership(deps, env, info, new_owner)
        }
        ExecuteMsg::ProposeNewReceiver { new_receiver } => {
            execute_propose_new_receiver(deps, env, info, new_receiver)
        }
        ExecuteMsg::DropNewReceiver {} => execute_drop_new_receiver(deps, env, info),
        ExecuteMsg::ClaimReceiver { prev_receiver } => {
            execute_claim_receiver(deps, env, info, prev_receiver)
        }
    }
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
fn execute_receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    match from_binary(&cw20_msg.msg)? {
        ReceiveMsg::CreateAllocations { allocations } => execute_create_allocations(
            deps,
            env,
            info.clone(),
            cw20_msg.sender,
            info.sender,
            cw20_msg.amount,
            allocations,
        ),
    }
}

/// # Description
/// Expose available contract queries.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Return the contract configuration.
///
/// * **QueryMsg::State {}** Return the contract state (number of ASTRO that still need to be withdrawn).
///
/// * **QueryMsg::Allocation {}** Return the allocation details for a specific account.
///
/// * **QueryMsg::UnlockedTokens {}** Return the amoint of unlocked ASTRO for a specific account.
///
/// * **QueryMsg::SimulateWithdraw {}** Return the result of a withdrawal simulation.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps, env)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::Allocation { account } => to_binary(&query_allocation(deps, env, account)?),
        QueryMsg::UnlockedTokens { account } => {
            to_binary(&query_tokens_unlocked(deps, env, account)?)
        }
        QueryMsg::SimulateWithdraw { account, timestamp } => {
            to_binary(&query_simulate_withdraw(deps, env, account, timestamp)?)
        }
    }
}

/// # Description
/// Admin function facilitating creation of new allocations.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **creator** is an object of type [`String`]. This is the allocations creator (the contract admin).
///
/// * **deposit_token** is an object of type [`Addr`]. This is the token being deposited (should be ASTRO).
///
/// * **deposit_amount** is an object of type [`Uint128`]. This is the of tokens sent along with the call (should equal the sum of allocation amounts)
///
/// * **deposit_amount** is a vector of tuples of type [(`String`, `AllocationParams`)]. New allocations being created.
fn execute_create_allocations(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    creator: String,
    deposit_token: Addr,
    deposit_amount: Uint128,
    allocations: Vec<(String, AllocationParams)>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.may_load(deps.storage)?.unwrap_or_default();

    if deps.api.addr_validate(&creator)? != config.owner {
        return Err(StdError::generic_err(
            "Only the contract owner can create allocations",
        ));
    }

    if deposit_token != config.astro_token {
        return Err(StdError::generic_err("Only ASTRO can be deposited"));
    }

    if deposit_amount != allocations.iter().map(|params| params.1.amount).sum() {
        return Err(StdError::generic_err("ASTRO deposit amount mismatch"));
    }

    state.total_astro_deposited += deposit_amount;
    state.remaining_astro_tokens += deposit_amount;

    for allocation in allocations {
        let (user_unchecked, params) = allocation;

        let user = deps.api.addr_validate(&user_unchecked)?;

        match PARAMS.load(deps.storage, &user) {
            Ok(..) => {
                return Err(StdError::generic_err(format!(
                    "Allocation (params) already exists for {}",
                    user
                )));
            }
            Err(..) => {
                PARAMS.save(deps.storage, &user, &params)?;
            }
        }

        match STATUS.load(deps.storage, &user) {
            Ok(..) => {
                return Err(StdError::generic_err(format!(
                    "Allocation (status) already exists for {}",
                    user
                )));
            }
            Err(..) => {
                STATUS.save(deps.storage, &user, &AllocationStatus::new())?;
            }
        }
    }

    STATE.save(deps.storage, &state)?;
    Ok(Response::default())
}

/// # Description
/// Allow allocation recipients to withdraw unlocked ASTRO.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
fn execute_withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.may_load(deps.storage)?.unwrap_or_default();

    let params = PARAMS.load(deps.storage, &info.sender)?;
    let mut status = STATUS.load(deps.storage, &info.sender)?;

    let SimulateWithdrawResponse { astro_to_withdraw } =
        helpers::compute_withdraw_amount(env.block.time.seconds(), &params, &mut status);

    state.remaining_astro_tokens -= astro_to_withdraw;

    // SAVE :: state & allocation
    STATE.save(deps.storage, &state)?;

    // Update status
    STATUS.save(deps.storage, &info.sender, &status)?;

    let mut msgs: Vec<WasmMsg> = vec![];

    if astro_to_withdraw.is_zero() {
        return Err(StdError::generic_err("No unlocked ASTRO to be withdrawn"));
    }

    msgs.push(WasmMsg::Execute {
        contract_addr: config.astro_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.to_string(),
            amount: astro_to_withdraw,
        })?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_messages(msgs)
        .add_attribute("astro_withdrawn", astro_to_withdraw))
}

/// # Description
/// Transfer contract ownership.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **new_owner** is an [`Option`] of type [`String`]. This is the newly proposed owner.
fn execute_transfer_ownership(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_owner: Option<String>,
) -> StdResult<Response> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(StdError::generic_err(
            "Only the current owner can transfer ownership",
        ));
    }

    if new_owner.is_some() {
        config.owner = deps.api.addr_validate(&new_owner.unwrap())?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

/// # Description
/// Allows the current allocation receiver to propose a new receiver/.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **new_receiver** is an object of type [`String`]. Newly proposed receiver for the allocation.
fn execute_propose_new_receiver(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_receiver: String,
) -> StdResult<Response> {
    let mut alloc_params = PARAMS.load(deps.storage, &info.sender)?;

    match alloc_params.proposed_receiver {
        Some(proposed_receiver) => {
            return Err(StdError::generic_err(format!(
                "Proposed receiver already set to {}",
                proposed_receiver
            )));
        }
        None => {
            let alloc_params_new_receiver = PARAMS
                .may_load(deps.storage, &deps.api.addr_validate(&new_receiver)?)?
                .unwrap_or_default();
            if !alloc_params_new_receiver.amount.is_zero() {
                return Err(StdError::generic_err(format!(
                "Invalid new_receiver. Proposed receiver already has an ASTRO allocation of {} ASTRO",alloc_params_new_receiver.amount
            )));
            }

            alloc_params.proposed_receiver = Some(deps.api.addr_validate(&new_receiver)?);
            PARAMS.save(deps.storage, &info.sender, &alloc_params)?;
        }
    }

    Ok(Response::new()
        .add_attribute("action", "ProposeNewReceiver")
        .add_attribute("proposed_receiver", new_receiver))
}

/// # Description
/// Drop the newly proposed receiver for a specific allocation.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
fn execute_drop_new_receiver(deps: DepsMut, _env: Env, info: MessageInfo) -> StdResult<Response> {
    let mut alloc_params = PARAMS.load(deps.storage, &info.sender)?;
    let prev_proposed_receiver: Addr;

    match alloc_params.proposed_receiver {
        Some(proposed_receiver) => {
            prev_proposed_receiver = proposed_receiver;
            alloc_params.proposed_receiver = None;
            PARAMS.save(deps.storage, &info.sender, &alloc_params)?;
        }
        None => {
            return Err(StdError::generic_err("Proposed receiver not set"));
        }
    }

    Ok(Response::new()
        .add_attribute("action", "DropNewReceiver")
        .add_attribute("dropped_proposed_receiver", prev_proposed_receiver))
}

/// # Description
/// Allows a newly proposed allocation receiver to claim the ownership of that allocation.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **prev_receiver** is an object of type [`String`]. This is the previous receiver for hte allocation.
fn execute_claim_receiver(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    prev_receiver: String,
) -> StdResult<Response> {
    let mut alloc_params = PARAMS.load(deps.storage, &deps.api.addr_validate(&prev_receiver)?)?;

    match alloc_params.proposed_receiver {
        Some(proposed_receiver) => {
            if proposed_receiver == info.sender {
                // Transfers Allocation Parameters ::
                // 1. Save the allocation for the new receiver
                alloc_params.proposed_receiver = None;
                PARAMS.save(deps.storage, &info.sender, &alloc_params)?;
                // 2. Remove the allocation info from the previous owner
                PARAMS.remove(deps.storage, &deps.api.addr_validate(&prev_receiver)?);
                // Transfers Allocation Status ::
                let status = STATUS.load(deps.storage, &deps.api.addr_validate(&prev_receiver)?)?;
                STATUS.save(deps.storage, &info.sender, &status)?;
            } else {
                return Err(StdError::generic_err(format!(
                    "Proposed receiver mismatch, actual proposed receiver : {}",
                    proposed_receiver
                )));
            }
        }
        None => {
            return Err(StdError::generic_err("Proposed receiver not set"));
        }
    }

    Ok(Response::new()
        .add_attribute("action", "ClaimReceiver")
        .add_attribute("prev_receiver", prev_receiver)
        .add_attribute("new_receiver", info.sender.to_string()))
}

/// # Description
/// Return the contract configuration.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
fn query_config(deps: Deps, _env: Env) -> StdResult<Config> {
    CONFIG.load(deps.storage)
}

/// # Description
/// Return the global distribution state.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
pub fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let state = STATE.may_load(deps.storage)?.unwrap_or_default();
    Ok(StateResponse {
        total_astro_deposited: state.total_astro_deposited,
        remaining_astro_tokens: state.remaining_astro_tokens,
    })
}

/// # Description
/// Return information about a specific allocation.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **account** is an object of type [`String`]. This is the account whose allocation we query.
fn query_allocation(deps: Deps, _env: Env, account: String) -> StdResult<AllocationResponse> {
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

/// # Description
/// Return the total amount of unlocked tokens for a specific account.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **account** is an object of type [`String`]. This is the account whose unlocked token amount we query.
fn query_tokens_unlocked(deps: Deps, env: Env, account: String) -> StdResult<Uint128> {
    let account_checked = deps.api.addr_validate(&account)?;

    let params = PARAMS.load(deps.storage, &account_checked)?;

    Ok(helpers::compute_unlocked_amount(
        env.block.time.seconds(),
        params.amount,
        &params.unlock_schedule,
    ))
}

/// # Description
/// Simulate a token withdrawal.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **account** is an object of type [`String`]. This is the account for which we simulate a withdrawal.
///
/// * **timestamp** is an [`Option`] of type [`u64`]. This is the timestamp where we assume the account would withdraw.
fn query_simulate_withdraw(
    deps: Deps,
    env: Env,
    account: String,
    timestamp: Option<u64>,
) -> StdResult<SimulateWithdrawResponse> {
    let account_checked = deps.api.addr_validate(&account)?;

    let params = PARAMS.load(deps.storage, &account_checked)?;
    let mut status = STATUS.load(deps.storage, &account_checked)?;

    let timestamp_ = match timestamp {
        Some(timestamp) => timestamp,
        None => env.block.time.seconds(),
    };

    Ok(helpers::compute_withdraw_amount(
        timestamp_,
        &params,
        &mut status,
    ))
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`Empty`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}

//----------------------------------------------------------------------------------------
// Helper Functions
//----------------------------------------------------------------------------------------

mod helpers {
    use cosmwasm_std::Uint128;

    use astroport_governance::builder_unlock::msg::SimulateWithdrawResponse;
    use astroport_governance::builder_unlock::{AllocationParams, AllocationStatus, Schedule};

    // Computes number of tokens that are now unlocked for a given allocation
    pub fn compute_unlocked_amount(
        timestamp: u64,
        amount: Uint128,
        schedule: &Schedule,
    ) -> Uint128 {
        // Tokens haven't begun unlocking
        if timestamp < schedule.start_time {
            Uint128::zero()
        }
        // Tokens unlock linearly between start time and end time
        else if timestamp < schedule.start_time + schedule.duration {
            amount.multiply_ratio(timestamp - schedule.start_time, schedule.duration)
        }
        // After end time, all tokens are fully unlocked
        else {
            amount
        }
    }

    // Computes number of tokens that are withdrawable for a given allocation
    pub fn compute_withdraw_amount(
        timestamp: u64,
        params: &AllocationParams,
        status: &mut AllocationStatus,
    ) -> SimulateWithdrawResponse {
        // Before the end of cliff period, no token can be withdrawn
        if timestamp < (params.unlock_schedule.start_time + params.unlock_schedule.cliff) {
            SimulateWithdrawResponse {
                astro_to_withdraw: Uint128::zero(),
            }
        } else {
            // "Unlocked" amount
            let astro_unlocked =
                compute_unlocked_amount(timestamp, params.amount, &params.unlock_schedule);

            // Withdrawable amount is unlocked amount minus the amount already withdrawn
            let astro_withdrawable = astro_unlocked - status.astro_withdrawn;
            status.astro_withdrawn += astro_withdrawable;

            SimulateWithdrawResponse {
                astro_to_withdraw: astro_withdrawable,
            }
        }
    }
}
