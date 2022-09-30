use std::ops::RangeInclusive;

use crate::astroport;
use astroport::asset::{pair_info_by_pool, AssetInfo};
use astroport::factory::PairType;
use astroport::querier::query_pair_info;
use cosmwasm_std::{Addr, Deps, Order, StdError, StdResult, Storage, Uint128};
use cw_storage_plus::Bound;

use astroport_governance::utils::calc_voting_power;

use crate::bps::BasicPoints;
use crate::error::ContractError;
use crate::state::{VotedPoolInfo, POOLS, POOL_PERIODS, POOL_SLOPE_CHANGES, POOL_VOTES};

/// Pools limit should be within the range `[2, 100]`
const POOL_NUMBER_LIMIT: RangeInclusive<u64> = 2..=100;

/// ## Description
/// The enum defines math operations with voting power and slope.
#[derive(Debug)]
pub(crate) enum Operation {
    Add,
    Sub,
}

impl Operation {
    pub fn calc_slope(&self, cur_slope: Uint128, slope: Uint128, bps: BasicPoints) -> Uint128 {
        match self {
            Operation::Add => cur_slope + bps * slope,
            Operation::Sub => cur_slope - bps * slope,
        }
    }

    pub fn calc_voting_power(&self, cur_vp: Uint128, vp: Uint128, bps: BasicPoints) -> Uint128 {
        match self {
            Operation::Add => cur_vp + bps * vp,
            Operation::Sub => cur_vp.saturating_sub(bps * vp),
        }
    }
}

/// ## Description
/// Enum wraps [`VotedPoolInfo`] so the contract can leverage storage operations efficiently.
#[derive(Debug)]
pub(crate) enum VotedPoolInfoResult {
    Unchanged(VotedPoolInfo),
    New(VotedPoolInfo),
}

/// ## Description
/// Filters pairs (LP token address, voting parameters) by criteria:
/// * pool's pair is registered in Factory,
/// * pool's pair type is not in blocked list,
/// * any of pair's token is not listed in blocked tokens list.
pub(crate) fn filter_pools(
    deps: Deps,
    generator_addr: &Addr,
    factory_addr: &Addr,
    pools: Vec<(Addr, Uint128)>,
    pools_limit: u64,
) -> StdResult<Vec<(String, Uint128)>> {
    let blocked_tokens: Vec<AssetInfo> = deps.querier.query_wasm_smart(
        generator_addr.clone(),
        &astroport::generator::QueryMsg::BlockedTokensList {},
    )?;
    let blocklisted_pair_types: Vec<PairType> = deps.querier.query_wasm_smart(
        factory_addr.clone(),
        &astroport::factory::QueryMsg::BlacklistedPairTypes {},
    )?;

    let pools = pools
        .into_iter()
        .filter_map(|(pool_addr, vxastro_amount)| {
            // Check the address is a LP token and retrieve a pair info
            let pair_info = pair_info_by_pool(deps, pool_addr).ok()?;
            // Check a pair is registered in factory
            query_pair_info(&deps.querier, factory_addr.clone(), &pair_info.asset_infos).ok()?;
            let condition = !blocklisted_pair_types.contains(&pair_info.pair_type)
                && !blocked_tokens.contains(&pair_info.asset_infos[0])
                && !blocked_tokens.contains(&pair_info.asset_infos[1]);
            if condition {
                Some((pair_info.liquidity_token.to_string(), vxastro_amount))
            } else {
                None
            }
        })
        .take(pools_limit as usize)
        .collect();

    Ok(pools)
}

