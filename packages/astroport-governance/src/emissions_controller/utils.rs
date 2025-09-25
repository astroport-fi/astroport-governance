use crate::emissions_controller::consts::{EPOCHS_START, EPOCH_LENGTH};
use crate::voting_escrow;
use astroport::asset::{pair_info_by_pool, AssetInfo, PairInfo};
use astroport::common::LP_SUBDENOM;
use astroport::{factory, pair};
use cosmwasm_std::{Addr, Deps, QuerierWrapper, StdError, StdResult, Uint128};
use itertools::Itertools;

/// Queries pair info corresponding to given LP token.
/// Handles both native and cw20 tokens.
/// If the token is native, it must follow the following format:
/// factory/{lp_minter}/astroport/share
/// where lp_minter is a valid bech32 address on the current chain.
pub fn query_pair_info(deps: Deps, lp_asset: &AssetInfo) -> StdResult<PairInfo> {
    match lp_asset {
        AssetInfo::Token { contract_addr } => pair_info_by_pool(&deps.querier, contract_addr),
        AssetInfo::NativeToken { denom } => {
            let lp_minter = get_pair_from_denom(deps, denom)?;
            deps.querier
                .query_wasm_smart(lp_minter, &pair::QueryMsg::Pair {})
        }
    }
}

pub fn get_pair_from_denom(deps: Deps, denom: &str) -> StdResult<Addr> {
    let parts = denom.split('/').collect_vec();
    if denom.starts_with("factory") && denom.ends_with(LP_SUBDENOM) {
        let lp_minter = parts[1];
        deps.api.addr_validate(lp_minter)
    } else {
        Err(StdError::generic_err(format!(
            "LP token {denom} doesn't follow token factory format: factory/{{lp_minter}}/{{token_name}}",
        )))
    }
}

/// Checks if the pool with the following asset infos is registered in the factory contract and
/// LP tokens address/denom matches the one registered in the factory.
pub fn get_pair_info(deps: Deps, factory: &Addr, maybe_lp: &str) -> StdResult<PairInfo> {
    deps.querier.query_wasm_smart(
        factory,
        &factory::QueryMsg::PairByLpToken {
            lp_token: maybe_lp.to_string(),
        },
    )
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
pub fn get_total_voting_power(
    querier: QuerierWrapper,
    vxastro: &Addr,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    querier.query_wasm_smart(
        vxastro,
        &voting_escrow::QueryMsg::TotalVotingPower { timestamp },
    )
}

#[inline]
pub fn query_incentives_addr(querier: QuerierWrapper, factory: &Addr) -> StdResult<Addr> {
    querier
        .query_wasm_smart::<factory::ConfigResponse>(factory, &factory::QueryMsg::Config {})?
        .generator_address
        .ok_or_else(|| StdError::generic_err("Generator address is not set"))
}

/// Normalize current timestamp to the beginning of the current epoch (Monday).
pub fn get_epoch_start(timestamp: u64) -> u64 {
    let rem = timestamp % EPOCHS_START;
    if rem % EPOCH_LENGTH == 0 {
        // Hit at the beginning of the current epoch
        timestamp
    } else {
        // Hit somewhere in the middle
        EPOCHS_START + rem / EPOCH_LENGTH * EPOCH_LENGTH
    }
}
