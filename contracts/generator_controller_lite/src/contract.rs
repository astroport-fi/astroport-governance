use std::collections::HashSet;
use std::convert::TryInto;

use crate::astroport;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport_governance::assembly::{
    Config as AssemblyConfig, ExecuteMsg::ExecuteEmissionsProposal,
};
use astroport_governance::astroport::asset::addr_opt_validate;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, Fraction, MessageInfo, Order,
    Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use itertools::Itertools;

use astroport_governance::generator_controller_lite::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, NetworkInfo, QueryMsg,
    UserInfoResponse, VOTERS_MAX_LIMIT,
};
use astroport_governance::utils::{check_contract_supports_channel, get_lite_period};
use astroport_governance::voting_escrow_lite::QueryMsg::CheckVotersAreBlacklisted;
use astroport_governance::voting_escrow_lite::{
    get_emissions_voting_power, get_lock_info, BlacklistedVotersResponse,
};

use crate::bps::BasicPoints;
use crate::error::ContractError;
use crate::state::{
    Config, TuneInfo, UserInfo, VotedPoolInfo, CONFIG, OWNERSHIP_PROPOSAL, POOLS, TUNE_INFO,
    USER_INFO,
};

use crate::utils::{
    cancel_user_changes, check_duplicated, determine_address_prefix, filter_pools, get_pool_info,
    group_pools_by_network, update_pool_info, validate_pool, validate_pools_limit, vote_for_pool,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "generator-controller-lite";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

type ExecuteResult = Result<Response, ContractError>;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> ExecuteResult {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            escrow_addr: deps.api.addr_validate(&msg.escrow_addr)?,
            generator_addr: deps.api.addr_validate(&msg.generator_addr)?,
            factory_addr: deps.api.addr_validate(&msg.factory_addr)?,
            assembly_addr: deps.api.addr_validate(&msg.assembly_addr)?,
            hub_addr: addr_opt_validate(deps.api, &msg.hub_addr)?,
            pools_limit: validate_pools_limit(msg.pools_limit)?,
            kick_voters_limit: None,
            main_pool: None,
            main_pool_min_alloc: Decimal::zero(),
            whitelisted_pools: vec![],
            // Set the current network as allowed by default
            whitelisted_networks: vec![NetworkInfo {
                address_prefix: determine_address_prefix(&msg.generator_addr)?,
                generator_address: deps.api.addr_validate(&msg.generator_addr)?,
                ibc_channel: None,
            }],
        },
    )?;

    // Set tune_ts just for safety so the first tuning could happen in 2 weeks
    TUNE_INFO.save(
        deps.storage,
        &TuneInfo {
            tune_period: get_lite_period(env.block.time.seconds())?,
            pool_alloc_points: vec![],
        },
    )?;

    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::KickBlacklistedVoters { blacklisted_voters }** Removes all votes applied by
