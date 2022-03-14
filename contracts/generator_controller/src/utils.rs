use crate::bps::BasicPoints;
use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::PairsResponse;
use std::convert::TryInto;

use crate::state::{VotedPoolInfo, POOLS, POOL_PERIODS, POOL_SLOPE_CHANGES, POOL_VOTES};

use astroport_governance::voting_escrow::QueryMsg::LockInfo;
use astroport_governance::voting_escrow::{
    LockInfoResponse, QueryMsg::UserVotingPower, VotingPowerResponse,
};
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Deps, DepsMut, Order, Pair, QuerierWrapper, StdError,
    StdResult, Uint128, Uint64, WasmMsg,
};
use cw_storage_plus::{Bound, U64Key};

use astroport_governance::utils::calc_voting_power;

#[derive(Debug)]
pub(crate) enum Operation {
    Add,
    Sub,
}

impl Operation {
    pub fn calc_slope(&self, cur_slope: Decimal, slope: Decimal, bps: BasicPoints) -> Decimal {
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

#[derive(Debug)]
pub(crate) enum VotedPoolInfoResult {
    Unchanged(VotedPoolInfo),
    New(VotedPoolInfo),
}

impl VotedPoolInfoResult {
    pub fn get(self) -> VotedPoolInfo {
        match self {
            VotedPoolInfoResult::Unchanged(pool_info) | VotedPoolInfoResult::New(pool_info) => {
                pool_info
            }
        }
    }
}

pub(crate) fn get_voting_power(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &UserVotingPower {
            user: user.to_string(),
        },
    )?;
    Ok(vp.voting_power)
}

pub(crate) fn get_lock_info(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
) -> StdResult<LockInfoResponse> {
    let lock_info: LockInfoResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &LockInfo {
            user: user.to_string(),
        },
    )?;
    Ok(lock_info)
}

pub(crate) fn filter_pools(
    deps: Deps,
    generator_addr: &Addr,
    factory_addr: &Addr,
    pools: Vec<(Addr, VotedPoolInfo)>,
) -> StdResult<Vec<(Addr, VotedPoolInfo)>> {
    let registered_pairs: PairsResponse = deps.querier.query_wasm_smart(
        factory_addr.clone(),
        &astroport::factory::QueryMsg::Pairs {
            start_after: None,
            limit: None,
        },
    )?;
    let blocked_tokens: Vec<AssetInfo> = deps.querier.query_wasm_smart(
        generator_addr.clone(),
        &astroport::generator::QueryMsg::BlockedListTokens {},
    )?;
    // TODO: add blocklisted pair types query
    let blocklisted_pair_types: Vec<_> = vec![];

    let pools = pools
        .into_iter()
        .filter_map(|(pair_addr, pool_info)| {
            // Both xyk and stable pair types have the same query and response formats.
            // However, new pair types have to inherit same formats. Otherwise we will get an error here
            let pair_info: PairInfo = deps
                .querier
                .query_wasm_smart(pair_addr.clone(), &astroport::pair::QueryMsg::Pair {})
                .ok()?;

            let condition = registered_pairs.pairs.contains(&pair_info)
                && !blocklisted_pair_types.contains(&pair_info.pair_type)
                && !blocked_tokens.contains(&pair_info.asset_infos[0])
                && !blocked_tokens.contains(&pair_info.asset_infos[1]);
            if condition {
                Some((pair_addr, pool_info))
            } else {
                None
            }
        })
        .collect();

    Ok(pools)
}

pub(crate) fn cancel_user_changes(
    deps: DepsMut,
    block_period: u64,
    pool_addr: &Addr,
    old_bps: BasicPoints,
    old_vp: Uint128,
    old_slope: Decimal,
    old_lock_end: u64,
) -> StdResult<()> {
    // Cancel scheduled slope changes
    let end_period_key = U64Key::new(old_lock_end + 1);
    let last_pool_period =
        fetch_last_pool_period(deps.as_ref(), block_period, pool_addr)?.unwrap_or(block_period);
    if last_pool_period < old_lock_end + 1 {
        let old_scheduled_change =
            POOL_SLOPE_CHANGES.load(deps.as_ref().storage, (pool_addr, end_period_key.clone()))?;
        let new_slope = old_scheduled_change - old_bps * old_slope;
        if !new_slope.is_zero() {
            POOL_SLOPE_CHANGES.save(deps.storage, (pool_addr, end_period_key), &new_slope)?
        } else {
            POOL_SLOPE_CHANGES.remove(deps.storage, (pool_addr, end_period_key))
        }
    }

    update_pool_info(
        deps,
        block_period,
        pool_addr,
        Some((old_bps, old_vp, old_slope, Operation::Sub)),
    )
    .map(|_| ())
}

