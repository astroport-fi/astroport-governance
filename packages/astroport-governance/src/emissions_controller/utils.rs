use crate::emissions_controller::consts::{
    EPOCHS_START, EPOCH_LENGTH, WHITELIST_VALIDATION_MAX_ROUTE_LENGTH,
};
use crate::emissions_controller::hub::RouteStep;
use crate::voting_escrow;
use astroport::asset::{pair_info_by_pool, AssetInfo, AssetInfoExt, PairInfo};
use astroport::common::LP_SUBDENOM;
use astroport::pair::SimulationResponse;
use astroport::{factory, pair};
use cosmwasm_std::{ensure, Addr, Decimal, Deps, QuerierWrapper, StdError, StdResult, Uint128};
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

/// Validates if the given pool with validation swap route is eligible for whitelist.
/// - liq_percent - percentage of whitelisted pool's liquidity
///   to be used as the offer amount in the first step.
/// - allowed_spread_per_step - maximum allowed spread per step.
/// - pair_info - the pair info of the pool to be whitelisted.
/// - route - the swap route to validate.
///   Must start with one of the pool's assets and lead to ASTRO.
///   The route length must be between 0 and WHITELIST_VALIDATION_MAX_ROUTE_LENGTH.
pub fn validate_whitelist_eligibility(
    querier: QuerierWrapper,
    factory: &Addr,
    liq_percent: Decimal,
    allowed_spread_per_step: Decimal,
    pair_info: &PairInfo,
    route: &[RouteStep],
) -> StdResult<()> {
    let route_len = route.len();
    ensure!(
        route_len > 0 && route_len <= WHITELIST_VALIDATION_MAX_ROUTE_LENGTH,
        StdError::generic_err(format!(
            "Route length must be between 0 and {WHITELIST_VALIDATION_MAX_ROUTE_LENGTH}, got {route_len}",
        ))
    );

    ensure!(
        route[0].offer_asset_info == pair_info.asset_infos[0]
            || route[0].offer_asset_info == pair_info.asset_infos[1],
        StdError::generic_err("The first step of the route must offer one of the pool's assets")
    );

    let mut step_amount = liq_percent
        * route[0]
            .offer_asset_info
            .query_pool(&querier, &pair_info.contract_addr)?;

    for step in route {
        let step_pair_info: PairInfo =
            querier.query_wasm_smart(&step.pair_address, &pair::QueryMsg::Pair {})?;

        // Verify that the pair is registered in the factory
        let step_pair_info_factory: PairInfo = querier.query_wasm_smart(
            factory,
            &factory::QueryMsg::PairByLpToken {
                lp_token: step_pair_info.liquidity_token,
            },
        )?;

        ensure!(
            step.pair_address == step_pair_info_factory.contract_addr,
            StdError::generic_err(format!(
                "Step pair address {} does not match the one in the factory {}",
                step.pair_address, step_pair_info.contract_addr
            ))
        );

        let res: SimulationResponse = querier.query_wasm_smart(
            &step.pair_address,
            &pair::QueryMsg::Simulation {
                offer_asset: step.offer_asset_info.with_balance(step_amount),
                ask_asset_info: Some(step.ask_asset_info.clone()),
            },
        )?;

        let spread = Decimal::from_ratio(res.spread_amount, res.return_amount);
        ensure!(
            spread <= allowed_spread_per_step,
            StdError::generic_err(format!("Spread {spread} is too high for step with pair {}. Max allowed is {allowed_spread_per_step}", step.pair_address))
        );

        step_amount = res.return_amount;
    }

    Ok(())
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