/// blacklisted voters
///
/// * **ExecuteMsg::KickUnlockedVoters { blacklisted_voters }** Removes all votes applied by
/// voters that started unlocking
///
/// * **ExecuteMsg::KickUnlockedOutpostVoter { blacklisted_voters }** Removes all votes applied by
/// voters that started unlocking on an Outpost
///
/// * **ExecuteMsg::Vote { votes }** Casts votes for pools
///
/// * **ExecuteMsg::OutpostVote { voter, votes, voting_power }** Casts votes for pools from an Outpost
///
/// * **ExecuteMsg::TunePools** Launches pool tuning
///
/// * **ExecuteMsg::ChangePoolsLimit { limit }** Changes the number of pools which are eligible
/// to receive allocation points
///
/// * **ExecuteMsg::UpdateConfig { blacklisted_voters_limit }** Changes the number of blacklisted
/// voters that can be kicked at once
///
/// * **ExecuteMsg::UpdateWhitelist { add, remove }** Adds or removes lp tokens which are eligible
/// to receive votes.
///
/// * **ExecuteMsg::UpdateNetworks { add, remove }** Adds or removes networks mappings for tuning
/// pools on remote chains via a special governance proposal
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change
/// contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> ExecuteResult {
    match msg {
        ExecuteMsg::KickBlacklistedVoters { blacklisted_voters } => {
            kick_blacklisted_voters(deps, env, blacklisted_voters)
        }
        ExecuteMsg::KickUnlockedVoters { unlocked_voters } => {
            kick_unlocked_voters(deps, env, unlocked_voters)
        }
        ExecuteMsg::KickUnlockedOutpostVoter { unlocked_voter } => {
            kick_unlocked_outpost_voter(deps, env, info, unlocked_voter)
        }
        ExecuteMsg::Vote { votes } => handle_vote(deps, env, info, votes),
        ExecuteMsg::OutpostVote {
            voter,
            votes,
            voting_power,
        } => handle_outpost_vote(deps, env, info, voter, votes, voting_power),
        ExecuteMsg::TunePools {} => tune_pools(deps, env),
        ExecuteMsg::ChangePoolsLimit { limit } => change_pools_limit(deps, info, limit),
        ExecuteMsg::UpdateConfig {
            assembly_addr,
            kick_voters_limit,
            main_pool,
            main_pool_min_alloc,
            remove_main_pool,
            hub_addr,
        } => update_config(
            deps,
            info,
            assembly_addr,
            kick_voters_limit,
            main_pool,
            main_pool_min_alloc,
            remove_main_pool,
            hub_addr,
        ),
        ExecuteMsg::UpdateWhitelist { add, remove } => update_whitelist(deps, info, add, remove),
        ExecuteMsg::UpdateNetworks { add, remove } => update_networks(deps, info, add, remove),
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
    }
}

/// Adds or removes lp tokens which are eligible to receive votes.
/// Returns a [`ContractError`] on failure.
fn update_whitelist(
    deps: DepsMut,
    info: MessageInfo,
    add: Option<Vec<String>>,
    remove: Option<Vec<String>>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    // Remove old LP tokens
    if let Some(remove_lp_tokens) = remove {
        config
            .whitelisted_pools
            .retain(|pool| !remove_lp_tokens.contains(&pool.to_string()));
    }

    // Add new lp tokens
    if let Some(add_lp_tokens) = add {
        config.whitelisted_pools.append(
            &mut add_lp_tokens
                .into_iter()
                .map(|lp_token| {
                    validate_pool(&config, &lp_token)?;
                    Ok(lp_token)
                })
                .collect::<Result<Vec<_>, ContractError>>()?,
        );
        check_duplicated(&config.whitelisted_pools).map_err(|_|
            ContractError::Std(StdError::generic_err("The resulting whitelist contains duplicated pools. It's either provided 'add' list contains duplicated pools or some of the added pools are already whitelisted.")))?;
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default().add_attribute("action", "update_whitelist"))
}