/// ## Description
/// Cancels user changes using old voting parameters for a given pool.  
/// Firstly, it removes slope change scheduled for previous lockup end period.  
/// Secondly, it updates voting parameters for the given period, but without user's vote.
pub(crate) fn cancel_user_changes(
    storage: &mut dyn Storage,
    period: u64,
    pool_addr: &Addr,
    old_bps: BasicPoints,
    old_vp: Uint128,
    old_slope: Uint128,
    old_lock_end: u64,
) -> StdResult<()> {
    // Cancel scheduled slope changes
    let last_pool_period = fetch_last_pool_period(storage, period, pool_addr)?.unwrap_or(period);
    if last_pool_period < old_lock_end + 1 {
        let end_period_key = old_lock_end + 1;
        let old_scheduled_change = POOL_SLOPE_CHANGES.load(storage, (pool_addr, end_period_key))?;
        let new_slope = old_scheduled_change - old_bps * old_slope;
        if !new_slope.is_zero() {
            POOL_SLOPE_CHANGES.save(storage, (pool_addr, end_period_key), &new_slope)?
        } else {
            POOL_SLOPE_CHANGES.remove(storage, (pool_addr, end_period_key))
        }
    }

    update_pool_info(
        storage,
        period,
        pool_addr,
        Some((old_bps, old_vp, old_slope, Operation::Sub)),
    )
    .map(|_| ())
}

/// ## Description
/// Applies user's vote for a given pool.   
/// Firstly, it schedules slope change for lockup end period.  
/// Secondly, it updates voting parameters with applied user's vote.
pub(crate) fn vote_for_pool(
    storage: &mut dyn Storage,
    period: u64,
    pool_addr: &Addr,
    bps: BasicPoints,
    vp: Uint128,
    slope: Uint128,
    lock_end: u64,
) -> StdResult<()> {
    // Schedule slope changes
    POOL_SLOPE_CHANGES.update::<_, StdError>(storage, (pool_addr, lock_end + 1), |slope_opt| {
        if let Some(saved_slope) = slope_opt {
            Ok(saved_slope + bps * slope)
        } else {
            Ok(bps * slope)
        }
    })?;
    update_pool_info(
        storage,
        period,
        pool_addr,
        Some((bps, vp, slope, Operation::Add)),
    )
    .map(|_| ())
}

/// ## Description
/// Fetches voting parameters for a given pool at specific period, applies new changes, saves it in storage
/// and returns new voting parameters in [`VotedPoolInfo`] object.
/// If there are no changes in 'changes' parameter
/// and voting parameters were already calculated before the function just returns [`VotedPoolInfo`].
pub(crate) fn update_pool_info(
    storage: &mut dyn Storage,
    period: u64,
    pool_addr: &Addr,
    changes: Option<(BasicPoints, Uint128, Uint128, Operation)>,
) -> StdResult<VotedPoolInfo> {
    if POOLS.may_load(storage, pool_addr)?.is_none() {
        POOLS.save(storage, pool_addr, &())?
    }
    let period_key = period;
    let pool_info = match get_pool_info_mut(storage, period, pool_addr)? {
        VotedPoolInfoResult::Unchanged(mut pool_info) | VotedPoolInfoResult::New(mut pool_info)
            if changes.is_some() =>
        {
            if let Some((bps, vp, slope, op)) = changes {
                pool_info.slope = op.calc_slope(pool_info.slope, slope, bps);
                pool_info.vxastro_amount = op.calc_voting_power(pool_info.vxastro_amount, vp, bps);
            }
            POOL_PERIODS.save(storage, (pool_addr, period_key), &())?;
            POOL_VOTES.save(storage, (period_key, pool_addr), &pool_info)?;
            pool_info
        }
        VotedPoolInfoResult::New(pool_info) => {
            POOL_PERIODS.save(storage, (pool_addr, period_key), &())?;
            POOL_VOTES.save(storage, (period_key, pool_addr), &pool_info)?;
            pool_info
        }
        VotedPoolInfoResult::Unchanged(pool_info) => pool_info,
    };

    Ok(pool_info)
}

