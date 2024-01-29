#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coins, to_json_binary, Addr, BankMsg, CosmosMsg, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20_base::contract::{execute_update_marketing, execute_upload_logo};
use cw20_base::state::MARKETING_INFO;
use cw_utils::must_pay;

use astroport_governance::voting_escrow_lite::{Config, ExecuteMsg};
use astroport_governance::{generator_controller_lite, outpost};

use crate::astroport::common::{
    claim_ownership, drop_ownership_proposal, propose_new_owner, validate_addresses,
};
use crate::error::ContractError;
use crate::marketing_validation::{validate_marketing_info, validate_whitelist_links};
use crate::state::{Lock, BLACKLIST, CONFIG, LOCKED, OWNERSHIP_PROPOSAL, VOTING_POWER_HISTORY};
use crate::utils::{blacklist_check, fetch_last_checkpoint};

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::Unlock {}** Unlock all xASTRO from a lock position, subject to a waiting period until withdrawal is possible.
///
/// * **ExecuteMsg::Relock {}** Relock all xASTRO from an unlocking position if the Hub could not be notified
///
/// * **ExecuteMsg::Withdraw {}** Withdraw all xASTRO from an lock position if the unlock time has expired.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
///
/// * **ExecuteMsg::UpdateBlacklist { append_addrs, remove_addrs }** Updates the contract's blacklist.
///
/// * **ExecuteMsg::UpdateMarketing { project, description, marketing }** Updates the contract's marketing information.
///
/// * **ExecuteMsg::UploadLogo { logo }** Uploads a new logo to the contract.
///
/// * **ExecuteMsg::SetLogoUrlsWhitelist { whitelist }** Sets the contract's logo whitelist.
///
/// * **ExecuteMsg::UpdateConfig { new_guardian }** Updates the contract's guardian.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::CreateLock {} => {
            blacklist_check(deps.storage, &info.sender)?;

            let config = CONFIG.load(deps.storage)?;
            let amount = must_pay(&info, &config.deposit_denom)?;

            create_lock(deps, env, info.sender, amount)
        }
        ExecuteMsg::DepositFor { user } => {
            blacklist_check(deps.storage, &info.sender)?;

            let addr = deps.api.addr_validate(&user)?;
            blacklist_check(deps.storage, &addr)?;

            let config = CONFIG.load(deps.storage)?;
            let amount = must_pay(&info, &config.deposit_denom)?;

            deposit_for(deps, env, amount, addr)
        }
        ExecuteMsg::ExtendLockAmount {} => {
            blacklist_check(deps.storage, &info.sender)?;

            let config = CONFIG.load(deps.storage)?;
            let amount = must_pay(&info, &config.deposit_denom)?;

            deposit_for(deps, env, amount, info.sender)
        }
        ExecuteMsg::Unlock {} => unlock(deps, env, info),
        ExecuteMsg::Relock { user } => relock(deps, env, info, user),
        ExecuteMsg::Withdraw {} => withdraw(deps, env, info),
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
            let config: Config = CONFIG.load(deps.storage)?;

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
        ExecuteMsg::UpdateBlacklist {
            append_addrs,
            remove_addrs,
        } => update_blacklist(deps, env, info, append_addrs, remove_addrs),
        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => {
            validate_marketing_info(project.as_ref(), description.as_ref(), None, &[])?;
            execute_update_marketing(deps, env, info, project, description, marketing)
                .map_err(Into::into)
        }
        ExecuteMsg::UploadLogo(logo) => {
            let config = CONFIG.load(deps.storage)?;
            validate_marketing_info(None, None, Some(&logo), &config.logo_urls_whitelist)?;
            execute_upload_logo(deps, env, info, logo).map_err(Into::into)
        }
        ExecuteMsg::SetLogoUrlsWhitelist { whitelist } => {
            let mut config = CONFIG.load(deps.storage)?;
            let marketing_info = MARKETING_INFO.load(deps.storage)?;
            if info.sender != config.owner && Some(info.sender) != marketing_info.marketing {
                Err(ContractError::Unauthorized {})
            } else {
                validate_whitelist_links(&whitelist)?;
                config.logo_urls_whitelist = whitelist;
                CONFIG.save(deps.storage, &config)?;
                Ok(Response::default().add_attribute("action", "set_logo_urls_whitelist"))
            }
        }
        ExecuteMsg::UpdateConfig {
            new_guardian,
            generator_controller,
            outpost,
        } => execute_update_config(deps, info, new_guardian, generator_controller, outpost),
    }
}