/// Adds or removes networks mappings for tuning
/// pools on remote chains via a special governance proposal
/// Returns a [`ContractError`] on failure.
fn update_networks(
    deps: DepsMut,
    info: MessageInfo,
    add: Option<Vec<NetworkInfo>>,
    remove: Option<Vec<String>>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    // Handle removals
    // The network added in instantiate, ie. the network of the contract itself, cannot be removed
    if let Some(remove_prefixes) = remove {
        let native_prefix = determine_address_prefix(config.generator_addr.as_ref())?;

        if remove_prefixes.contains(&native_prefix) {
            return Err(ContractError::Std(StdError::generic_err(format!(
                "Cannot remove the native network with prefix {}",
                native_prefix
            ))));
        }

        config
            .whitelisted_networks
            .retain(|network| !remove_prefixes.contains(&network.address_prefix));
    }

    let mut response = Response::default().add_attribute("action", "update_networks");
    if let Some(add_prefix) = add {
        // Get the assembly contract to check if the controller supports a specific channel
        let assembly_config: AssemblyConfig = deps
            .querier
            .query_wasm_smart(config.assembly_addr.clone(), &QueryMsg::Config {})?;

        config.whitelisted_networks.append(
            &mut add_prefix
                .into_iter()
                .map(|mut network_info| {
                    // If the IBC channel is set, check if the controller supports it
                    if let Some(ibc_channel) = network_info.ibc_channel.clone() {
                        match &assembly_config.ibc_controller {
                            Some(ibc_controller) => {
                                check_contract_supports_channel(
                                    deps.querier,
                                    ibc_controller,
                                    &ibc_channel,
                                )?;
                            }
                            None => {
                                return Err(ContractError::Std(StdError::generic_err(
                                    "The Assembly does not have an IBC controller set",
                                )))
                            }
                        }
                    }
                    // Determine the prefix based on the generator address
                    network_info.address_prefix =
                        determine_address_prefix(network_info.generator_address.as_ref())?;
                    Ok(network_info)
                })
                .collect::<Result<Vec<_>, ContractError>>()?,
        );
        let prefixes: Vec<String> = config
            .whitelisted_networks
            .iter()
            .map(|info| info.address_prefix.clone())
            .collect();
        check_duplicated(&prefixes).map_err(|_|
            ContractError::Std(StdError::generic_err("The resulting whitelist contains duplicated prefixes. It's either provided 'add' list contains duplicated prefixes or some of the added prefixes are already whitelisted.")))?;
        // Emit added prefixes
        response = response.add_attribute("added", prefixes.join(","));
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(response)
}

/// This function removes all votes applied by blacklisted voters.
///
/// * **holders** list with blacklisted holders whose votes will be removed.
fn kick_blacklisted_voters(deps: DepsMut, env: Env, voters: Vec<String>) -> ExecuteResult {
    let block_period = get_lite_period(env.block.time.seconds())?;
    let config = CONFIG.load(deps.storage)?;

    if voters.len() > config.kick_voters_limit.unwrap_or(VOTERS_MAX_LIMIT) as usize {
        return Err(ContractError::KickVotersLimitExceeded {});
    }

    // Check duplicated voters
    let addrs_set = voters.iter().collect::<HashSet<_>>();
    if voters.len() != addrs_set.len() {
        return Err(ContractError::DuplicatedVoters {});
    }

    // Check if voters are blacklisted
    let res: BlacklistedVotersResponse = deps.querier.query_wasm_smart(
        config.escrow_addr,
        &CheckVotersAreBlacklisted {
            voters: voters.clone(),
        },
    )?;

    if !res.eq(&BlacklistedVotersResponse::VotersBlacklisted {}) {
        return Err(ContractError::Std(StdError::generic_err(res.to_string())));
    }

    for voter in voters {
        if let Some(user_info) = USER_INFO.may_load(deps.storage, &voter)? {
            // Cancel changes applied by previous votes immediately
            user_info.votes.iter().try_for_each(|(pool_addr, bps)| {
                cancel_user_changes(
                    deps.storage,
                    block_period,
                    pool_addr,
                    *bps,
                    user_info.voting_power,
                )
            })?;

            let user_info = UserInfo {
                vote_period: Some(block_period),
                ..Default::default()
            };

            USER_INFO.save(deps.storage, &voter, &user_info)?;
        }
    }

    Ok(Response::new().add_attribute("action", "kick_blocklisted_holders"))
}

/// This function removes all votes applied by unlocked voters.
///
/// * **holders** list with unlocked holders whose votes will be removed.
fn kick_unlocked_voters(deps: DepsMut, env: Env, voters: Vec<String>) -> ExecuteResult {
    let block_period = get_lite_period(env.block.time.seconds())?;
    let config = CONFIG.load(deps.storage)?;

    if voters.len() > config.kick_voters_limit.unwrap_or(VOTERS_MAX_LIMIT) as usize {
        return Err(ContractError::KickVotersLimitExceeded {});
    }

    // Check duplicated voters
    let addrs_set = voters.iter().collect::<HashSet<_>>();
    if voters.len() != addrs_set.len() {
        return Err(ContractError::DuplicatedVoters {});
    }

    for voter in voters {
        let lock_info = get_lock_info(&deps.querier, config.escrow_addr.clone(), voter.clone())?;
        if lock_info.end.is_none() {
            // This voter has not unlocked
            return Err(ContractError::AddressIsLocked(voter));
        }

        if let Some(user_info) = USER_INFO.may_load(deps.storage, &voter)? {
            // Cancel changes applied by previous votes immediately
            user_info.votes.iter().try_for_each(|(pool_addr, bps)| {
                cancel_user_changes(
                    deps.storage,
                    block_period,
                    pool_addr,
                    *bps,
                    user_info.voting_power,
                )
            })?;

            let user_info = UserInfo {
                vote_period: Some(block_period),
                ..Default::default()
            };

            USER_INFO.save(deps.storage, &voter, &user_info)?;
        }
    }

    Ok(Response::new().add_attribute("action", "kick_holders"))
}

/// This function removes all votes applied by an unlocked voters from an Outpost.
///
/// * **voter** the unlocked holder whose votes will be removed.
fn kick_unlocked_outpost_voter(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    voter: String,
) -> ExecuteResult {
    let config = CONFIG.load(deps.storage)?;

    // We only allow the Hub to kick a voter from an Outpost
    let hub = match config.hub_addr {
        Some(hub) => hub,
        None => return Err(ContractError::InvalidHub {}),
    };

    if info.sender != hub {
        return Err(ContractError::Unauthorized {});
    }

    let block_period = get_lite_period(env.block.time.seconds())?;
    if let Some(user_info) = USER_INFO.may_load(deps.storage, &voter)? {
        // Cancel changes applied by previous votes immediately
        user_info.votes.iter().try_for_each(|(pool_addr, bps)| {
            cancel_user_changes(
                deps.storage,
                block_period,
                pool_addr,
                *bps,
                user_info.voting_power,
            )
        })?;

        let user_info = UserInfo {
            vote_period: Some(block_period),
            ..Default::default()
        };

        USER_INFO.save(deps.storage, &voter, &user_info)?;
    }

    Ok(Response::new().add_attribute("action", "kick_outpost_holders"))
}

/// Handles a vote on the current chain.
///
/// * **votes** is a vector of pairs ([`String`], [`u16`]).
/// Tuple consists of pool address and percentage of user's voting power for a given pool.
/// Percentage should be in BPS form.
fn handle_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    votes: Vec<(String, u16)>,
) -> ExecuteResult {
    let user = info.sender.to_string();
    let config = CONFIG.load(deps.storage)?;
    let user_vp = get_emissions_voting_power(&deps.querier, &config.escrow_addr, &user)?;

    apply_vote(deps, env, user, user_vp, config, votes)?;

    Ok(Response::new().add_attribute("action", "vote"))
}

