use crate::bps::BasicPoints;

use crate::state::{VotedPoolInfo, CONFIG};
use astroport::asset::addr_validate_to_lower;

use astroport_governance::voting_escrow::QueryMsg::LockInfo;
use astroport_governance::voting_escrow::{
    LockInfoResponse, QueryMsg::UserVotingPower, VotingPowerResponse,
};
use cosmwasm_std::{Addr, Decimal, Deps, DepsMut, Fraction, Pair, StdError, StdResult, Uint128};
use cw_storage_plus::Path;

use std::str;

pub(crate) fn get_voting_power(deps: Deps, user: &Addr) -> StdResult<Uint128> {
    let voting_addr = CONFIG.load(deps.storage)?.escrow_addr;
    let vp: VotingPowerResponse = deps.querier.query_wasm_smart(
        voting_addr,
        &UserVotingPower {
            user: user.to_string(),
        },
    )?;
    Ok(vp.voting_power)
}

pub(crate) fn get_lock_end(deps: Deps, user: &Addr) -> StdResult<u64> {
    let voting_addr = CONFIG.load(deps.storage)?.escrow_addr;
    let lock_info: LockInfoResponse = deps.querier.query_wasm_smart(
        voting_addr,
        &LockInfo {
            user: user.to_string(),
        },
    )?;
    Ok(lock_info.end)
}

pub(crate) fn cancel_user_changes(
    deps: DepsMut,
    pool_votes_path: Path<VotedPoolInfo>,
    old_bps: BasicPoints,
    old_slope: Decimal,
    old_vp: Uint128,
) -> StdResult<()> {
    pool_votes_path
        .update(deps.storage, |pool_opt| {
            // pool_opt should never become None in this context
            let mut pool_info =
                pool_opt.ok_or_else(|| StdError::generic_err("Pool info was not found"))?;
            pool_info.vxastro_amount -= old_bps * old_vp;
            pool_info.slope = pool_info.slope
                - Decimal::from_ratio(old_slope * old_bps.into(), pool_info.slope.denominator());
            Ok(pool_info)
        })
        .map(|_| ())
}

pub(crate) fn vote_for_pool(
    deps: DepsMut,
    pool_votes_path: Path<VotedPoolInfo>,
    bps: BasicPoints,
    vp: Uint128,
    slope: Decimal,
) -> StdResult<()> {
    pool_votes_path
        .update(deps.storage, |pool_opt| {
            let mut pool_info = pool_opt.unwrap_or_default();
            pool_info.vxastro_amount += bps * vp;
            pool_info.slope = pool_info.slope
                + Decimal::from_ratio(slope * bps.into(), pool_info.slope.denominator());
            Ok(pool_info)
        })
        .map(|_| ())
}

/// # Description
/// Helper function for deserialization
pub(crate) fn deserialize_pair(
    deps: Deps,
    pair: StdResult<Pair<VotedPoolInfo>>,
) -> StdResult<(Addr, VotedPoolInfo)> {
    let (addr_serialized, pool_info) = pair?;
    let addr_str = str::from_utf8(&addr_serialized)
        .map_err(|_| StdError::generic_err("Deserialization error"))?;
    let addr = addr_validate_to_lower(deps.api, addr_str)?;
    Ok((addr, pool_info))
}
