use astroport::asset::addr_opt_validate;
use astroport::asset::validate_native_denom;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coins, ensure, BankMsg, DepsMut, Env, MessageInfo, Response, StdError, Uint128,
};
use cw2::set_contract_version;
use cw_utils::{may_pay, must_pay};

use astroport_governance::builder_unlock::{Config, CreateAllocationParams, Schedule};
use astroport_governance::builder_unlock::{ExecuteMsg, InstantiateMsg};

use crate::error::ContractError;
use crate::state::{Allocation, CONFIG, OWNERSHIP_PROPOSAL, PARAMS, STATE};

// Version and name used for contract migration.
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters in the `msg` variable.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    validate_native_denom(&msg.astro_denom)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            astro_denom: msg.astro_denom,
            max_allocations_amount: msg.max_allocations_amount,
        },
    )?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &Default::default(), env.block.time.seconds())?;

    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::CreateAllocations** Create allocations.
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
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::CreateAllocations { allocations } => {
            execute_create_allocations(deps, env, info, allocations)
        }
        ExecuteMsg::Withdraw {} => execute_withdraw(deps, env, info),
        ExecuteMsg::ProposeNewReceiver { new_receiver } => {
            execute_propose_new_receiver(deps, env, info, new_receiver)
        }
        ExecuteMsg::DropNewReceiver {} => execute_drop_new_receiver(deps, env, info),
        ExecuteMsg::ClaimReceiver { prev_receiver } => {
            execute_claim_receiver(deps, env, info, prev_receiver)
        }
        ExecuteMsg::IncreaseAllocation { receiver, amount } => {
            let config = CONFIG.load(deps.storage)?;
            ensure!(
                info.sender == config.owner,
                StdError::generic_err("Only the contract owner can increase allocations")
            );
            let deposit_amount = may_pay(&info, &config.astro_denom)?;

            execute_increase_allocation(deps, env, &config, receiver, amount, deposit_amount)
        }
        ExecuteMsg::DecreaseAllocation { receiver, amount } => {
            execute_decrease_allocation(deps, env, info, receiver, amount)
        }
        ExecuteMsg::TransferUnallocated { amount, recipient } => {
            execute_transfer_unallocated(deps, env, info, amount, recipient)
        }
        ExecuteMsg::ProposeNewOwner {
            new_owner,
            expires_in,
        } => {
            let config = CONFIG.load(deps.storage)?;
            propose_new_owner(
                deps,
                info,
                env,
                new_owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config = CONFIG.load(deps.storage)?;
            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.owner = new_owner;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
        ExecuteMsg::UpdateConfig {
            new_max_allocations_amount,
        } => update_config(deps, info, new_max_allocations_amount),
        ExecuteMsg::UpdateUnlockSchedules {
            new_unlock_schedules,
        } => update_unlock_schedules(deps, env, info, new_unlock_schedules),
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
pub fn execute_create_allocations(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    allocations: Vec<(String, CreateAllocationParams)>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    ensure!(
        info.sender == config.owner,
        StdError::generic_err("Only the contract owner can create allocations",)
    );

    let deposit_amount = must_pay(&info, &config.astro_denom)?;
    let expected_deposit: Uint128 = allocations.iter().map(|(_, params)| params.amount).sum();
    ensure!(
        deposit_amount == expected_deposit,
        ContractError::DepositAmountMismatch {
            expected: expected_deposit,
            got: deposit_amount,
        }
    );

    let mut state = STATE.load(deps.storage)?;

    state.total_astro_deposited += deposit_amount;
    state.remaining_astro_tokens += deposit_amount;

    ensure!(
        state.total_astro_deposited <= config.max_allocations_amount,
        ContractError::TotalAllocationExceedsAmount(config.max_allocations_amount)
    );

    let block_ts = env.block.time.seconds();

    for (user_unchecked, params) in allocations {
        let user = deps.api.addr_validate(&user_unchecked)?;
        let allocation = Allocation::new_allocation(deps.storage, block_ts, &user, params)?;
        allocation.save(deps.storage)?;
    }

    STATE.save(deps.storage, &state, block_ts)?;

    Ok(Response::default())
}

/// Allow allocation recipients to withdraw unlocked ASTRO.
pub fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let block_ts = env.block.time.seconds();
    let mut allocation = Allocation::must_load(deps.storage, block_ts, &info.sender)?;

    let astro_to_withdraw = allocation.withdraw_and_update()?;
    allocation.save(deps.storage)?;

    let mut state = STATE.load(deps.storage)?;
    state.remaining_astro_tokens -= astro_to_withdraw;

    STATE.save(deps.storage, &state, block_ts)?;

    let bank_msg = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: coins(
            astro_to_withdraw.u128(),
            CONFIG.load(deps.storage)?.astro_denom,
        ),
    };

    Ok(Response::new()
        .add_message(bank_msg)
        .add_attribute("astro_withdrawn", astro_to_withdraw))
}

