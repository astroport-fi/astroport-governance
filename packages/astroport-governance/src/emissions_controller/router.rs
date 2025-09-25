use astroport::asset::{determine_asset_info, Asset, AssetInfo, AssetInfoExt, PairInfo};
use astroport::pair::SimulationResponse;
use astroport::{factory, pair};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, ensure, ensure_eq, ensure_ne, Addr, Decimal, Deps, DepsMut, Order, QuerierWrapper,
    Response, StdError, StdResult, Storage,
};
use cw_storage_plus::{Bound, Item, Map};
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

pub const MAX_SWAPS_DEPTH: usize = 10;
pub const PAGINATION_LIMIT: u32 = 50;

#[derive(Error, Debug, PartialEq)]
pub enum RouterError {
    #[error("{0}")]
    Std(#[from] StdError),
    #[error("No registered route for {asset}")]
    RouteNotFound { asset: String },
    #[error("Failed to build route for {asset} with the max multi-hop depth {MAX_SWAPS_DEPTH}")]
    FailedToBuildRoute { asset: String },
    #[error("Empty routes")]
    EmptyRoutes {},
    #[error("Route cannot contain ASTRO as intermediate asset")]
    AstroInRoute {},
    #[error("Message contains duplicated routes for asset {asset}")]
    DuplicatedRoutes { asset: String },
}

#[cw_serde]
pub struct RouteStep {
    pub asset_out: AssetInfo,
    pub pool_addr: Addr,
}

#[cw_serde]
pub struct RouteStepVerbose {
    pub asset_in: AssetInfo,
    pub asset_out: AssetInfo,
    pub pool_addr: String,
}

/// Routes is a map of asset_in and asset_out to pool address.
/// Key: (asset_in) binary representing [`AssetInfo`] converted with [`asset_info_key`],
/// Value: RouteStep object {asset_out, pool_addr}
pub const ROUTES: Map<&[u8], RouteStep> = Map::new("routes");
/// Default assets which the router tries to swap into if there is no direct route to target asset.
/// Usually these are assets with high liquidity like USDC, NTRN, LUNA, etc.
/// Order matters - router will try to swap into the first asset first, then second, etc.
/// If there is a saved route to a target asset, default assets are not used.
pub const DEFAULT_ASSETS: Item<Vec<AssetInfo>> = Item::new("default_assets");

pub struct RoutesBuilder {
    routes_cache: HashMap<AssetInfo, RouteStep>,
    liq_percent: Decimal,
    allowed_spread_per_step: Decimal,
    default_assets: Vec<AssetInfo>,
    factory: Addr,
}

impl Default for RoutesBuilder {
    fn default() -> Self {
        Self {
            routes_cache: HashMap::new(),
            liq_percent: Decimal::zero(),
            allowed_spread_per_step: Decimal::zero(),
            default_assets: vec![],
            factory: Addr::unchecked(""),
        }
    }
}

impl RoutesBuilder {
    pub fn new(
        storage: &dyn Storage,
        factory: &Addr,
        liq_percent: Decimal,
        allowed_spread_per_step: Decimal,
    ) -> StdResult<Self> {
        Ok(Self {
            liq_percent,
            allowed_spread_per_step,
            default_assets: DEFAULT_ASSETS.may_load(storage)?.unwrap_or_default(),
            factory: factory.clone(),
            ..Default::default()
        })
    }

    pub fn build_route(
        &mut self,
        storage: &dyn Storage,
        asset_in: &AssetInfo,
        target_asset_info: &AssetInfo,
    ) -> Result<Vec<RouteStep>, RouterError> {
        let mut prev_asset = asset_in.clone();
        let mut routes = vec![];

        for _ in 0..MAX_SWAPS_DEPTH {
            if &prev_asset == target_asset_info {
                break;
            }

            let step = if let Some(found) = self.routes_cache.get(&prev_asset).cloned() {
                found
            } else {
                let step = ROUTES
                    .may_load(storage, &asset_info_key(&prev_asset))?
                    .ok_or(RouterError::RouteNotFound {
                        asset: prev_asset.to_string(),
                    })?;
                self.routes_cache.insert(prev_asset, step.clone());

                step
            };

            prev_asset = step.asset_out.clone();

            routes.push(step);
        }

        ensure_eq!(
            &prev_asset,
            target_asset_info,
            RouterError::FailedToBuildRoute {
                asset: asset_in.to_string(),
            }
        );

        Ok(routes)
    }