/// Creates a lock for the user that lasts until Unlock is called
/// Creates a lock if it doesn't exist and triggers a [`checkpoint`] for the staker.
/// If a lock already exists, then a [`ContractError`] is returned.
///
/// * **user** staker for which we create a lock position.
///
/// * **amount** amount of xASTRO deposited in the lock position.
fn create_lock(
    deps: DepsMut,
    env: Env,
    user: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    LOCKED.update(
        deps.storage,
        user.clone(),
        env.block.time.seconds(),
        |lock_opt| {
            if lock_opt.is_some() && !lock_opt.unwrap().amount.is_zero() {
                return Err(ContractError::LockAlreadyExists {});
            }
            Ok(Lock { amount, end: None })
        },
    )?;
    checkpoint(deps, env, user, Some(amount))?;

    Ok(Response::default().add_attribute("action", "create_lock"))
}

/// Deposits an 'amount' of xASTRO tokens into 'user''s lock.
/// Triggers a [`checkpoint`] for the user.
/// If the user does not have a lock, then a lock is created.
///
/// * **amount** amount of xASTRO to deposit.
///
/// * **user** user who's lock amount will increase.
fn deposit_for(
    deps: DepsMut,
    env: Env,
    amount: Uint128,
    user: Addr,
) -> Result<Response, ContractError> {
    LOCKED.update(
        deps.storage,
        user.clone(),
        env.block.time.seconds(),
        |lock_opt| {
            match lock_opt {
                Some(mut lock) if !lock.amount.is_zero() => match lock.end {
                    // This lock is still locked
                    None => {
                        lock.amount += amount;
                        Ok(lock)
                    }
                    // This lock is expired or being unlocked, thus reject the deposit
                    Some(end) => {
                        if end <= env.block.time.seconds() {
                            return Err(ContractError::LockExpired {});
                        }
                        Err(ContractError::Unlocking {})
                    }
                },
                // If no lock exists, create a new one
                _ => Ok(Lock { amount, end: None }),
            }
        },
    )?;
    checkpoint(deps, env, user, Some(amount))?;

    Ok(Response::default().add_attribute("action", "deposit_for"))
}

/// Starts the unlock of the whole amount of locked xASTRO from a specific user lock.
/// If the user lock doesn't exist or if it has been unlocked, then a [`ContractError`] is returned.
///
/// Note: When a user unlocks, they lose their emission voting power immediately
fn unlock(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let sender = info.sender;

    // 'LockDoesNotExist' is thrown either when a lock does not exist in LOCKED or when a lock exists but lock.amount == 0
    let lock = LOCKED
        .may_load(deps.storage, sender.clone())?
        .filter(|lock| !lock.amount.is_zero())
        .ok_or(ContractError::LockDoesNotExist {})?;

    match lock.end {
        // This lock is still locked, we can unlock
        None => {
            let config = CONFIG.load(deps.storage)?;
            let response = Response::default().add_attribute("action", "unlock_initiated");

            // Start the unlock for this address
            start_unlock(lock, deps, env, sender.clone())?;

            // We only allow either the generator controller _or_ the Outpost to be set at any time
            let kick_msg = match (&config.generator_controller_addr, &config.outpost_addr) {
                (Some(generator_controller), None) => {
                    // On the Hub we kick the user from the Generator Controller directly
                    // Voting power is removed immediately after a user unlocks
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: generator_controller.to_string(),
                        msg: to_json_binary(
                            &generator_controller_lite::ExecuteMsg::KickUnlockedVoters {
                                unlocked_voters: vec![sender.to_string()],
                            },
                        )?,
                        funds: vec![],
                    })
                }
                (None, Some(outpost)) => {
                    // If this vxASTRO contract is deployed on an Outpost we need to
                    // forward the unlock to the Hub, if the notification fails
                    // the funds will be locked again
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: outpost.to_string(),
                        msg: to_json_binary(&outpost::ExecuteMsg::KickUnlocked { user: sender })?,
                        funds: vec![],
                    })
                }
                _ => {
                    return Err(StdError::generic_err(
                        "Either Generator Controller or Outpost must be set",
                    )
                    .into());
                }
            };

            Ok(response.add_message(kick_msg))
        }
        // This lock is expired or being unlocked, can't unlock again
        Some(end) => {
            if end <= env.block.time.seconds() {
                return Err(ContractError::LockExpired {});
            }
            Err(ContractError::Unlocking {})
        }
    }
}