/// Allows the current allocation receiver to propose a new receiver.
///
/// * **new_receiver** new proposed receiver for the allocation.
pub fn execute_propose_new_receiver(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_receiver: String,
) -> Result<Response, ContractError> {
    let mut allocation =
        Allocation::must_load(deps.storage, env.block.time.seconds(), &info.sender)?;
    let new_receiver = deps.api.addr_validate(&new_receiver)?;

    allocation.propose_new_receiver(deps.storage, &new_receiver)?;
    allocation.save(deps.storage)?;

    Ok(Response::new()
        .add_attribute("action", "ProposeNewReceiver")
        .add_attribute("proposed_receiver", new_receiver))
}

/// Drop the new proposed receiver for a specific allocation.
pub fn execute_drop_new_receiver(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut allocation =
        Allocation::must_load(deps.storage, env.block.time.seconds(), &info.sender)?;

    let proposed_receiver = allocation.drop_proposed_receiver()?;
    allocation.save(deps.storage)?;

    Ok(Response::new()
        .add_attribute("action", "DropNewReceiver")
        .add_attribute("dropped_proposed_receiver", proposed_receiver))
}

/// Allows a newly proposed allocation receiver to claim the ownership of that allocation.
///
/// * **prev_receiver** this is the previous receiver for the allocation.
pub fn execute_claim_receiver(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    prev_receiver: String,
) -> Result<Response, ContractError> {
    let prev_receiver_addr = deps.api.addr_validate(&prev_receiver)?;
    let allocation =
        Allocation::must_load(deps.storage, env.block.time.seconds(), &prev_receiver_addr)?;

    if allocation.params.proposed_receiver == Some(info.sender.clone()) {
        ensure!(
            !PARAMS.has(deps.storage, &info.sender),
            ContractError::ProposedReceiverAlreadyHasAllocation {}
        );

        let new_allocation = allocation.claim_allocation(deps.storage, &info.sender)?;
        new_allocation.save(deps.storage)?;
    } else {
        return Err(ContractError::ProposedReceiverMismatch {});
    }

    Ok(Response::new().add_attributes(vec![
        attr("action", "ClaimReceiver"),
        attr("prev_receiver", prev_receiver),
        attr("receiver", info.sender),
    ]))
}

/// Decrease an address' ASTRO allocation.
///
/// * **receiver** address that will have its allocation decreased.
///
/// * **amount** ASTRO amount to decrease the allocation by.
pub fn execute_decrease_allocation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    receiver: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    ensure!(
        info.sender == config.owner,
        ContractError::UnauthorizedDecreaseAllocation {}
    );

    let receiver = deps.api.addr_validate(&receiver)?;
    let block_ts = env.block.time.seconds();
    let mut allocation = Allocation::must_load(deps.storage, block_ts, &receiver)?;

    allocation.decrease_allocation(amount)?;
    allocation.save(deps.storage)?;

    let mut state = STATE.load(deps.storage)?;

    state.unallocated_astro_tokens = state.unallocated_astro_tokens.checked_add(amount)?;
    state.remaining_astro_tokens = state.remaining_astro_tokens.checked_sub(amount)?;

    STATE.save(deps.storage, &state, block_ts)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "execute_decrease_allocation"),
        attr("receiver", receiver),
        attr("amount", amount),
    ]))
}