/// Handles a vote from an Outpost.
///
/// * **voter** is the address of the voter from the Outpost.
///
/// * **votes** is a vector of pairs ([`String`], [`u16`]).
/// Tuple consists of pool address and percentage of user's voting power for a given pool.
/// Percentage should be in BPS form.
///
/// * **voting_power** is voting power of the voter from the Outpost as validated by the Hub.
fn handle_outpost_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    voter: String,
    votes: Vec<(String, u16)>,
    voting_power: Uint128,
) -> ExecuteResult {
    let config = CONFIG.load(deps.storage)?;

    // We only allow the Hub to submit emission votes on behalf of Outpost user
    // The Hub is responsible for validating the Hub vote with the Outpost
    let hub = match config.hub_addr.clone() {
        Some(hub) => hub,
        None => return Err(ContractError::InvalidHub {}),
    };

    if info.sender != hub {
        return Err(ContractError::Unauthorized {});
    }

    apply_vote(deps, env, voter, voting_power, config, votes)?;

    Ok(Response::new().add_attribute("action", "outpost_vote"))
}

/// Apply the votes for the given user
///
/// The function checks that:
/// * the user voting power is > 0,
/// * user didn't vote in this period,
/// * 'votes' vector doesn't contain duplicated pool addresses,
/// * sum of all BPS values <= 10000.
///
/// The function cancels changes applied by previous votes and apply new votes for the this period.
/// New vote parameters are saved in [`USER_INFO`].
fn apply_vote(
    deps: DepsMut,
    env: Env,
    voter: String,
    voting_power: Uint128,
    config: ConfigResponse,
    votes: Vec<(String, u16)>,
) -> Result<(), ContractError> {
    if voting_power.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    if config.whitelisted_pools.is_empty() {
        return Err(ContractError::WhitelistEmpty {});
    }

    let user_info = USER_INFO
        .may_load(deps.storage, &voter)?
        .unwrap_or_default();

    let block_period = get_lite_period(env.block.time.seconds())?;
    if let Some(vote_period) = user_info.vote_period {
        if vote_period == block_period {
            return Err(ContractError::CooldownError {});
        }
    }

    // Has the user voted in this period?
    check_duplicated(
        &votes
            .iter()
            .map(|vote| {
                let (lp_token, _) = vote;
                lp_token
            })
            .collect::<Vec<_>>(),
    )?;

    // Validating addrs and bps
    let votes = votes
        .into_iter()
        .map(|(addr, bps)| {
            // Voting for the main pool is prohibited
            if let Some(main_pool) = &config.main_pool {
                if addr == *main_pool {
                    return Err(ContractError::MainPoolVoteOrWhitelistingProhibited(
                        main_pool.to_string(),
                    ));
                }
            }
            if !config.whitelisted_pools.contains(&addr) {
                return Err(ContractError::PoolIsNotWhitelisted(addr));
            }

            validate_pool(&config, &addr)?;

            let bps: BasicPoints = bps.try_into()?;
            Ok((addr, bps))
        })
        .collect::<Result<Vec<_>, ContractError>>()?;

    // Check the bps sum is within the limit
    votes
        .iter()
        .try_fold(BasicPoints::default(), |acc, (_, bps)| {
            acc.checked_add(*bps)
        })?;

    // Cancel changes applied by previous votes
    user_info.votes.iter().try_for_each(|(pool_addr, bps)| {
        cancel_user_changes(
            deps.storage,
            block_period,
            pool_addr,
            *bps,
            user_info.voting_power,
        )
    })?;

    // Votes are applied to current period
    // In vxASTRO lite, voting power is removed immediately
    // when a user unlocks
    votes.iter().try_for_each(|(pool_addr, bps)| {
        vote_for_pool(
            deps.storage,
            block_period,
            pool_addr.as_str(),
            *bps,
            voting_power,
        )
    })?;

    let user_info = UserInfo {
        vote_period: Some(block_period),
        voting_power,
        votes,
    };

    Ok(USER_INFO.save(deps.storage, &voter, &user_info)?)
}

