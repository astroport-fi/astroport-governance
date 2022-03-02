use crate::bps::BasicPoints;
use std::convert::TryInto;

use crate::state::VotedPoolInfo;
use astroport::asset::addr_validate_to_lower;

use astroport_governance::voting_escrow::QueryMsg::LockInfo;
use astroport_governance::voting_escrow::{
    LockInfoResponse, QueryMsg::UserVotingPower, VotingPowerResponse,
};
use cosmwasm_std::{
    Addr, Decimal, Deps, DepsMut, Fraction, Pair, QuerierWrapper, StdError, StdResult, Uint128,
    Uint256,
};
use cw_storage_plus::Path;

use std::str;

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
    deps: DepsMut,
    pool_votes_path: Path<VotedPoolInfo>,
    old_bps: BasicPoints,
    old_slope: Decimal,
    old_vp: Uint128,
) -> StdResult<()> {
    pool_votes_path
        .update(deps.storage, |pool_opt| {
            // TODO: recalculate current period pool info basing on pervious period
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

pub(crate) trait CheckedMulRatio {
    fn checked_multiply_ratio(
        self,
        numerator: impl Into<u128>,
        denominator: impl Into<Uint256>,
    ) -> StdResult<Uint128>;
}

impl CheckedMulRatio for Uint128 {
    fn checked_multiply_ratio(
        self,
        numerator: impl Into<u128>,
        denominator: impl Into<Uint256>,
    ) -> StdResult<Uint128> {
        let numerator = self.full_mul(numerator);
        let denominator = denominator.into();
        let mut result = numerator / denominator;
        let rem = numerator
            .checked_rem(denominator)
            .map_err(|_| StdError::generic_err("Division by zero"))?;
        // Rounding up if residual is more than 50% of denominator
        if rem.ge(&(denominator / Uint256::from(2u8))) {
            result += Uint256::from(1u128);
        }
        result
            .try_into()
            .map_err(|_| StdError::generic_err("Uint256 -> Uint128 conversion error"))
    }
}
