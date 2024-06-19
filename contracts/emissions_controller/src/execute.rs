use std::collections::{HashMap, HashSet};

use astroport::asset::{determine_asset_info, validate_native_denom};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::incentives;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, ensure, to_json_binary, wasm_execute, BankMsg, Coin, CosmosMsg, Decimal, DepsMut, Env,
    Fraction, IbcMsg, IbcTimeout, MessageInfo, Order, Response, StdError, StdResult, Storage,
    Uint128,
};
use cw_utils::{must_pay, nonpayable};
use itertools::Itertools;
use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::query::NeutronQuery;

use astroport_governance::emissions_controller::consts::{
    EPOCH_LENGTH, IBC_TIMEOUT, MAX_POOLS_TO_VOTE, VOTE_COOLDOWN,
};
use astroport_governance::emissions_controller::hub::{
    AstroPoolConfig, HubMsg, OutpostInfo, OutpostParams, OutpostStatus, TuneInfo, UserInfo,
    VotedPoolInfo,
};
use astroport_governance::emissions_controller::msg::{ExecuteMsg, VxAstroIbcMsg};
use astroport_governance::emissions_controller::utils::{check_lp_token, get_voting_power};
use astroport_governance::utils::check_contract_supports_channel;
use astroport_governance::{assembly, voting_escrow};

use crate::error::ContractError;
use crate::state::{
    CONFIG, OUTPOSTS, OWNERSHIP_PROPOSAL, POOLS_WHITELIST, TUNE_INFO, USER_INFO, VOTED_POOLS,
};
use crate::utils::{
    build_emission_ibc_msg, determine_outpost_prefix, get_epoch_start, get_outpost_prefix,
    min_ntrn_ibc_fee, raw_emissions_to_schedules, simulate_tune, validate_outpost_prefix,
    TuneResult,
};