/// Locks the given user's xASTRO lock again if the Hub could not be notified
///
/// When a user unlocks, the Hub needs to be notified so that the user's votes
/// can be kicked from the Generator Controller. If the notification to the Hub
/// fails, then the position must be locked again
/// If the user lock doesn't exist or if it has been completely unlocked,
/// then a [`ContractError`] is returned.
fn relock(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    user: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Check that the caller is the Outpost contract
    if Some(info.sender) != config.outpost_addr {
        return Err(ContractError::Unauthorized {});
    }

    let sender = Addr::unchecked(user);
    // 'LockDoesNotExist' is thrown either when a lock does not exist in LOCKED or when a lock exists but lock.amount == 0
    let mut lock = LOCKED
        .may_load(deps.storage, sender.clone())?
        .filter(|lock| !lock.amount.is_zero())
        .ok_or(ContractError::LockDoesNotExist {})?;

    // If the lock has been unlocked
    if lock.end.is_some() {
        lock.end = None;
        LOCKED.save(
            deps.storage,
            sender.clone(),
            &lock,
            env.block.time.seconds(),
        )?;
        // Relock needs to add back the user's voting power
        VOTING_POWER_HISTORY.save(
            deps.storage,
            (sender.clone(), env.block.time.seconds()),
            &lock.amount,
        )?;
        checkpoint_total(deps.storage, env, Some(lock.amount), None)?;
    }

    Ok(Response::new()
        .add_attribute("action", "relock")
        .add_attribute("user", sender))
}

/// Withdraws the whole amount of locked xASTRO from a specific user lock.
/// If the user lock doesn't exist or if it has not yet expired, then a [`ContractError`] is returned.
fn withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let sender = info.sender;
    // 'LockDoesNotExist' is thrown either when a lock does not exist in LOCKED or when a lock exists but lock.amount == 0
    let mut lock = LOCKED
        .may_load(deps.storage, sender.clone())?
        .filter(|lock| !lock.amount.is_zero())
        .ok_or(ContractError::LockDoesNotExist {})?;

    match lock.end {
        // This lock is still locked, withdrawal not possible
        None => Err(ContractError::NotUnlocked {}),
        // This lock is expired or being unlocked
        Some(end) => {
            // Still unlocking, can't withdraw
            if end > env.block.time.seconds() {
                return Err(ContractError::LockHasNotExpired {});
            }
            // Unlocked, withdrawal is now allowed
            let config = CONFIG.load(deps.storage)?;

            let transfer_msg = BankMsg::Send {
                to_address: sender.to_string(),
                amount: coins(lock.amount.u128(), &config.deposit_denom),
            };
            lock.amount = Uint128::zero();
            LOCKED.save(deps.storage, sender, &lock, env.block.time.seconds())?;

            Ok(Response::default()
                .add_message(transfer_msg)
                .add_attribute("action", "withdraw"))
        }
    }
}

