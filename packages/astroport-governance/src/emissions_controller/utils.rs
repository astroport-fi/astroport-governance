use astroport::asset::{pair_info_by_pool, AssetInfo, PairInfo};
use astroport::{factory, pair};
use cosmwasm_std::{Addr, QuerierWrapper, StdError, StdResult, Uint128};

use crate::emissions_controller::consts::LP_SUBDENOM;
use crate::voting_escrow;

/// Queries pair info corresponding to given LP token.
/// Handles both native and cw20 tokens.
/// If the token is native, it must follow the following format:
/// factory/{lp_minter}/astroport/share
/// where lp_minter is a valid bech32 address on the current chain.
pub fn query_pair_info(querier: QuerierWrapper, lp_asset: &AssetInfo) -> StdResult<PairInfo> {
    match lp_asset {
        AssetInfo::Token { contract_addr } => pair_info_by_pool(&querier, contract_addr),
        AssetInfo::NativeToken { denom } => {
            if denom.starts_with("factory/") && denom.ends_with(LP_SUBDENOM) {
                let lp_minter = denom.split('/').nth(1).unwrap();
                querier.query_wasm_smart(lp_minter, &pair::QueryMsg::Pair {})
            } else {
                Err(StdError::generic_err(format!(
                    "LP token {denom} doesn't follow token factory format: factory/{{lp_minter}}{LP_SUBDENOM}",
                )))
            }
        }
    }
}

/// Checks if the given LP token is registered in the factory.
pub fn check_lp_token(
    querier: QuerierWrapper,
    factory: &Addr,
    maybe_lp: &AssetInfo,
) -> StdResult<()> {
    let pair_info = query_pair_info(querier, maybe_lp)?;
    querier
        .query_wasm_smart::<PairInfo>(
            factory,
            &factory::QueryMsg::Pair {
                asset_infos: pair_info.asset_infos.to_vec(),
            },
        )
        .map_err(|_| {
            StdError::generic_err(format!(
                "The pair is not registered: {}-{}",
                pair_info.asset_infos[0], pair_info.asset_infos[1]
            ))
        })
        .and_then(|resp| {
            if resp.liquidity_token.as_str() == maybe_lp.to_string() {
                Ok(())
            } else {
                Err(StdError::generic_err(format!(
                    "LP token {maybe_lp} doesn't match LP token registered in factory {}",
                    resp.liquidity_token
                )))
            }
        })
}

#[inline]
pub fn get_voting_power(
    querier: QuerierWrapper,
    vxastro: &Addr,
    voter: impl Into<String>,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    querier.query_wasm_smart(
        vxastro,
        &voting_escrow::QueryMsg::UserVotingPower {
            user: voter.into(),
            timestamp,
        },
    )
}

#[inline]
pub fn query_incentives_addr(querier: QuerierWrapper, factory: &Addr) -> StdResult<Addr> {
    querier
        .query_wasm_smart::<factory::ConfigResponse>(factory, &factory::QueryMsg::Config {})?
        .generator_address
        .ok_or_else(|| StdError::generic_err("Generator address is not set"))
}