    pub fn validate_whitelisting_pool(
        &mut self,
        deps: Deps,
        target_asset_info: &AssetInfo,
        pair_info: &PairInfo,
    ) -> Result<Asset, RouterError> {
        let balances = pair_info.query_pools(&deps.querier, &pair_info.contract_addr)?;

        let asset_in = pair_info.asset_infos[0].with_balance(self.liq_percent * balances[0].amount);
        self.try_swap_into_target(deps, asset_in, target_asset_info, pair_info)
            .or_else(|_| {
                let asset_in =
                    pair_info.asset_infos[1].with_balance(self.liq_percent * balances[1].amount);
                self.try_swap_into_target(deps, asset_in, target_asset_info, pair_info)
            })
    }

    fn get_cached_or_read_route(
        &mut self,
        storage: &dyn Storage,
        asset_in: &AssetInfo,
    ) -> Result<Option<RouteStep>, RouterError> {
        if let Some(found) = self.routes_cache.get(asset_in).cloned() {
            Ok(Some(found))
        } else if let Some(step) = ROUTES.may_load(storage, &asset_info_key(asset_in))? {
            self.routes_cache.insert(asset_in.clone(), step.clone());
            Ok(Some(step))
        } else {
            Ok(None)
        }
    }

    pub fn try_swap_into_target(
        &mut self,
        deps: Deps,
        asset_in: Asset,
        target_asset_info: &AssetInfo,
        pair_info: &PairInfo,
    ) -> Result<Asset, RouterError> {
        let mut prev_asset = asset_in.clone();

        for i in 0..MAX_SWAPS_DEPTH {
            if prev_asset.info.eq(target_asset_info) {
                break;
            }

            prev_asset = if let Some(step) =
                self.get_cached_or_read_route(deps.storage, &prev_asset.info)?
            {
                let res: SimulationResponse = deps.querier.query_wasm_smart(
                    &step.pool_addr,
                    &pair::QueryMsg::Simulation {
                        offer_asset: prev_asset.clone(),
                        ask_asset_info: None,
                    },
                )?;

                let allowed_spread = if i == 0 && step.pool_addr == pair_info.contract_addr {
                    // If the first step is the pool to be whitelisted, we allow a spread liq_percent + allowed_spread_per_step,
                    // since we are simulating with a fraction of the pool's liquidity.
                    self.liq_percent + self.allowed_spread_per_step
                } else {
                    self.allowed_spread_per_step
                };

                let spread = Decimal::from_ratio(res.spread_amount, res.return_amount);
                ensure!(
                    spread <= allowed_spread,
                    StdError::generic_err(format!("Spread {spread} is too high for step with pair {}. Max allowed is {allowed_spread}", step.pool_addr))
                );

                self.routes_cache
                    .insert(prev_asset.info.clone(), step.clone());

                step.asset_out.with_balance(res.return_amount)
            } else {
                let mut ret_asset = None;
                for def_asset in &self.default_assets {
                    // Closure that finds an asset_out with an acceptable spread for a given pair
                    // Returns Some((asset_out, pool_addr)) if found, None otherwise
                    let yield_ret_asset_if_low_spread = |pi: &PairInfo| {
                        let res: SimulationResponse = deps
                            .querier
                            .query_wasm_smart(
                                &pair_info.contract_addr,
                                &pair::QueryMsg::Simulation {
                                    offer_asset: prev_asset.clone(),
                                    ask_asset_info: None,
                                },
                            )
                            .ok()?;

                        let allowed_spread =
                            if i == 0 && pi.contract_addr == pair_info.contract_addr {
                                // If the first step is the pool to be whitelisted, we allow a spread liq_percent + allowed_spread_per_step,
                                // since we are simulating with a fraction of the pool's liquidity.
                                self.liq_percent + self.allowed_spread_per_step
                            } else {
                                self.allowed_spread_per_step
                            };

                        let spread = Decimal::from_ratio(res.spread_amount, res.return_amount);
                        if spread <= allowed_spread {
                            Some((
                                def_asset.with_balance(res.return_amount),
                                pi.contract_addr.clone(),
                            ))
                        } else {
                            None
                        }
                    };

                    // Prepare query params for factory
                    let asset_infos = vec![prev_asset.info.clone(), def_asset.clone()];
                    let mut start_after = None;

                    loop {
                        let pairs: Vec<PairInfo> = deps.querier.query_wasm_smart(
                            &self.factory,
                            &factory::QueryMsg::PairsByAssetInfos {
                                asset_infos: asset_infos.clone(),
                                start_after: start_after.clone(),
                                limit: Some(PAGINATION_LIMIT),
                            },
                        )?;

                        if let Some((asset_out, pool_addr)) =
                            pairs.iter().find_map(yield_ret_asset_if_low_spread)
                        {
                            self.routes_cache.insert(
                                prev_asset.info.clone(),
                                RouteStep {
                                    asset_out: def_asset.clone(),
                                    pool_addr,
                                },
                            );

                            ret_asset = Some(asset_out);
                            break;
                        }

                        if pairs.len() < PAGINATION_LIMIT as usize {
                            break;
                        }
                        start_after = pairs.last().map(|p| p.contract_addr.to_string());
                    }
                }

                ret_asset.ok_or_else(|| RouterError::RouteNotFound {
                    asset: prev_asset.to_string(),
                })?
            };
        }

        ensure_eq!(
            &prev_asset.info,
            target_asset_info,
            RouterError::FailedToBuildRoute {
                asset: asset_in.to_string(),
            }
        );

        Ok(prev_asset)
    }

