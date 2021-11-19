#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use astroport_governance::astro_vesting::msg::{
    AllocationResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg, SimulateWithdrawResponse,
};
use astroport_governance::astro_vesting::{AllocationParams, AllocationStatus, Config};

use crate::state::{CONFIG, PARAMS, STATUS};

// version info for migration info
const CONTRACT_NAME: &str = "astro-vesting";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

//----------------------------------------------------------------------------------------
// Entry Points
//----------------------------------------------------------------------------------------

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
            refund_recipient: deps.api.addr_validate(&msg.refund_recipient)?,
            astro_token: deps.api.addr_validate(&msg.astro_token)?,
            default_unlock_schedule: msg.default_unlock_schedule,
        },
    )?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(cw20_msg) => execute_receive_cw20(deps, env, info, cw20_msg),
        ExecuteMsg::Withdraw {} => execute_withdraw(deps, env, info),
        ExecuteMsg::Terminate {} => execute_terminate(deps, env, info),
        ExecuteMsg::TransferOwnership {
            new_owner,
            new_refund_recipient,
        } => execute_transfer_ownership(deps, env, info, new_owner, new_refund_recipient),
        ExecuteMsg::ProposeNewReceiver { new_receiver } => {
            execute_propose_new_receiver(deps, env, info, new_receiver)
        }
        ExecuteMsg::DropNewReceiver {} => execute_drope_new_receiver(deps, env, info),
        ExecuteMsg::ClaimReceiver { prev_receiver } => {
            execute_claim_receiver(deps, env, info, prev_receiver)
        }
    }
}

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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps, env)?),
        QueryMsg::Allocation { account } => to_binary(&query_allocation(deps, env, account)?),
        QueryMsg::SimulateWithdraw { account } => {
            to_binary(&query_simulate_withdraw(deps, env, account)?)
        }
    }
}

//----------------------------------------------------------------------------------------
// Execute Points
//----------------------------------------------------------------------------------------

/// @dev Admin function facilitating creation of new Allocations
/// @params creator: Function caller address. Needs to be the admin
/// @params deposit_token: Token being deposited, should be ASTRO
/// @params deposit_amount: Number of tokens sent along-with the call, should equal the sum of allocation amount
/// @params allocations: New Allocations being created
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

    if deps.api.addr_validate(&creator)? != config.owner {
        return Err(StdError::generic_err("Only owner can create allocations"));
    }

    if deposit_token != config.astro_token {
        return Err(StdError::generic_err("Only Astro token can be deposited"));
    }

    if deposit_amount != allocations.iter().map(|params| params.1.amount).sum() {
        return Err(StdError::generic_err("Deposit amount mismatch"));
    }

    for allocation in allocations {
        let (user_unchecked, params) = allocation;

        let user = deps.api.addr_validate(&user_unchecked)?;

        match PARAMS.load(deps.storage, &user) {
            Ok(..) => {
                return Err(StdError::generic_err("Allocation already exists for user"));
            }
            Err(..) => {
                PARAMS.save(deps.storage, &user, &params)?;
            }
        }

        match STATUS.load(deps.storage, &user) {
            Ok(..) => {
                return Err(StdError::generic_err("Allocation already exists for user"));
            }
            Err(..) => {
                STATUS.save(deps.storage, &user, &AllocationStatus::new())?;
            }
        }
    }

    Ok(Response::default())
}

/// @dev Allows allocation receivers to claim their ASTRO tokens that can be withdrawn
fn execute_withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let params = PARAMS.load(deps.storage, &info.sender)?;
    let mut status = STATUS.load(deps.storage, &info.sender)?;

    let SimulateWithdrawResponse { astro_to_withdraw } = helpers::compute_withdraw_amounts(
        env.block.time.seconds(),
        &params,
        &mut status,
        config.default_unlock_schedule,
    );

    // Update status
    STATUS.save(deps.storage, &info.sender, &status)?;

    let mut msgs: Vec<WasmMsg> = vec![];

    if !astro_to_withdraw.is_zero() {
        msgs.push(WasmMsg::Execute {
            contract_addr: config.astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: astro_to_withdraw,
            })?,
            funds: vec![],
        });
    }

    Ok(Response::new()
        .add_messages(msgs)
        .add_attribute("astro_withdrawn", astro_to_withdraw))
}

/// @dev Allows allocation receivers to terminate their ASTRO allocation
fn execute_terminate(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut params = PARAMS.load(deps.storage, &info.sender)?;

    let timestamp = env.block.time.seconds();
    let astro_vested =
        helpers::compute_vested_or_unlocked_amount(timestamp, params.amount, &params.vest_schedule);

    // Refund the unvested ASTRO tokens to owner
    let astro_to_refund = params.amount - astro_vested;

    // Set the total allocation amount to the current vested amount, and vesting end time
    // to now. This will effectively end vesting and prevent more tokens to be vested
    params.amount = astro_vested;
    params.vest_schedule.duration = timestamp - params.vest_schedule.start_time;

    PARAMS.save(deps.storage, &info.sender, &params)?;

    let msg = WasmMsg::Execute {
        contract_addr: config.astro_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: config.refund_recipient.to_string(),
            amount: astro_to_refund,
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(msg)
        .add_attribute("astro_refunded", astro_to_refund)
        .add_attribute("new_amount", params.amount)
        .add_attribute(
            "new_vest_duration",
            format!("{}", params.vest_schedule.duration),
        ))
}