/// The function checks that the last pools tuning happened >= 14 days ago.
/// Then it calculates voting power for each pool at the current period, filters all pools which
/// are not eligible to receive allocation points,
/// takes top X pools by voting power, where X is 'config.pools_limit', calculates allocation points
/// for these pools and applies allocation points in generator contract.
///
/// For pools on the same network (e.g. Terra), the allocation points are set
/// directly on the generator. For pools on different networks (e.g. Injective),
/// we create a special governance proposal to set the allocation points on the
/// remote generator.
///
/// We determine the network of a pool by looking at the address prefix.
fn tune_pools(deps: DepsMut, env: Env) -> ExecuteResult {
    let mut tune_info = TUNE_INFO.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let block_period = get_lite_period(env.block.time.seconds())?;

    if tune_info.tune_period == block_period {
        return Err(ContractError::CooldownError {});
    }

    // We're tuning pools based on the previous voting period
    let tune_period = block_period - 1;
    let pool_votes: Vec<_> = POOLS
        .keys(deps.as_ref().storage, None, None, Order::Ascending)
        .collect::<Vec<_>>()
        .into_iter()
        .map(|pool_addr| {
            let pool_addr = pool_addr?;

            let pool_info = update_pool_info(deps.storage, tune_period, &pool_addr, None)?;
            // Remove pools with zero voting power so we won't iterate over them in future
            if pool_info.vxastro_amount.is_zero() {
                POOLS.remove(deps.storage, &pool_addr)
            }
            Ok((pool_addr, pool_info.vxastro_amount))
        })
        .collect::<StdResult<Vec<_>>>()?
        .into_iter()
        .filter(|(_, vxastro_amount)| !vxastro_amount.is_zero())
        .sorted_by(|(_, a), (_, b)| b.cmp(a)) // Sort in descending order
        .collect();

    // Filter pools which are not eligible to receive allocation points
    // Pools might be on a different chain and thus not much can be done in
    // terms of validation. That will be handled via governance proposals and
    // the whitelist
    tune_info.pool_alloc_points = filter_pools(
        pool_votes,
        config.pools_limit + 1, // +1 additional pool if we will need to remove the main pool
    )?;

    // Set allocation points for the main pool
    match config.main_pool {
        Some(main_pool) if !config.main_pool_min_alloc.is_zero() => {
            // Main pool may appear in the pool list thus we need to eliminate its contribution in the total VP.
            tune_info
                .pool_alloc_points
                .retain(|(pool, _)| pool != &main_pool.to_string());
            // If there is no main pool in the filtered list then we need to remove additional pool
            tune_info.pool_alloc_points = tune_info
                .pool_alloc_points
                .iter()
                .take(config.pools_limit as usize)
                .cloned()
                .collect();

            let total_vp: Uint128 = tune_info
                .pool_alloc_points
                .iter()
                .fold(Uint128::zero(), |acc, (_, vp)| acc + vp);
            // Calculate main pool contribution.
            // Example (30% for the main pool): VP + x = y, x = 0.3y => y = VP/0.7  => x = 0.3 * VP / 0.7,
            // where VP - total VP, x - main pool's contribution, y - new total VP.
            // x = 0.3 * VP * (1-0.3)^(-1)
            let main_pool_contribution = config.main_pool_min_alloc
                * total_vp
                * (Decimal::one() - config.main_pool_min_alloc).inv().unwrap();
            tune_info
                .pool_alloc_points
                .push((main_pool.to_string(), main_pool_contribution))
        }
        _ => {
            // there is no main pool or min alloc is 0%
            tune_info.pool_alloc_points = tune_info
                .pool_alloc_points
                .iter()
                .take(config.pools_limit as usize)
                .cloned()
                .collect();
        }
    }

    if tune_info.pool_alloc_points.is_empty() {
        return Err(ContractError::TuneNoPools {});
    }

    // Tuning can only happen once per period. As we're tuning for the previous
    // period, we set this to the current period
    tune_info.tune_period = block_period;
    TUNE_INFO.save(deps.storage, &tune_info)?;

    // Split pools by network and send separate messages for each network
    let grouped_pools = group_pools_by_network(&config.whitelisted_networks, &tune_info);

    let mut response = Response::new().add_attribute("action", "tune_pools");
    for (network_info, pool_alloc_points) in &grouped_pools {
        // The message to set the allocation points on the generator, either
        // directly or via a governance proposal for Outposts
        let setup_pools_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: network_info.generator_address.to_string(),
            msg: to_binary(&astroport::generator::ExecuteMsg::SetupPools {
                pools: pool_alloc_points.to_vec(),
            })?,
            funds: vec![],
        });

        match &network_info.ibc_channel {
            // If the channel is empty, then this is setting up pools on the network
            // we are deployed on and we can continue as normal
            None => {
                response = response
                    .add_attribute("tune", network_info.address_prefix.to_string())
                    .add_attribute("pool_count", pool_alloc_points.len().to_string())
                    .add_message(setup_pools_msg);
            }
            // If the channel is not empty, then this is setting up pools on an
            // Outpost
            Some(ibc_channel) => {
                // We need to submit the setup pools message to the
                // Assembly as a proposal to execute on the remote chain
                let proposal_msg = to_binary(&ExecuteEmissionsProposal {
                    title: format!(
                        // Sample title: "Update emissions on the inj outpost", "Update emissions on the neutron outpost"
                        "Update emissions on the {} outpost",
                        network_info.address_prefix
                    ),
                    description: format!(
                        // Sample title: "This proposal aims to update emissions on the inj outpost using IBC channel-2"
                        "This proposal aims to update emissions on the {} outpost using IBC {}",
                        network_info.address_prefix, ibc_channel
                    ),
                    messages: vec![setup_pools_msg],
                    ibc_channel: Some(ibc_channel.to_string()),
                })?;

                let setup_pools_assembly_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: config.assembly_addr.to_string(),
                    msg: proposal_msg,
                    funds: vec![],
                });

                response = response
                    .add_attribute("tune", network_info.address_prefix.to_string())
                    .add_attribute("channel", ibc_channel)
                    .add_attribute("pool_count", pool_alloc_points.len().to_string())
                    .add_message(setup_pools_assembly_msg);
            }
        }
    }
    Ok(response)
}