/// Exposes all the execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg<HubMsg>,
) -> Result<Response<NeutronMsg>, ContractError> {
    match msg {
        ExecuteMsg::Vote { votes } => {
            nonpayable(&info)?;
            let votes_map: HashMap<_, _> = votes.iter().cloned().collect();
            ensure!(
                votes.len() == votes_map.len(),
                ContractError::DuplicatedVotes {}
            );
            ensure!(
                votes_map.len() <= MAX_POOLS_TO_VOTE,
                ContractError::ExceededMaxPoolsToVote {}
            );
            let deps = deps.into_empty();
            let config = CONFIG.load(deps.storage)?;
            let voting_power = get_voting_power(deps.querier, &config.vxastro, &info.sender, None)?;
            ensure!(!voting_power.is_zero(), ContractError::ZeroVotingPower {});

            handle_vote(deps, env, info.sender.as_str(), voting_power, votes_map)
        }
        ExecuteMsg::UpdateUserVotes { user, is_unlock } => {
            let config = CONFIG.load(deps.storage)?;
            ensure!(
                info.sender == config.vxastro,
                ContractError::Unauthorized {}
            );
            let voter = deps.api.addr_validate(&user)?;
            let deps = deps.into_empty();

            let voting_power = get_voting_power(deps.querier, &config.vxastro, &voter, None)?;
            handle_update_user(deps.storage, env, voter.as_str(), voting_power).and_then(
                |response| {
                    if is_unlock {
                        let confirm_unlock_msg = wasm_execute(
                            config.vxastro,
                            &voting_escrow::ExecuteMsg::ConfirmUnlock {
                                user: voter.to_string(),
                            },
                            vec![],
                        )?;
                        Ok(response.add_message(confirm_unlock_msg))
                    } else {
                        Ok(response)
                    }
                },
            )
        }
        ExecuteMsg::RefreshUserVotes {} => {
            nonpayable(&info)?;
            let config = CONFIG.load(deps.storage)?;
            let deps = deps.into_empty();

            let voting_power = get_voting_power(deps.querier, &config.vxastro, &info.sender, None)?;

            ensure!(!voting_power.is_zero(), ContractError::ZeroVotingPower {});
            handle_update_user(deps.storage, env, info.sender.as_str(), voting_power)
        }
        ExecuteMsg::ProposeNewOwner {
            new_owner,
            expires_in,
        } => {
            nonpayable(&info)?;
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
            nonpayable(&info)?;
            let config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            nonpayable(&info)?;
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
        ExecuteMsg::Custom(hub_msg) => match hub_msg {
            HubMsg::WhitelistPool { lp_token: pool } => whitelist_pool(deps, env, info, pool),
            HubMsg::UpdateOutpost {
                prefix,
                astro_denom,
                outpost_params,
                astro_pool_config,
            } => update_outpost(
                deps,
                env,
                info,
                prefix,
                astro_denom,
                outpost_params,
                astro_pool_config,
            ),
            HubMsg::RemoveOutpost { prefix } => remove_outpost(deps, env, info, prefix),
            HubMsg::TunePools {} => tune_pools(deps, env),
            HubMsg::RetryFailedOutposts {} => retry_failed_outposts(deps, info, env),
            HubMsg::UpdateConfig {
                pools_per_outpost,
                whitelisting_fee,
                fee_receiver,
                emissions_multiple,
                max_astro,
            } => update_config(
                deps,
                info,
                pools_per_outpost,
                whitelisting_fee,
                fee_receiver,
                emissions_multiple,
                max_astro,
            ),
            HubMsg::RegisterProposal { proposal_id } => register_proposal(deps, env, proposal_id),
        },
    }
}

/// Permissionless endpoint to whitelist a pool.
/// Requires a fee to be paid.
/// This endpoint is meant to be executed by users from the Hub or from other outposts via IBC hooks.
pub fn whitelist_pool(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    pool: String,
) -> Result<Response<NeutronMsg>, ContractError> {
    let deps = deps.into_empty();
    let config = CONFIG.load(deps.storage)?;
    let amount = must_pay(&info, &config.whitelisting_fee.denom)?;
    ensure!(
        amount == config.whitelisting_fee.amount,
        ContractError::IncorrectWhitelistFee(config.whitelisting_fee)
    );

    // Perform basic LP token validation. Ensure the outpost exists.
    let outposts = OUTPOSTS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<HashMap<_, _>>>()?;
    if let Some(prefix) = get_outpost_prefix(&pool, &outposts) {
        if outposts.get(&prefix).unwrap().params.is_none() {
            // Validate LP token on the Hub
            determine_asset_info(&pool, deps.api)
                .and_then(|maybe_lp| check_lp_token(deps.querier, &config.factory, &maybe_lp))?
        }
    } else {
        return Err(ContractError::NoOutpostForPool(pool));
    }

    // Astro pools receive flat emissions hence we don't allow people to vote for them
    ensure!(
        outposts.values().all(|outpost_info| {
            outpost_info
                .astro_pool_config
                .as_ref()
                .map(|conf| conf.astro_pool != pool)
                .unwrap_or(true)
        }),
        ContractError::IsAstroPool {}
    );

    POOLS_WHITELIST.update(deps.storage, |v| {
        let mut pools: HashSet<_> = v.into_iter().collect();
        if !pools.insert(pool.clone()) {
            return Err(ContractError::PoolAlreadyWhitelisted(pool.clone()));
        };
        Ok(pools.into_iter().collect())
    })?;

    // Starting the voting process from scratch for this pool
    VOTED_POOLS.save(
        deps.storage,
        &pool,
        &VotedPoolInfo {
            init_ts: env.block.time.seconds(),
            voting_power: Uint128::zero(),
        },
        env.block.time.seconds(),
    )?;

    let send_fee_msg = BankMsg::Send {
        to_address: config.fee_receiver.to_string(),
        amount: info.funds,
    };

    Ok(Response::default()
        .add_message(send_fee_msg)
        .add_attributes([attr("action", "whitelist_pool"), attr("pool", &pool)]))
}

/// Permissioned endpoint to add or update outpost.
/// Performs several simple checks to cut off possible human errors.
pub fn update_outpost(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    prefix: String,
    astro_denom: String,
    outpost_params: Option<OutpostParams>,
    astro_pool_config: Option<AstroPoolConfig>,
) -> Result<Response<NeutronMsg>, ContractError> {
    nonpayable(&info)?;
    let deps = deps.into_empty();
    let config = CONFIG.load(deps.storage)?;

    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    validate_native_denom(&astro_denom)?;
    if let Some(conf) = &astro_pool_config {
        validate_outpost_prefix(&conf.astro_pool, &prefix)?;
        ensure!(
            !conf.constant_emissions.is_zero(),
            ContractError::ZeroAstroEmissions {}
        )
    }

    if let Some(params) = &outpost_params {
        validate_outpost_prefix(&params.emissions_controller, &prefix)?;
        ensure!(
            astro_denom.starts_with("ibc/") && astro_denom.len() == 68,
            ContractError::InvalidOutpostAstroDenom {}
        );
        check_contract_supports_channel(
            deps.as_ref().into_empty().querier,
            &env.contract.address,
            &params.voting_channel,
        )?;
        ensure!(
            params.ics20_channel.starts_with("channel-"),
            ContractError::InvalidOutpostIcs20Channel {}
        );
    } else {
        if let Some(conf) = &astro_pool_config {
            let maybe_lp_token = determine_asset_info(&conf.astro_pool, deps.api)?;
            check_lp_token(deps.querier, &config.factory, &maybe_lp_token)?;
        }
        ensure!(
            astro_denom == config.astro_denom,
            ContractError::InvalidHubAstroDenom(config.astro_denom)
        );
    }

    OUTPOSTS.save(
        deps.storage,
        &prefix,
        &OutpostInfo {
            params: outpost_params,
            astro_denom,
            astro_pool_config,
        },
    )?;

    Ok(Response::default().add_attributes([("action", "update_outpost"), ("prefix", &prefix)]))
}

/// Removes outpost from the contract as well as all whitelisted
/// and being voted pools related to this outpost.
pub fn remove_outpost(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    prefix: String,
) -> Result<Response<NeutronMsg>, ContractError> {
    nonpayable(&info)?;
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    // Remove all votable pools related to this outpost
    let voted_pools = VOTED_POOLS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?;
    let prefix_some = Some(prefix.clone());
    voted_pools
        .iter()
        .filter(|pool| determine_outpost_prefix(pool) == prefix_some)
        .try_for_each(|pool| VOTED_POOLS.remove(deps.storage, pool, env.block.time.seconds()))?;

    // And clear whitelist
    POOLS_WHITELIST.update::<_, StdError>(deps.storage, |mut whitelist| {
        whitelist.retain(|pool| determine_outpost_prefix(pool) != prefix_some);
        Ok(whitelist)
    })?;

    OUTPOSTS.remove(deps.storage, &prefix);

    Ok(Response::default().add_attributes([("action", "remove_outpost"), ("prefix", &prefix)]))
}

/// This permissionless endpoint retries failed emission IBC messages.
pub fn retry_failed_outposts(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    env: Env,
) -> Result<Response<NeutronMsg>, ContractError> {
    nonpayable(&info)?;
    let mut tune_info = TUNE_INFO.load(deps.storage)?;
    let outposts = OUTPOSTS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<HashMap<_, _>>>()?;

    let mut attrs = vec![attr("action", "retry_failed_outposts")];
    let ibc_fee = min_ntrn_ibc_fee(deps.as_ref())?;
    let config = CONFIG.load(deps.storage)?;

    let retry_msgs = tune_info
        .outpost_emissions_statuses
        .iter_mut()
        .filter_map(|(outpost, status)| {
            let outpost_info = outposts.get(outpost)?;
            outpost_info.params.as_ref().and_then(|params| {
                if *status == OutpostStatus::Failed {
                    let raw_schedules = tune_info.pools_grouped.get(outpost)?;
                    let (schedules, astro_funds) = raw_emissions_to_schedules(
                        &env,
                        raw_schedules,
                        &outpost_info.astro_denom,
                        &config.astro_denom,
                    );
                    // Ignoring this outpost if it failed to serialize IbcHook msg for some reason
                    let msg =
                        build_emission_ibc_msg(&env, params, &ibc_fee, astro_funds, &schedules)
                            .ok()?;

                    *status = OutpostStatus::InProgress;
                    attrs.push(attr("outpost", outpost));

                    Some(msg)
                } else {
                    None
                }
            })
        })
        .collect_vec();

    ensure!(
        !retry_msgs.is_empty(),
        ContractError::NoFailedOutpostsToRetry {}
    );

    TUNE_INFO.save(deps.storage, &tune_info, env.block.time.seconds())?;

    Ok(Response::new()
        .add_messages(retry_msgs)
        .add_attributes(attrs))
}

/// The function checks that:
/// * user didn't vote for the last 10 days,
/// * sum of all percentage values <= 1.
/// User can direct his voting power partially.
///
/// The function cancels changes applied by previous votes and applies new votes for the next epoch.
/// New vote parameters are saved in [`USER_INFO`].
///
/// * **voter** is a voter address.
/// * **voting_power** is a user's voting power reported from the outpost.
/// * **votes** is a map LP token -> percentage of user's voting power to direct to this pool.
pub fn handle_vote(
    deps: DepsMut,
    env: Env,
    voter: &str,
    voting_power: Uint128,
    votes: HashMap<String, Decimal>,
) -> Result<Response<NeutronMsg>, ContractError> {
    let user_info = USER_INFO.may_load(deps.storage, voter)?.unwrap_or_default();
    let block_ts = env.block.time.seconds();
    // Is the user eligible to vote again?
    ensure!(
        user_info.vote_ts + VOTE_COOLDOWN <= block_ts,
        ContractError::VoteCooldown(user_info.vote_ts + VOTE_COOLDOWN)
    );

    let mut total_weight = Decimal::zero();
    let whitelist: HashSet<_> = POOLS_WHITELIST.load(deps.storage)?.into_iter().collect();
    for (pool, weight) in &votes {
        ensure!(
            whitelist.contains(pool),
            ContractError::PoolIsNotWhitelisted(pool.clone())
        );

        total_weight += weight;

        ensure!(
            total_weight <= Decimal::one(),
            ContractError::InvalidTotalWeight {}
        );
    }

    // Cancel previous user votes. Filter non-whitelisted pools.
    let cache = user_info
        .votes
        .into_iter()
        .filter(|(pool, _)| whitelist.contains(pool))
        .map(|(pool, weight)| {
            let pool_info = VOTED_POOLS.load(deps.storage, &pool)?;
            // Subtract old vote from pool voting power if pool wasn't reset to 0
            let pool_dedicated_vp = if pool_info.init_ts <= user_info.vote_ts {
                user_info
                    .voting_power
                    .multiply_ratio(weight.numerator(), weight.denominator())
            } else {
                Uint128::zero()
            };
            Ok((pool, pool_info.with_sub_vp(pool_dedicated_vp)))
        })
        .collect::<StdResult<HashMap<_, _>>>()?;

    // Apply new votes with fresh user voting power.
    votes
        .iter()
        .try_for_each(|(pool, weight)| -> StdResult<()> {
            let pool_dedicated_vp =
                voting_power.multiply_ratio(weight.numerator(), weight.denominator());

            let pool_info = if let Some(pool_info) = cache.get(pool).cloned() {
                pool_info
            } else {
                VOTED_POOLS.load(deps.storage, pool)?
            };

            VOTED_POOLS.save(
                deps.storage,
                pool,
                &pool_info.with_add_vp(pool_dedicated_vp),
                block_ts,
            )
        })?;

    USER_INFO.save(
        deps.storage,
        voter,
        &UserInfo {
            vote_ts: block_ts,
            voting_power,
            votes,
        },
        block_ts,
    )?;

    Ok(Response::default()
        .add_attributes([attr("action", "vote"), attr("voting_power", voting_power)]))
}

/// This function updates existing user's voting power contribution in pool votes.
/// Is used to reflect user's vxASTRO balance changes in the emissions controller contract.
pub fn handle_update_user(
    store: &mut dyn Storage,
    env: Env,
    voter: &str,
    new_voting_power: Uint128,
) -> Result<Response<NeutronMsg>, ContractError> {
    if let Some(user_info) = USER_INFO.may_load(store, voter)? {
        let block_ts = env.block.time.seconds();

        let whitelist: HashSet<_> = POOLS_WHITELIST.load(store)?.into_iter().collect();
        user_info
            .votes
            .iter()
            .filter(|(pool, _)| whitelist.contains(pool.as_str()))
            .try_for_each(|(pool, weight)| {
                let pool_info = VOTED_POOLS.load(store, pool)?;
                // Subtract old vote from pool voting power if pool wasn't reset to 0
                let pool_dedicated_vp = if pool_info.init_ts <= user_info.vote_ts {
                    user_info
                        .voting_power
                        .multiply_ratio(weight.numerator(), weight.denominator())
                } else {
                    Uint128::zero()
                };
                let add_vp =
                    new_voting_power.multiply_ratio(weight.numerator(), weight.denominator());

                let new_pool_info = pool_info.with_sub_vp(pool_dedicated_vp).with_add_vp(add_vp);
                VOTED_POOLS.save(store, pool, &new_pool_info, block_ts)
            })?;

        // Updating only voting power
        USER_INFO.save(
            store,
            voter,
            &UserInfo {
                voting_power: new_voting_power,
                ..user_info
            },
            block_ts,
        )?;

        Ok(Response::default().add_attributes([
            attr("action", "update_user_votes"),
            attr("voter", voter),
            attr("old_voting_power", user_info.voting_power),
            attr("new_voting_power", new_voting_power),
        ]))
    } else {
        Ok(Response::default())
    }
}

/// The function checks that the last pools tuning happened >= 14 days ago.
/// Then it calculates voting power per each pool.
/// takes top X pools by voting power, where X is
/// 'config.pools_per_outpost' * number of outposts,
/// calculates total ASTRO emission amount for upcoming epoch,
/// distributes it between selected pools
/// and sends emission messages to each outpost.
pub fn tune_pools(
    deps: DepsMut<NeutronQuery>,
    env: Env,
) -> Result<Response<NeutronMsg>, ContractError> {
    let tune_info = TUNE_INFO.load(deps.storage)?;
    let block_ts = env.block.time.seconds();

    ensure!(
        tune_info.tune_ts + EPOCH_LENGTH <= block_ts,
        ContractError::TuneCooldown(tune_info.tune_ts + EPOCH_LENGTH)
    );

    let config = CONFIG.load(deps.storage)?;
    let ibc_fee = min_ntrn_ibc_fee(deps.as_ref())?;
    let deps = deps.into_empty();

    let voted_pools = VOTED_POOLS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<HashSet<_>>>()?;
    let outposts = OUTPOSTS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<HashMap<_, _>>>()?;
    let epoch_start = get_epoch_start(block_ts);

    let TuneResult {
        candidates,
        new_emissions_state,
        next_pools_grouped,
    } = simulate_tune(deps.as_ref(), &voted_pools, &outposts, epoch_start, &config)?;

    let total_pool_limit = config.pools_per_outpost as usize * outposts.len();

    // If candidates list size is more than the total pool number limit,
    // we need to whitelist all candidates
    // and those which have more than the threshold voting power.
    // Otherwise, keep the current whitelist.
    if candidates.len() > total_pool_limit {
        let total_vp = candidates
            .iter()
            .fold(Uint128::zero(), |acc, (_, (_, vp))| acc + vp);

        let new_whitelist: HashSet<_> = candidates
            .iter()
            .skip(total_pool_limit)
            .filter(|(_, (_, pool_vp))| {
                let threshold_vp = total_vp.multiply_ratio(
                    config.whitelist_threshold.numerator(),
                    config.whitelist_threshold.denominator(),
                );
                *pool_vp >= threshold_vp
            })
            .chain(candidates.iter().take(total_pool_limit))
            .map(|(_, (pool, _))| (*pool).clone())
            .collect();

        // Remove all non-whitelisted pools
        voted_pools
            .difference(&new_whitelist)
            .try_for_each(|pool| VOTED_POOLS.remove(deps.storage, pool, block_ts))?;

        POOLS_WHITELIST.save(deps.storage, &new_whitelist.into_iter().collect())?;
    }

    let mut attrs = vec![attr("action", "tune_pools")];
    let mut outpost_emissions_statuses = HashMap::new();
    let setup_pools_msgs = next_pools_grouped
        .iter()
        .map(|(prefix, raw_schedules)| {
            let outpost_info = outposts.get(prefix).unwrap();

            let (schedules, astro_funds) = raw_emissions_to_schedules(
                &env,
                raw_schedules,
                &outpost_info.astro_denom,
                &config.astro_denom,
            );

            let msg = if let Some(params) = &outpost_info.params {
                outpost_emissions_statuses.insert(prefix.clone(), OutpostStatus::InProgress);
                build_emission_ibc_msg(&env, params, &ibc_fee, astro_funds, &schedules)?
            } else {
                let incentives_msg = incentives::ExecuteMsg::IncentivizeMany(schedules);
                wasm_execute(&config.incentives_addr, &incentives_msg, vec![astro_funds])?.into()
            };

            attrs.push(attr("outpost", prefix));
            attrs.push(attr(
                "pools",
                serde_json::to_string(&raw_schedules)
                    .map_err(|err| StdError::generic_err(err.to_string()))?,
            ));

            Ok(msg)
        })
        .collect::<StdResult<Vec<CosmosMsg<NeutronMsg>>>>()?;

    TUNE_INFO.save(
        deps.storage,
        &TuneInfo {
            tune_ts: epoch_start,
            pools_grouped: next_pools_grouped,
            outpost_emissions_statuses,
            emissions_state: new_emissions_state,
        },
        block_ts,
    )?;

    Ok(Response::new()
        .add_messages(setup_pools_msgs)
        .add_attributes(attrs))
}

/// Permissioned to the contract owner.
/// Updates the contract configuration.
pub fn update_config(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    pools_limit: Option<u64>,
    whitelisting_fee: Option<Coin>,
    fee_receiver: Option<String>,
    emissions_multiple: Option<Decimal>,
    max_astro: Option<Uint128>,
) -> Result<Response<NeutronMsg>, ContractError> {
    nonpayable(&info)?;
    let mut config = CONFIG.load(deps.storage)?;

    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    let mut attrs = vec![attr("action", "update_config")];

    if let Some(pools_limit) = pools_limit {
        attrs.push(attr("new_pools_limit", pools_limit.to_string()));
        config.pools_per_outpost = pools_limit;
    }

    if let Some(whitelisting_fee) = whitelisting_fee {
        attrs.push(attr("new_whitelisting_fee", whitelisting_fee.to_string()));
        config.whitelisting_fee = whitelisting_fee;
    }

    if let Some(fee_receiver) = fee_receiver {
        attrs.push(attr("new_fee_receiver", &fee_receiver));
        config.fee_receiver = deps.api.addr_validate(&fee_receiver)?;
    }

    if let Some(emissions_multiple) = emissions_multiple {
        attrs.push(attr(
            "new_emissions_multiple",
            emissions_multiple.to_string(),
        ));
        config.emissions_multiple = emissions_multiple;
    }

    if let Some(max_astro) = max_astro {
        attrs.push(attr("new_max_astro", max_astro.to_string()));
        config.max_astro = max_astro;
    }

    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default().add_attributes(attrs))
}

