use crate::bps::BasicPoints;

use crate::state::{VotedPoolInfo, LAST_POOL_PERIOD, POOL_VOTES};

use astroport_governance::voting_escrow::QueryMsg::LockInfo;
use astroport_governance::voting_escrow::{
    LockInfoResponse, QueryMsg::UserVotingPower, VotingPowerResponse,
};
use cosmwasm_std::{Addr, Decimal, Deps, DepsMut, QuerierWrapper, StdResult, Uint128};
use cw_storage_plus::U64Key;

use astroport_governance::utils::calc_voting_power_by_dt;

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
            Operation::Sub => cur_vp - bps * vp,
        }
    }
}

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

pub(crate) fn cancel_user_changes(
    mut deps: DepsMut,
    block_period: u64,
    pool_addr: &Addr,
    old_bps: BasicPoints,
    old_vp: Uint128,
    old_slope: Decimal,
) -> StdResult<()> {
    update_pool_info(
        deps.branch(),
        block_period,
        pool_addr,
        Some((old_bps, old_vp, old_slope, Operation::Sub)),
    )
    .map(|_| ())
}

pub(crate) fn vote_for_pool(
    mut deps: DepsMut,
    period: u64,
    pool_addr: &Addr,
    bps: BasicPoints,
    vp: Uint128,
    slope: Decimal,
) -> StdResult<()> {
    update_pool_info(
        deps.branch(),
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
    let pool_info = match get_pool_info(deps.as_ref(), period, pool_addr)? {
        VotedPoolInfoResult::Unchanged(pool_info) => pool_info,
        VotedPoolInfoResult::New(mut pool_info) => {
            if let Some((bps, vp, slope, op)) = changes {
                pool_info.slope = op.calc_slope(pool_info.slope, slope, bps);
                pool_info.vxastro_amount = op.calc_voting_power(pool_info.vxastro_amount, vp, bps);
            }
            LAST_POOL_PERIOD.save(deps.storage, pool_addr, &period)?;
            POOL_VOTES.save(deps.storage, (U64Key::new(period), pool_addr), &pool_info)?;
            pool_info
        }
    };

    Ok(pool_info)
}

/// Returns pool info at the period or tries to calculate it.
pub(crate) fn get_pool_info(
    deps: Deps,
    period: u64,
    pool_addr: &Addr,
) -> StdResult<VotedPoolInfoResult> {
    let pool_info_opt = if let Some(pool_info) =
        POOL_VOTES.may_load(deps.storage, (U64Key::new(period), pool_addr))?
    {
        VotedPoolInfoResult::Unchanged(pool_info)
    } else if let Some(last_period) = LAST_POOL_PERIOD.may_load(deps.storage, pool_addr)? {
        let pool_info = if last_period < period {
            let prev_pool_info =
                POOL_VOTES.load(deps.storage, (U64Key::new(last_period), pool_addr))?;
            VotedPoolInfo {
                vxastro_amount: calc_voting_power_by_dt(
                    prev_pool_info.slope,
                    prev_pool_info.vxastro_amount,
                    period - last_period,
                ),
                ..prev_pool_info
            }
        } else {
            VotedPoolInfo::default()
        };
        VotedPoolInfoResult::New(pool_info)
    } else {
        VotedPoolInfoResult::New(VotedPoolInfo::default())
    };
    Ok(pool_info_opt)
}