/// Only contract owner can call this function.  
/// The function sets a new limit of blacklisted voters that can be kicked at once.
///
/// * **assembly_addr** is a new address of the Assembly contract
///
/// * **kick_voters_limit** is a new limit of blacklisted or unlocked voters which can be kicked at once
///
/// * **main_pool** is a main pool address
///
/// * **main_pool_min_alloc** is a minimum percentage of ASTRO emissions that this pool should get every block
///
/// * **remove_main_pool** should the main pool be removed or not
#[allow(clippy::too_many_arguments)]
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    assembly_addr: Option<String>,
    kick_voters_limit: Option<u32>,
    main_pool: Option<String>,
    main_pool_min_alloc: Option<Decimal>,
    remove_main_pool: Option<bool>,
    hub_addr: Option<String>,
) -> ExecuteResult {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(assembly_addr) = assembly_addr {
        config.assembly_addr = deps.api.addr_validate(&assembly_addr)?;
    }

    if let Some(kick_voters_limit) = kick_voters_limit {
        config.kick_voters_limit = Some(kick_voters_limit);
    }

    if let Some(main_pool_min_alloc) = main_pool_min_alloc {
        if main_pool_min_alloc == Decimal::zero() || main_pool_min_alloc >= Decimal::one() {
            return Err(ContractError::MainPoolMinAllocFailed {});
        }
        config.main_pool_min_alloc = main_pool_min_alloc;
    }

    if let Some(main_pool) = main_pool {
        if config.main_pool_min_alloc.is_zero() {
            return Err(StdError::generic_err("Main pool min alloc can not be zero").into());
        }
        config.main_pool = Some(deps.api.addr_validate(&main_pool)?);
    }

    if let Some(remove_main_pool) = remove_main_pool {
        if remove_main_pool {
            config.main_pool = None;
        }
    }

    if let Some(hub_addr) = hub_addr {
        config.hub_addr = Some(deps.api.addr_validate(&hub_addr)?);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default().add_attribute("action", "update_config"))
}