/// Register an active proposal on all available outposts.
/// Endpoint is permissionless so anyone can retry to register a proposal in case of IBC timeout.
pub fn register_proposal(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    proposal_id: u64,
) -> Result<Response<NeutronMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    // Ensure a proposal exists and active
    let proposal = deps
        .querier
        .query_wasm_smart::<assembly::Proposal>(
            &config.assembly,
            &assembly::QueryMsg::Proposal { proposal_id },
        )
        .and_then(|proposal| {
            ensure!(
                env.block.height <= proposal.end_block,
                StdError::generic_err("Proposal is not active")
            );

            Ok(proposal)
        })?;

    let outposts = OUTPOSTS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?;

    let data = to_json_binary(&VxAstroIbcMsg::RegisterProposal {
        proposal_id,
        start_time: proposal.start_time,
    })?;
    let timeout = IbcTimeout::from(env.block.time.plus_seconds(IBC_TIMEOUT));

    let mut attrs = vec![("action", "register_proposal")];

    let ibc_messages: Vec<CosmosMsg<NeutronMsg>> = outposts
        .iter()
        .filter_map(|(outpost, outpost_info)| {
            outpost_info.params.as_ref().map(|params| {
                attrs.push(("outpost", outpost));
                IbcMsg::SendPacket {
                    channel_id: params.voting_channel.clone(),
                    data: data.clone(),
                    timeout: timeout.clone(),
                }
                .into()
            })
        })
        .collect();

    Ok(Response::new()
        .add_messages(ibc_messages)
        .add_attributes(attrs))
}
