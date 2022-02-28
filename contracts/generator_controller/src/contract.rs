use crate::bps::BasicPoints;
use crate::error::ContractError;
use crate::state::{
    Config, GaugeInfo, UserInfo, VotedPoolInfo, CONFIG, GAUGE_INFO, POOL_VOTES, USER_INFO,
};
use crate::utils::{
    cancel_user_changes, deserialize_pair, get_lock_info, get_voting_power, vote_for_pool,
};
use astroport::asset::addr_validate_to_lower;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::DecimalCheckedOps;
use astroport_governance::generator_controller::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use astroport_governance::utils::{calc_voting_power, get_period, WEEK};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError,
    StdResult, Uint128, Uint64,
};
use cw2::set_contract_version;
use cw_storage_plus::U64Key;
use std::convert::TryInto;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astro-generator-controller";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const DAY: u64 = 86400;
/// The user can only vote once every 10 days
const VOTE_COOLDOWN: u64 = DAY * 10;
/// The owner can only gauge generators once every 14 days
const GAUGE_COOLDOWN: u64 = WEEK * 2;

type ExecuteResult = Result<Response, ContractError>;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
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
            pools_limit: msg.pools_limit,
        },
    )?;

    Ok(Response::default())
}

/// ## Description
/// Parses execute message and routes it to intended function. Returns [`Response`] if execution succeed
/// or [`ContractError`] if error occurred.
///  
/// ## Execute messages
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> ExecuteResult {
    match msg {
        ExecuteMsg::Vote { votes } => handle_vote(deps, env, info, votes),
        ExecuteMsg::GaugeGenerators {} => gauge_generators(deps, env, info),
    }
}