/// Increase an address' ASTRO allocation.
///
/// * **receiver** address that will have its allocation increased.
///
/// * **amount** ASTRO amount to increase the allocation by.
///
/// * **deposit_amount** is amount of ASTRO to increase the allocation
pub fn execute_increase_allocation(
    deps: DepsMut,
    env: Env,
    config: &Config,
    receiver: String,
    amount: Uint128,
    deposit_amount: Uint128,
) -> Result<Response, ContractError> {
    let receiver = deps.api.addr_validate(&receiver)?;
    let block_ts = env.block.time.seconds();
    let mut allocation = Allocation::must_load(deps.storage, block_ts, &receiver)?;

    allocation.increase_allocation(amount)?;
    allocation.save(deps.storage)?;

    let mut state = STATE.load(deps.storage)?;

    state.total_astro_deposited += deposit_amount;
    state.unallocated_astro_tokens += deposit_amount;

    ensure!(
        state.total_astro_deposited <= config.max_allocations_amount,
        ContractError::TotalAllocationExceedsAmount(config.max_allocations_amount)
    );

    ensure!(
        state.unallocated_astro_tokens >= amount,
        ContractError::UnallocatedTokensExceedsTotalDeposited(state.unallocated_astro_tokens)
    );

    state.unallocated_astro_tokens = state.unallocated_astro_tokens.checked_sub(amount)?;
    state.remaining_astro_tokens += amount;

    STATE.save(deps.storage, &state, block_ts)?;

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
pub fn execute_transfer_unallocated(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    recipient: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    ensure!(
        config.owner == info.sender,
        ContractError::UnallocatedTransferUnauthorized {}
    );

    let mut state = STATE.load(deps.storage)?;

    ensure!(
        state.unallocated_astro_tokens >= amount,
        ContractError::InsufficientUnallocatedTokens(state.unallocated_astro_tokens)
    );

    state.unallocated_astro_tokens = state.unallocated_astro_tokens.checked_sub(amount)?;
    state.total_astro_deposited = state.total_astro_deposited.checked_sub(amount)?;

    let recipient = addr_opt_validate(deps.api, &recipient)?.unwrap_or_else(|| info.sender.clone());
    let bank_msg = BankMsg::Send {
        to_address: recipient.to_string(),
        amount: coins(amount.u128(), config.astro_denom),
    };

    STATE.save(deps.storage, &state, env.block.time.seconds())?;

    Ok(Response::new()
        .add_attribute("action", "execute_transfer_unallocated")
        .add_attribute("amount", amount)
        .add_message(bank_msg))
}

/// Updates builder unlock contract parameters.
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_max_allocations_amount: Uint128,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    let state = STATE.load(deps.storage)?;

    if new_max_allocations_amount < state.total_astro_deposited {
        return Err(StdError::generic_err(format!(
            "The new max allocations amount {new_max_allocations_amount} can not be less than currently deposited {}",
            state.total_astro_deposited,
        )).into());
    }

    config.max_allocations_amount = new_max_allocations_amount;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute("new_max_allocations_amount", new_max_allocations_amount))
}

/// Updates builder unlock schedules for specified accounts.
pub fn update_unlock_schedules(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_unlock_schedules: Vec<(String, Schedule)>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    let block_ts = env.block.time.seconds();

    for (account, new_schedule) in new_unlock_schedules {
        let account_addr = deps.api.addr_validate(&account)?;
        let mut allocation = Allocation::must_load(deps.storage, block_ts, &account_addr)?;
        allocation.update_unlock_schedule(&new_schedule)?;
        allocation.save(deps.storage)?;
    }

    Ok(Response::new().add_attribute("action", "update_unlock_schedules"))
}
