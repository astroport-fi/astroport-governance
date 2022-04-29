use std::collections::HashSet;
use std::convert::TryInto;

use astroport::asset::{addr_validate_to_lower, pair_info_by_pool};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError,
    StdResult, WasmMsg,
};
use cw2::set_contract_version;
use itertools::Itertools;

use astroport_governance::generator_controller::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, UserInfoResponse,
};
use astroport_governance::utils::{calc_voting_power, get_period, WEEK};
use astroport_governance::voting_escrow::{get_lock_info, get_voting_power};

use crate::bps::BasicPoints;
use crate::error::ContractError;
use crate::state::{
    Config, TuneInfo, UserInfo, VotedPoolInfo, CONFIG, OWNERSHIP_PROPOSAL, POOLS, TUNE_INFO,
    USER_INFO,
};
use crate::utils::{
    cancel_user_changes, filter_pools, get_pool_info, update_pool_info, validate_pools_limit,
    vote_for_pool,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astro-generator-controller";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const DAY: u64 = 86400;
/// The user can only vote once every 10 days
const VOTE_COOLDOWN: u64 = DAY * 10;
/// It is possible to tune pools once every 14 days
const TUNE_COOLDOWN: u64 = WEEK * 2;

type ExecuteResult = Result<Response, ContractError>;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
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
            owner: addr_validate_to_lower(deps.api, &msg.owner)?,
            escrow_addr: addr_validate_to_lower(deps.api, &msg.escrow_addr)?,
            generator_addr: addr_validate_to_lower(deps.api, &msg.generator_addr)?,
            factory_addr: addr_validate_to_lower(deps.api, &msg.factory_addr)?,
            pools_limit: validate_pools_limit(msg.pools_limit)?,
        },
    )?;

    // Set tune_ts just for safety so the first tuning could happen in 2 weeks
    TUNE_INFO.save(
        deps.storage,
        &TuneInfo {
            tune_ts: env.block.time.seconds(),
            pool_alloc_points: vec![],
        },
    )?;

    Ok(Response::default())
}

/// ## Description
/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::Vote { votes }** Casts votes for pools
///
/// * **ExecuteMsg::TunePools** Launches pool tuning
///
/// * **ExecuteMsg::ChangePoolLimit { limit }** Changes the number of pools which are eligible to receive allocation points
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> ExecuteResult {
    match msg {
        ExecuteMsg::Vote { votes } => handle_vote(deps, env, info, votes),
        ExecuteMsg::TunePools {} => tune_pools(deps, env),
        ExecuteMsg::ChangePoolsLimit { limit } => change_pools_limit(deps, info, limit),
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
    }
}