pub(crate) fn vote_for_pool(
    deps: DepsMut,
    period: u64,
    pool_addr: &Addr,
    bps: BasicPoints,
    vp: Uint128,
    slope: Decimal,
    lock_end: u64,
) -> StdResult<()> {
    // Schedule slope changes
    POOL_SLOPE_CHANGES.update::<_, StdError>(
        deps.storage,
        (pool_addr, U64Key::new(lock_end + 1)),
        |slope_opt| {
            if let Some(saved_slope) = slope_opt {
                Ok(saved_slope + bps * slope)
            } else {
                Ok(bps * slope)
            }
        },
    )?;
    update_pool_info(
        deps,
        period,
        pool_addr,
        Some((bps, vp, slope, Operation::Add)),
    )
    .map(|_| ())
}

pub(crate) fn update_pool_info(
    deps: DepsMut,
    period: u64,
    pool_addr: &Addr,
    changes: Option<(BasicPoints, Uint128, Decimal, Operation)>,
) -> StdResult<VotedPoolInfo> {
    let period_key = U64Key::new(period);
    let pool_info = match get_pool_info(deps.as_ref(), period, pool_addr)? {
        VotedPoolInfoResult::Unchanged(mut pool_info) | VotedPoolInfoResult::New(mut pool_info)
            if changes.is_some() =>
        {
            if let Some((bps, vp, slope, op)) = changes {
                pool_info.slope = op.calc_slope(pool_info.slope, slope, bps);
                pool_info.vxastro_amount = op.calc_voting_power(pool_info.vxastro_amount, vp, bps);
            }
            if POOLS.may_load(deps.storage, pool_addr)?.is_none() {
                POOLS.save(deps.storage, pool_addr, &())?
            }
            POOL_PERIODS.save(deps.storage, (pool_addr, period_key.clone()), &())?;
            POOL_VOTES.save(deps.storage, (period_key, pool_addr), &pool_info)?;
            pool_info
        }
        VotedPoolInfoResult::New(pool_info) => {
            if POOLS.may_load(deps.storage, pool_addr)?.is_none() {
                POOLS.save(deps.storage, pool_addr, &())?
            }
            POOL_PERIODS.save(deps.storage, (pool_addr, period_key.clone()), &())?;
            POOL_VOTES.save(deps.storage, (period_key, pool_addr), &pool_info)?;
            pool_info
        }
        VotedPoolInfoResult::Unchanged(pool_info) => pool_info,
    };

    Ok(pool_info)
}

/// Returns pool info at the period or tries to calculate it.
pub(crate) fn get_pool_info(
    deps: Deps,
    period: u64,
    pool_addr: &Addr,
) -> StdResult<VotedPoolInfoResult> {
    let pool_info_result = if let Some(pool_info) =
        POOL_VOTES.may_load(deps.storage, (U64Key::new(period), pool_addr))?
    {
        VotedPoolInfoResult::Unchanged(pool_info)
    } else {
        let pool_info =
            if let Some(mut prev_period) = fetch_last_pool_period(deps, period, pool_addr)? {
                let mut pool_info =
                    POOL_VOTES.load(deps.storage, (U64Key::new(prev_period), pool_addr))?;
                // Recalculating passed periods
                let scheduled_slope_changes =
                    fetch_slope_changes(deps, pool_addr, prev_period, period)?;
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

        VotedPoolInfoResult::New(pool_info)
    };

    Ok(pool_info_result)
}

pub(crate) fn fetch_last_pool_period(
    deps: Deps,
    period: u64,
    pool_addr: &Addr,
) -> StdResult<Option<u64>> {
    let period_opt = POOL_PERIODS
        .prefix(pool_addr)
        .range(
            deps.storage,
            None,
            Some(Bound::Exclusive(U64Key::new(period).wrapped)),
            Order::Descending,
        )
        .next()
        .map(deserialize_pair)
        .transpose()?
        .map(|(period, _)| period);
    Ok(period_opt)
}

/// Helper function for deserialization.
pub(crate) fn deserialize_pair<T>(pair: StdResult<Pair<T>>) -> StdResult<(u64, T)> {
    let (period_serialized, change) = pair?;
    let period_bytes: [u8; 8] = period_serialized
        .try_into()
        .map_err(|_| StdError::generic_err("Deserialization error"))?;
    Ok((u64::from_be_bytes(period_bytes), change))
}

/// Fetches all slope changes between `last_period` and `period`.
pub(crate) fn fetch_slope_changes(
    deps: Deps,
    pool_addr: &Addr,
    last_period: u64,
    period: u64,
) -> StdResult<Vec<(u64, Decimal)>> {
    POOL_SLOPE_CHANGES
        .prefix(pool_addr)
        .range(
            deps.storage,
            Some(Bound::Exclusive(U64Key::new(last_period).wrapped)),
            Some(Bound::Inclusive(U64Key::new(period).wrapped)),
            Order::Ascending,
        )
        .map(deserialize_pair)
        .collect()
}

pub(crate) fn setup_pools_msg(
    generator_addr: &Addr,
    pool_apoints: Vec<(String, Uint64)>,
) -> StdResult<CosmosMsg> {
    let msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: generator_addr.to_string(),
        msg: to_binary(&astroport::generator::ExecuteMsg::SetupPools {
            pools: pool_apoints,
        })?,
        funds: vec![],
    });

    Ok(msg)
}