/// ## Description
/// Returns pool info at specified period or calculates it. Saves intermediate results in storage.
pub(crate) fn get_pool_info_mut(
    storage: &mut dyn Storage,
    period: u64,
    pool_addr: &Addr,
) -> StdResult<VotedPoolInfoResult> {
    let pool_info_result = if let Some(pool_info) =
        POOL_VOTES.may_load(storage, (period, pool_addr))?
    {
        VotedPoolInfoResult::Unchanged(pool_info)
    } else {
        let pool_info_result =
            if let Some(mut prev_period) = fetch_last_pool_period(storage, period, pool_addr)? {
                let mut pool_info = POOL_VOTES.load(storage, (prev_period, pool_addr))?;
                // Recalculating passed periods
                let scheduled_slope_changes =
                    fetch_slope_changes(storage, pool_addr, prev_period, period)?;
                for (recalc_period, scheduled_change) in scheduled_slope_changes {
                    pool_info = VotedPoolInfo {
                        vxastro_amount: calc_voting_power(
                            pool_info.slope,
                            pool_info.vxastro_amount,
                            prev_period,
                            recalc_period,
                        ),
                        slope: pool_info.slope - scheduled_change,
                    };
                    // Save intermediate result
                    let recalc_period_key = recalc_period;
                    POOL_PERIODS.save(storage, (pool_addr, recalc_period_key), &())?;
                    POOL_VOTES.save(storage, (recalc_period_key, pool_addr), &pool_info)?;
                    prev_period = recalc_period
                }

                VotedPoolInfo {
                    vxastro_amount: calc_voting_power(
                        pool_info.slope,
                        pool_info.vxastro_amount,
                        prev_period,
                        period,
                    ),
                    ..pool_info
                }
            } else {
                VotedPoolInfo::default()
            };

        VotedPoolInfoResult::New(pool_info_result)
    };

    Ok(pool_info_result)
}

/// ## Description
/// Returns pool info at specified period or calculates it.
pub(crate) fn get_pool_info(
    storage: &dyn Storage,
    period: u64,
    pool_addr: &Addr,
) -> StdResult<VotedPoolInfo> {
    let pool_info = if let Some(pool_info) = POOL_VOTES.may_load(storage, (period, pool_addr))? {
        pool_info
    } else if let Some(mut prev_period) = fetch_last_pool_period(storage, period, pool_addr)? {
        let mut pool_info = POOL_VOTES.load(storage, (prev_period, pool_addr))?;
        // Recalculating passed periods
        let scheduled_slope_changes = fetch_slope_changes(storage, pool_addr, prev_period, period)?;
        for (recalc_period, scheduled_change) in scheduled_slope_changes {
            pool_info = VotedPoolInfo {
                vxastro_amount: calc_voting_power(
                    pool_info.slope,
                    pool_info.vxastro_amount,
                    prev_period,
                    recalc_period,
                ),
                slope: pool_info.slope - scheduled_change,
            };
            prev_period = recalc_period
        }

        VotedPoolInfo {
            vxastro_amount: calc_voting_power(
                pool_info.slope,
                pool_info.vxastro_amount,
                prev_period,
                period,
            ),
            ..pool_info
        }
    } else {
        VotedPoolInfo::default()
    };

    Ok(pool_info)
}

/// ## Description
/// Fetches last period for specified pool which has saved result in [`POOL_PERIODS`].
pub(crate) fn fetch_last_pool_period(
    storage: &dyn Storage,
    period: u64,
    pool_addr: &Addr,
) -> StdResult<Option<u64>> {
    let period_opt = POOL_PERIODS
        .prefix(pool_addr)
        .range(
            storage,
            None,
            Some(Bound::exclusive(period)),
            Order::Descending,
        )
        .next()
        .transpose()?
        .map(|(period, _)| period);
    Ok(period_opt)
}

/// ## Description
/// Fetches all slope changes between `last_period` and `period` for specific pool.
pub(crate) fn fetch_slope_changes(
    storage: &dyn Storage,
    pool_addr: &Addr,
    last_period: u64,
    period: u64,
) -> StdResult<Vec<(u64, Uint128)>> {
    POOL_SLOPE_CHANGES
        .prefix(pool_addr)
        .range(
            storage,
            Some(Bound::exclusive(last_period)),
            Some(Bound::inclusive(period)),
            Order::Ascending,
        )
        .collect()
}

pub(crate) fn validate_pools_limit(number: u64) -> Result<u64, ContractError> {
    if !POOL_NUMBER_LIMIT.contains(&number) {
        Err(ContractError::InvalidPoolNumber(number))
    } else {
        Ok(number)
    }
}