    pub fn set_routes(
        &mut self,
        deps: DepsMut,
        routes: Vec<RouteStep>,
        astro_denom: &str,
    ) -> Result<Response, RouterError> {
        ensure!(!routes.is_empty(), RouterError::EmptyRoutes {});

        let mut attrs = vec![attr("action", "set_routes")];

        let astro = AssetInfo::native(astro_denom);
        let mut assets_in_set = HashSet::new();

        for route in &routes {
            let (pair_info, asset_in) = get_validated_pair_info(
                deps.querier,
                &self.factory,
                &route.pool_addr,
                &route.asset_out,
            )?;

            ensure!(
                assets_in_set.insert(asset_in.clone()),
                RouterError::DuplicatedRoutes {
                    asset: asset_in.to_string()
                }
            );

            ensure_ne!(asset_in, astro, RouterError::AstroInRoute {});

            let route_key = asset_info_key(&asset_in);
            if ROUTES.has(deps.storage, &route_key) {
                attrs.push(attr("updated_route", asset_in.to_string()));
            }

            let route_step = RouteStep {
                asset_out: route.asset_out.clone(),
                pool_addr: pair_info.contract_addr.clone(),
            };

            // If route exists then this iteration updates the route.
            ROUTES.save(deps.storage, &route_key, &route_step)?;

            self.routes_cache.insert(asset_in.clone(), route_step);
        }

        // Check all updated routes lead to ASTRO. It also checks for possible loops.
        for asset_in in self.routes_cache.keys().cloned().collect_vec() {
            self.build_route(deps.storage, &asset_in, &astro)
                .map(|_| ())?;
        }

        Ok(Response::new().add_attributes(attrs))
    }