fn handle_vote(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    votes: Vec<(String, u16)>,
) -> ExecuteResult {
    let user = info.sender;
    let block_period = get_period(env.block.time.seconds());
    let escrow_addr = CONFIG.load(deps.storage)?.escrow_addr;
    let user_vp = get_voting_power(&escrow_addr, &user)?;

    if user_vp.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let lock_info = get_lock_info(&escrow_addr, &user)?;
    if lock_info.end <= block_period + 1 {
        return Err(ContractError::LockExpiresSoon {});
    }

    let user_info = USER_INFO.may_load(deps.storage, &user)?.unwrap_or_default();
    // Does the user eligible to vote again?
    if env.block.time.seconds() - user_info.vote_ts < VOTE_COOLDOWN {
        return Err(ContractError::CooldownError(VOTE_COOLDOWN / DAY));
    }

    // Validating addrs and bps
    let votes = votes
        .into_iter()
        .map(|(addr, bps)| {
            let addr = addr_validate_to_lower(deps.api, &addr)?;
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

    let mut old_vp_at_period = Uint128::zero();
    if !user_info.slope.is_zero() {
        // Calculate voting power before changes
        old_vp_at_period = calc_voting_power(
            user_info.slope,
            user_info.voting_power,
            get_period(user_info.vote_ts),
            block_period,
        );
        // Cancel changes applied by previous votes
        user_info.votes.iter().try_for_each(|(pool_addr, bps)| {
            let pool_votes_path = POOL_VOTES.key((U64Key::new(block_period), &pool_addr));
            cancel_user_changes(
                deps.branch(),
                pool_votes_path,
                *bps,
                user_info.slope,
                old_vp_at_period,
            )
        })?;
    }

    // Votes are applied to the next period
    votes.iter().try_for_each(|(pool_addr, bps)| {
        let pool_votes_path = POOL_VOTES.key((U64Key::new(block_period + 1), &pool_addr));
        vote_for_pool(
            deps.branch(),
            pool_votes_path,
            *bps,
            user_vp,
            lock_info.slope,
        )
    })?;

    let user_info = UserInfo {
        vote_ts: env.block.time.seconds(),
        voting_power: user_vp,
        slope: lock_info.slope,
        votes,
    };

    USER_INFO.save(deps.storage, &user, &user_info)?;

    Ok(Response::new().add_attribute("action", "vote"))
}

fn gauge_generators(deps: DepsMut, env: Env, info: MessageInfo) -> ExecuteResult {
    let gauge_info = GAUGE_INFO.may_load(deps.storage)?.unwrap_or_default();
    let config = CONFIG.load(deps.storage)?;
    let block_period = get_period(env.block.time.seconds());

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if env.block.time.seconds() - gauge_info.gauge_ts < GAUGE_COOLDOWN {
        return Err(ContractError::CooldownError(GAUGE_COOLDOWN / DAY));
    }

    let mut response = Response::new();

    // Cancel previous alloc points
    for (_pool_addr, _cur_alloc_points) in gauge_info.pool_alloc_points.iter() {
        // TODO: push msg to response.messages to cancel previous pool alloc points
    }

    // Recalculate voted pool info for passed periods including current block period
    for recalc_period in (get_period(gauge_info.gauge_ts) - 1)..=block_period {
        POOL_VOTES
            .prefix(U64Key::new(recalc_period))
            .range(deps.storage, None, None, Order::Ascending)
            .map(|pair_result| {
                let (pool_addr, old_pool_info) = deserialize_pair(deps.as_ref(), pair_result)?;
                let old_vp = calc_voting_power(
                    old_pool_info.slope,
                    old_pool_info.vxastro_amount,
                    recalc_period - 1,
                    recalc_period,
                );
                let new_pool_info_opt =
                    POOL_VOTES.may_load(deps.storage, (U64Key::new(recalc_period), &pool_addr))?;
                let mut new_pool_info = new_pool_info_opt.unwrap_or_default();
                new_pool_info.vxastro_amount = new_pool_info.vxastro_amount.checked_add(old_vp)?;
                new_pool_info.slope = new_pool_info.slope.checked_add(old_pool_info.slope)?;
                Ok((pool_addr, new_pool_info))
            })
            .collect::<StdResult<Vec<_>>>()?
            .iter()
            .try_for_each(|(pool_addr, pool_info)| {
                POOL_VOTES.save(
                    deps.storage,
                    (U64Key::new(recalc_period), pool_addr),
                    pool_info,
                )
            })?
    }

    let mut pool_votes = POOL_VOTES
        .prefix(U64Key::new(block_period))
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?;

    pool_votes.sort_by(|(_, a), (_, b)| a.vxastro_amount.cmp(&b.vxastro_amount));
    let winners: Vec<_> = pool_votes
        .into_iter()
        .rev()
        .take(config.pools_limit as usize)
        .collect();

    let total_vp = winners
        .iter()
        .fold(Uint128::zero(), |acc, (_, vp)| acc + vp.vxastro_amount);

    let mut pool_alloc_points = vec![];
    for (pool_addr_serialized, pool_info) in winners {
        let alloc_points: u16 = BasicPoints::from_ratio(pool_info.vxastro_amount, total_vp)?.into();
        let alloc_points = Uint64::from(alloc_points);
        let pool_addr = String::from_utf8(pool_addr_serialized)
            .map_err(|_| StdError::generic_err("Deserialization error"))
            .and_then(|pool_addr_string| addr_validate_to_lower(deps.api, &pool_addr_string))?;
        pool_alloc_points.push((pool_addr.clone(), alloc_points));
        // TODO: push msg to response.messages to set pool alloc points
    }

    GAUGE_INFO.save(
        deps.storage,
        &GaugeInfo {
            gauge_ts: env.block.time.seconds(),
            pool_alloc_points,
        },
    )?;

    Ok(response.add_attribute("action", "gauge_generators"))
}

/// # Description
/// Describes all query messages.
/// ## Queries
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::UserInfo { user } => to_binary(&user_info(deps, user)?),
        QueryMsg::GaugeInfo => to_binary(&GAUGE_INFO.load(deps.storage)?),
        QueryMsg::Config => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::PoolInfo { pool_addr } => to_binary(&pool_info(deps, env, pool_addr, None)?),
        QueryMsg::PoolInfoAtPeriod { pool_addr, period } => {
            to_binary(&pool_info(deps, env, pool_addr, Some(period))?)
        }
    }
}

fn user_info(deps: Deps, user: String) -> StdResult<UserInfo> {
    let user_addr = addr_validate_to_lower(deps.api, &user)?;
    USER_INFO
        .may_load(deps.storage, &user_addr)?
        .ok_or_else(|| StdError::generic_err("User not found"))
}

fn pool_info(
    deps: Deps,
    env: Env,
    pool_addr: String,
    period: Option<u64>,
) -> StdResult<VotedPoolInfo> {
    let pool_addr = addr_validate_to_lower(deps.api, &pool_addr)?;
    let period = period.unwrap_or_else(|| get_period(env.block.time.seconds()));
    let pool_info = POOL_VOTES
        .may_load(deps.storage, (U64Key::new(period), &pool_addr))?
        .unwrap_or_default();
    Ok(pool_info)
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