/// @dev Admin function to update the owner / refund_recepient addresses
fn execute_transfer_ownership(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_owner: String,
    new_refund_recipient: String,
) -> StdResult<Response> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(StdError::generic_err("Only owner can transfer ownership"));
    }

    config.owner = deps.api.addr_validate(&new_owner)?;
    config.refund_recipient = deps.api.addr_validate(&new_refund_recipient)?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

/// @dev Facilitates a user to propose the transfer of the ownership of his allocation to a new terra address.
/// @params new_receiver : Proposed terra address to which the ownership of his allocation is to be transferred
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
            alloc_params.proposed_receiver = Some(deps.api.addr_validate(&new_receiver)?);
            PARAMS.save(deps.storage, &info.sender, &alloc_params)?;
        }
    }

    Ok(Response::new()
        .add_attribute("action", "ProposeNewReceiver")
        .add_attribute("proposed_receiver", new_receiver))
}

/// @dev Facilitates a user to drop the initially proposed receiver for his allocation
fn execute_drope_new_receiver(deps: DepsMut, _env: Env, info: MessageInfo) -> StdResult<Response> {
    let mut alloc_params = PARAMS.load(deps.storage, &info.sender)?;

    match alloc_params.proposed_receiver {
        Some(_) => {
            alloc_params.proposed_receiver = None;
            PARAMS.save(deps.storage, &info.sender, &alloc_params)?;
        }
        None => {
            return Err(StdError::generic_err("Proposed receiver not set"));
        }
    }

    Ok(Response::new()
        .add_attribute("action", "DropNewReceiver")
        .add_attribute(
            "dropped_proposed_receiver",
            alloc_params.proposed_receiver.unwrap(),
        ))
}

/// @dev Allows a proposed receiver of an auction to claim the ownership of that auction
/// @params prev_receiver : User who proposed the info.sender as the proposed terra address to which the ownership of his allocation is to be transferred
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
                // 1. Save the allocation against the new receiver
                alloc_params.proposed_receiver = None;
                PARAMS.save(deps.storage, &info.sender, &alloc_params)?;
                // 2. Remove the allocation info of previous owner
                PARAMS.remove(deps.storage, &deps.api.addr_validate(&prev_receiver)?);
                // Transfers Allocation Status ::
                let status = STATUS.load(deps.storage, &deps.api.addr_validate(&prev_receiver)?)?;
                STATUS.save(deps.storage, &info.sender, &status)?;
            } else {
                return Err(StdError::generic_err(format!(
                    "Proposed receiver mismatch, Proposed receiver : {}",
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

//----------------------------------------------------------------------------------------
// Query Functions
//----------------------------------------------------------------------------------------

fn query_config(deps: Deps, _env: Env) -> StdResult<Config<Addr>> {
    CONFIG.load(deps.storage)
}

fn query_allocation(deps: Deps, _env: Env, account: String) -> StdResult<AllocationResponse> {
    let account_checked = deps.api.addr_validate(&account)?;

    Ok(AllocationResponse {
        params: PARAMS.load(deps.storage, &account_checked)?,
        status: STATUS.load(deps.storage, &account_checked)?,
    })
}

fn query_simulate_withdraw(
    deps: Deps,
    env: Env,
    account: String,
) -> StdResult<SimulateWithdrawResponse> {
    let account_checked = deps.api.addr_validate(&account)?;

    let config = CONFIG.load(deps.storage)?;
    let params = PARAMS.load(deps.storage, &account_checked)?;
    let mut status = STATUS.load(deps.storage, &account_checked)?;

    Ok(helpers::compute_withdraw_amounts(
        env.block.time.seconds(),
        &params,
        &mut status,
        config.default_unlock_schedule,
    ))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}

//----------------------------------------------------------------------------------------
// Helper Functions
//----------------------------------------------------------------------------------------

mod helpers {
    use cosmwasm_std::Uint128;

    use astroport_governance::astro_vesting::msg::SimulateWithdrawResponse;
    use astroport_governance::astro_vesting::{AllocationParams, AllocationStatus, Schedule};

    use std::cmp;

    pub fn compute_vested_or_unlocked_amount(
        timestamp: u64,
        amount: Uint128,
        schedule: &Schedule,
    ) -> Uint128 {
        // Before the end of cliff period, no token will be vested/unlocked
        if timestamp < schedule.start_time + schedule.cliff {
            Uint128::zero()
        // After the end of cliff, tokens vest/unlock linearly between start time and end time
        } else if timestamp < schedule.start_time + schedule.duration {
            amount.multiply_ratio(timestamp - schedule.start_time, schedule.duration)
        // After end time, all tokens are fully vested/unlocked
        } else {
            amount
        }
    }

    pub fn compute_withdraw_amounts(
        timestamp: u64,
        params: &AllocationParams,
        status: &mut AllocationStatus,
        default_unlock_schedule: Schedule,
    ) -> SimulateWithdrawResponse {
        let unlock_schedule = match &params.unlock_schedule {
            Some(schedule) => schedule,
            None => &default_unlock_schedule,
        };

        // "Free" amount is the smaller between vested amount and unlocked amount
        let astro_vested =
            compute_vested_or_unlocked_amount(timestamp, params.amount, &params.vest_schedule);
        let astro_unlocked =
            compute_vested_or_unlocked_amount(timestamp, params.amount, unlock_schedule);

        let astro_free = cmp::min(astro_vested, astro_unlocked);

        // Withdrawable amount is unlocked amount minus the amount already withdrawn
        let astro_withdrawn = status.astro_withdrawn + status.astro_withdrawn;
        let astro_withdrawable = astro_free - astro_withdrawn;

        status.astro_withdrawn += astro_withdrawable;

        SimulateWithdrawResponse {
            astro_to_withdraw: astro_withdrawable,
        }
    }
}