    pub fn set_default_assets(
        &mut self,
        storage: &mut dyn Storage,
        assets: Vec<AssetInfo>,
        astro_denom: &str,
    ) -> Result<Response, RouterError> {
        let astro = AssetInfo::native(astro_denom);

        // Check all default assets lead to ASTRO
        for asset_in in &assets {
            self.build_route(storage, asset_in, &astro).map(|_| ())?;
        }

        DEFAULT_ASSETS.save(storage, &assets)?;

        Ok(Response::new().add_attributes(vec![
            attr("action", "set_default_assets"),
            attr("assets", assets.iter().map(|a| a.to_string()).join(",")),
        ]))
    }
}

pub fn query_routes(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<RouteStepVerbose>> {
    let limit = limit.unwrap_or(PAGINATION_LIMIT) as usize;
    let start_after = start_after
        .map(|asset| {
            determine_asset_info(&asset, deps.api).map(|asset_info| asset_info_key(&asset_info))
        })
        .transpose()?;

    ROUTES
        .range(
            deps.storage,
            start_after.as_deref().map(Bound::exclusive),
            None,
            Order::Ascending,
        )
        .map(|item| {
            item.and_then(|(asset_in_key, route_step)| {
                Ok(RouteStepVerbose {
                    asset_in: from_key_to_asset_info(asset_in_key)?,
                    asset_out: route_step.asset_out,
                    pool_addr: route_step.pool_addr.to_string(),
                })
            })
        })
        .take(limit)
        .collect()
}

/// Validates that pair was registered using the official Astroport factory.
/// Ensures target asset is one of the pair's assets.
/// Returns the pair info from the factory and the other asset (not target).
pub fn get_validated_pair_info(
    querier: QuerierWrapper,
    factory: impl Into<String>,
    pool_addr: impl Into<String>,
    target_asset_info: &AssetInfo,
) -> StdResult<(PairInfo, AssetInfo)> {
    let pool_addr = pool_addr.into();
    let pool_pair_info: PairInfo =
        querier.query_wasm_smart(&pool_addr, &pair::QueryMsg::Pair {})?;

    let factory_pair_info: PairInfo = querier.query_wasm_smart(
        factory,
        &factory::QueryMsg::PairByLpToken {
            lp_token: pool_pair_info.liquidity_token,
        },
    )?;

    ensure_eq!(
        pool_pair_info.contract_addr,
        factory_pair_info.contract_addr,
        StdError::generic_err(format!(
            "Pool address mismatch: {pool_addr} != {}",
            factory_pair_info.contract_addr
        ))
    );

    ensure!(
        factory_pair_info.asset_infos.contains(target_asset_info),
        StdError::generic_err(format!(
            "Invalid pool asset: pool {pool_addr} doesn't contain asset {target_asset_info}"
        ))
    );

    let asset_in = if &factory_pair_info.asset_infos[0] == target_asset_info {
        factory_pair_info.asset_infos[1].clone()
    } else {
        factory_pair_info.asset_infos[0].clone()
    };

    Ok((factory_pair_info, asset_in))
}

pub fn asset_info_key(asset_info: &AssetInfo) -> Vec<u8> {
    let mut bytes = vec![];
    match asset_info {
        AssetInfo::NativeToken { denom } => {
            bytes.push(0);
            bytes.extend_from_slice(denom.as_bytes());
        }
        AssetInfo::Token { contract_addr } => {
            bytes.push(1);
            bytes.extend_from_slice(contract_addr.as_bytes());
        }
    }

    bytes
}

pub fn from_key_to_asset_info(bytes: Vec<u8>) -> StdResult<AssetInfo> {
    match bytes[0] {
        0 => String::from_utf8(bytes[1..].to_vec())
            .map_err(StdError::invalid_utf8)
            .map(AssetInfo::native),
        1 => String::from_utf8(bytes[1..].to_vec())
            .map_err(StdError::invalid_utf8)
            .map(AssetInfo::cw20_unchecked),
        _ => Err(StdError::generic_err(
            "Failed to deserialize asset info key",
        )),
    }
}