/// Only contract owner can call this function.
/// The function sets new limit of pools which are eligible to receive allocation points.
///
/// * **limit** is a new limit of pools which are eligible to receive allocation points.
fn change_pools_limit(deps: DepsMut, info: MessageInfo, limit: u64) -> ExecuteResult {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    config.pools_limit = validate_pools_limit(limit)?;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default().add_attribute("action", "change_pools_limit"))
}

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::UserInfo { user }** Fetch user information
///
/// * **QueryMsg::TuneInfo** Fetch last tuning information
///
/// * **QueryMsg::Config** Fetch contract config
///
/// * **QueryMsg::PoolInfo { pool_addr }** Fetch pool's voting information at the current period.
///
/// * **QueryMsg::PoolInfoAtPeriod { pool_addr, period }** Fetch pool's voting information at a specified period.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::UserInfo { user } => to_binary(&user_info(deps, user)?),
        QueryMsg::TuneInfo {} => to_binary(&TUNE_INFO.load(deps.storage)?),
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::PoolInfo { pool_addr } => to_binary(&pool_info(deps, env, pool_addr, None)?),
        QueryMsg::PoolInfoAtPeriod { pool_addr, period } => {
            to_binary(&pool_info(deps, env, pool_addr, Some(period))?)
        }
    }
}

/// Returns user information.
fn user_info(deps: Deps, user: String) -> StdResult<UserInfoResponse> {
    USER_INFO
        .may_load(deps.storage, &user)?
        .map(UserInfo::into_response)
        .ok_or_else(|| StdError::generic_err("User not found"))
}

/// Returns pool's voting information at a specified period.
fn pool_info(
    deps: Deps,
    env: Env,
    pool_addr: String,
    period: Option<u64>,
) -> StdResult<VotedPoolInfo> {
    let block_period = get_lite_period(env.block.time.seconds())?;
    let period = period.unwrap_or(block_period);
    get_pool_info(deps.storage, period, &pool_addr)
}

/// Manages contract migration
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Err(ContractError::MigrationError {})
}