/// ## Description
/// The function checks that:
/// * the user voting power is > 0,
/// * user didn't vote for last 10 days,
/// * all pool addresses are valid LP token addresses,
/// * 'votes' vector doesn't contain duplicated pool addresses,
/// * sum of all BPS values <= 10000.
///
/// The function cancels changes applied by previous votes and apply new votes for the next period.
/// New vote parameters are saved in [`USER_INFO`].
///
/// The function returns [`Response`] in case of success or [`ContractError`] in case of errors.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
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
    let user = info.sender;
    let block_period = get_period(env.block.time.seconds())?;
    let escrow_addr = CONFIG.load(deps.storage)?.escrow_addr;
    let user_vp = get_voting_power(deps.querier, &escrow_addr, &user)?;

    if user_vp.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let user_info = USER_INFO.may_load(deps.storage, &user)?.unwrap_or_default();
    // Does the user eligible to vote again?
    if env.block.time.seconds() - user_info.vote_ts < VOTE_COOLDOWN {
        return Err(ContractError::CooldownError(VOTE_COOLDOWN / DAY));
    }

    // Check duplicated votes
    let addrs_set = votes
        .iter()
        .cloned()
        .map(|(addr, _)| addr)
        .collect::<HashSet<_>>();
    if votes.len() != addrs_set.len() {
        return Err(ContractError::DuplicatedPools {});
    }

    // Validating addrs and bps
    let votes = votes
        .into_iter()
        .map(|(addr, bps)| {
            let addr = addr_validate_to_lower(deps.api, &addr)?;
            // Check an address is a lp token
            pair_info_by_pool(deps.as_ref(), addr.clone())
                .map_err(|_| ContractError::InvalidLPTokenAddress(addr.to_string()))?;
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

    if user_info.lock_end > block_period {
        let user_last_vote_period = get_period(user_info.vote_ts).unwrap_or(block_period);
        // Calculate voting power before changes
        let old_vp_at_period = calc_voting_power(
            user_info.slope,
            user_info.voting_power,
            user_last_vote_period,
            block_period,
        );

        // Cancel changes applied by previous votes
        user_info.votes.iter().try_for_each(|(pool_addr, bps)| {
            cancel_user_changes(
                deps.storage,
                block_period + 1,
                pool_addr,
                *bps,
                old_vp_at_period,
                user_info.slope,
                user_info.lock_end,
            )
        })?;
    }

    let ve_lock_info = get_lock_info(deps.querier, &escrow_addr, &user)?;

    // Votes are applied to the next period
    votes.iter().try_for_each(|(pool_addr, bps)| {
        vote_for_pool(
            deps.storage,
            block_period + 1,
            pool_addr,
            *bps,
            user_vp,
            ve_lock_info.slope,
            ve_lock_info.end,
        )
    })?;

    let user_info = UserInfo {
        vote_ts: env.block.time.seconds(),
        voting_power: user_vp,
        slope: ve_lock_info.slope,
        lock_end: ve_lock_info.end,
        votes,
    };

    USER_INFO.save(deps.storage, &user, &user_info)?;

    Ok(Response::new().add_attribute("action", "vote"))
}

/// ## Description
/// Only contract owner can call this function.
/// The function checks that the last pools tuning happened >= 14 days ago.
/// Then it calculates voting power for each pool at the current period, filters all pools which
/// are not eligible to receive allocation points,
/// takes top X pools by voting power, where X is 'config.pools_limit', calculates allocation points
/// for these pools and applies allocation points in generator contract.
/// The function returns [`Response`] in case of success or [`ContractError`] in case of errors.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
fn tune_pools(deps: DepsMut, env: Env) -> ExecuteResult {
    let mut tune_info = TUNE_INFO.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let block_period = get_period(env.block.time.seconds())?;

    if env.block.time.seconds() - tune_info.tune_ts < TUNE_COOLDOWN {
        return Err(ContractError::CooldownError(TUNE_COOLDOWN / DAY));
    }

    let pool_votes: Vec<_> = POOLS
        .keys(deps.as_ref().storage, None, None, Order::Ascending)
        .collect::<Vec<_>>()
        .into_iter()
        .map(|pool_addr_serialized| {
            let pool_addr = String::from_utf8(pool_addr_serialized)
                .map_err(|_| StdError::generic_err("Deserialization error"))
                .and_then(|pool_addr_string| addr_validate_to_lower(deps.api, &pool_addr_string))?;
            let pool_info = update_pool_info(deps.storage, block_period, &pool_addr, None)?;
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

    tune_info.pool_alloc_points = filter_pools(
        deps.as_ref(),
        &config.generator_addr,
        &config.factory_addr,
        pool_votes,
        config.pools_limit,
    )?;

    if tune_info.pool_alloc_points.is_empty() {
        return Err(ContractError::TuneNoPools {});
    }

    // Set new alloc points
    let setup_pools_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.generator_addr.to_string(),
        msg: to_binary(&astroport::generator::ExecuteMsg::SetupPools {
            pools: tune_info.pool_alloc_points.clone(),
        })?,
        funds: vec![],
    });

    tune_info.tune_ts = env.block.time.seconds();
    TUNE_INFO.save(deps.storage, &tune_info)?;

    Ok(Response::new()
        .add_message(setup_pools_msg)
        .add_attribute("action", "tune_pools"))
}

/// ## Description
/// Only contract owner can call this function.
/// The function sets new limit of pools which are eligible to receive allocation points.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
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

/// # Description
/// Expose available contract queries.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
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

/// # Description
/// Returns user information using a [`UserInfoResponse`] object.
fn user_info(deps: Deps, user: String) -> StdResult<UserInfoResponse> {
    let user_addr = addr_validate_to_lower(deps.api, &user)?;
    USER_INFO
        .may_load(deps.storage, &user_addr)?
        .map(UserInfo::into_response)
        .ok_or_else(|| StdError::generic_err("User not found"))
}

/// # Description
/// Returns pool's voting information using a [`VotedPoolInfo`] object at a specified period.
fn pool_info(
    deps: Deps,
    env: Env,
    pool_addr: String,
    period: Option<u64>,
) -> StdResult<VotedPoolInfo> {
    let pool_addr = addr_validate_to_lower(deps.api, &pool_addr)?;
    let block_period = get_period(env.block.time.seconds())?;
    let period = period.unwrap_or(block_period);
    get_pool_info(deps.storage, period, &pool_addr)
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