/// Update the staker blacklist. Whitelists addresses specified in 'remove_addrs'
/// and blacklists new addresses specified in 'append_addrs'. Nullifies staker voting power and
/// cancels their contribution in the total voting power (total vxASTRO supply).
///
/// * **append_addrs** array of addresses to blacklist.
///
/// * **remove_addrs** array of addresses to whitelist.
fn update_blacklist(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    append_addrs: Vec<String>,
    remove_addrs: Vec<String>,
) -> Result<Response, ContractError> {
    if append_addrs.is_empty() && remove_addrs.is_empty() {
        return Err(StdError::generic_err("Append and remove arrays are empty").into());
    }

    let config = CONFIG.load(deps.storage)?;
    // Permission check
    if info.sender != config.owner && Some(info.sender) != config.guardian_addr {
        return Err(ContractError::Unauthorized {});
    }
    let blacklist = BLACKLIST.load(deps.storage)?;
    let append: Vec<_> = validate_addresses(deps.api, &append_addrs)?
        .into_iter()
        .filter(|addr| !blacklist.contains(addr))
        .collect();
    let remove: Vec<_> = validate_addresses(deps.api, &remove_addrs)?
        .into_iter()
        .filter(|addr| blacklist.contains(addr))
        .collect();

    let timestamp = env.block.time.seconds();
    let mut reduce_total_vp = Uint128::zero(); // accumulator for decreasing total voting power

    for addr in append.iter() {
        let last_checkpoint = fetch_last_checkpoint(deps.storage, addr, timestamp)?;
        if let Some((_, emissions_power)) = last_checkpoint {
            // We need to checkpoint with zero power and zero slope
            VOTING_POWER_HISTORY.save(deps.storage, (addr.clone(), timestamp), &Uint128::zero())?;

            let cur_power = emissions_power;
            // User's contribution is already zero. Skipping them
            if cur_power.is_zero() {
                continue;
            }

            // User's contribution in the total voting power calculation
            reduce_total_vp += cur_power;
        }
    }

    if !reduce_total_vp.is_zero() {
        // Trigger a total voting power recalculation
        checkpoint_total(deps.storage, env.clone(), None, Some(reduce_total_vp))?;
    }

    for addr in remove.iter() {
        let lock_opt = LOCKED.may_load(deps.storage, addr.clone())?;
        if let Some(Lock { amount, end, .. }) = lock_opt {
            match end {
                // Only checkpoint the amount if the lock if still active
                None => checkpoint(deps.branch(), env.clone(), addr.clone(), Some(amount))?,
                // This lock is expired or being unlocked and has already been set to zero
                Some(_) => checkpoint(deps.branch(), env.clone(), addr.clone(), None)?,
            }
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

    let mut attrs = vec![attr("action", "update_blacklist")];
    if !append_addrs.is_empty() {
        attrs.push(attr("added_addresses", append_addrs.join(",")))
    }
    if !remove_addrs.is_empty() {
        attrs.push(attr("removed_addresses", remove_addrs.join(",")))
    }

    // TODO: Submit update blacklist immediately

    Ok(Response::default().add_attributes(attrs))
}

/// Updates contracts' guardian address.
fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_guardian: Option<String>,
    generator_controller: Option<String>,
    outpost: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(new_guardian) = new_guardian {
        config.guardian_addr = Some(deps.api.addr_validate(&new_guardian)?);
    }

    if let Some(generator_controller) = generator_controller {
        if config.outpost_addr.is_some() {
            return Err(StdError::generic_err(
                "Only one of Generator Controller or Outpost can be set",
            )
            .into());
        }
        config.generator_controller_addr = Some(deps.api.addr_validate(&generator_controller)?);
    }

    if let Some(outpost) = outpost {
        if config.generator_controller_addr.is_some() {
            return Err(StdError::generic_err(
                "Only one of Generator Controller or Outpost can be set",
            )
            .into());
        }
        config.outpost_addr = Some(deps.api.addr_validate(&outpost)?);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default().add_attribute("action", "execute_update_config"))
}

/// Start the unlock of a user's Lock
///
/// The unlocking time is based on the current block time + configured unlock period
fn start_unlock(mut lock: Lock, deps: DepsMut, env: Env, sender: Addr) -> StdResult<()> {
    let config = CONFIG.load(deps.storage)?;
    let unlock_time = env.block.time.seconds() + config.unlock_period;
    lock.end = Some(unlock_time);
    LOCKED.save(
        deps.storage,
        sender.clone(),
        &lock,
        env.block.time.seconds(),
    )?;
    // Update user's voting power
    VOTING_POWER_HISTORY.save(
        deps.storage,
        (sender, env.block.time.seconds()),
        &Uint128::zero(),
    )?;
    // Update total voting power
    checkpoint_total(deps.storage, env, None, Some(lock.amount))
}

/// Checkpoint a user's voting power (vxASTRO balance).
/// This function fetches the user's last available checkpoint, calculates the user's current voting power
/// and saves the new checkpoint for the current period in [`HISTORY`] (using the user's address).
/// If a user already checkpointed themselves for the current period, then this function uses the current checkpoint as the latest
/// available one.
///
/// * **addr** staker for which we checkpoint the voting power.
///
/// * **add_amount** amount of vxASTRO to add to the staker's balance.
fn checkpoint(deps: DepsMut, env: Env, addr: Addr, add_amount: Option<Uint128>) -> StdResult<()> {
    let timestamp = env.block.time.seconds();
    let add_amount = add_amount.unwrap_or_default();

    // Get the last user checkpoint
    let last_checkpoint = fetch_last_checkpoint(deps.storage, &addr, timestamp)?;
    let new_power = if let Some((_, emissions_power)) = last_checkpoint {
        emissions_power.checked_add(add_amount)?
    } else {
        add_amount
    };

    VOTING_POWER_HISTORY.save(deps.storage, (addr, timestamp), &new_power)?;
    checkpoint_total(deps.storage, env, Some(add_amount), None)
}

/// Checkpoint the total voting power (total supply of vxASTRO).
/// This function fetches the last available vxASTRO checkpoint
/// saves all recalculated periods in [`HISTORY`].
///
/// * **add_voting_power** amount of vxASTRO to add to the total.
///
/// * **reduce_power** amount of vxASTRO to subtract from the total.
fn checkpoint_total(
    storage: &mut dyn Storage,
    env: Env,
    add_voting_power: Option<Uint128>,
    reduce_power: Option<Uint128>,
) -> StdResult<()> {
    let timestamp = env.block.time.seconds();
    let contract_addr = env.contract.address;
    let add_voting_power = add_voting_power.unwrap_or_default();

    // Get last checkpoint
    let last_checkpoint = fetch_last_checkpoint(storage, &contract_addr, timestamp)?;
    let new_point = if let Some((_, emissions_power)) = last_checkpoint {
        let mut new_power = emissions_power.saturating_add(add_voting_power);
        new_power = new_power.saturating_sub(reduce_power.unwrap_or_default());
        new_power
    } else {
        add_voting_power
    };
    VOTING_POWER_HISTORY.save(storage, (contract_addr, timestamp), &new_point)
}
