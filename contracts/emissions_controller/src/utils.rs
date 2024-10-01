use std::collections::{HashMap, HashSet};

use astroport::asset::{determine_asset_info, Asset};
use astroport::common::LP_SUBDENOM;
use astroport::incentives::{IncentivesSchedule, InputSchedule};
use cosmwasm_schema::cw_serde;
use cosmwasm_schema::serde::Serialize;
use cosmwasm_std::{
    coin, Coin, CosmosMsg, Decimal, Deps, Env, Order, QuerierWrapper, StdError, StdResult, Storage,
    Uint128,
};
use itertools::Itertools;
use neutron_sdk::bindings::msg::{IbcFee, NeutronMsg};
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::query::min_ibc_fee::query_min_ibc_fee;
use neutron_sdk::sudo::msg::RequestPacketTimeoutHeight;

use astroport_governance::emissions_controller::consts::{
    EPOCHS_START, EPOCH_LENGTH, FEE_DENOM, IBC_TIMEOUT,
};
use astroport_governance::emissions_controller::hub::{
    Config, EmissionsState, OutpostInfo, OutpostParams,
};
use astroport_governance::emissions_controller::outpost::OutpostMsg;
use astroport_governance::emissions_controller::utils::check_lp_token;

use crate::error::ContractError;
use crate::state::{get_active_outposts, OUTPOSTS, POOLS_WHITELIST, TUNE_INFO, VOTED_POOLS};

/// Determine outpost prefix from address or tokenfactory denom.
pub fn determine_outpost_prefix(value: &str) -> Option<String> {
    let mut maybe_addr = Some(value);

    if value.starts_with("factory/") && value.ends_with(LP_SUBDENOM) {
        maybe_addr = value.split('/').nth(1);
    }

    maybe_addr.and_then(|value| {
        value.find('1').and_then(|delim_ind| {
            if delim_ind > 0 && value.chars().all(char::is_alphanumeric) {
                Some(value[..delim_ind].to_string())
            } else {
                None
            }
        })
    })
}

/// Determine outpost prefix for the pool LP token and validate
/// that this outpost exists.
pub fn get_outpost_prefix(
    pool: &str,
    outpost_prefixes: &HashMap<String, OutpostInfo>,
) -> Option<String> {
    determine_outpost_prefix(pool).and_then(|maybe_prefix| {
        if outpost_prefixes.contains_key(&maybe_prefix) {
            Some(maybe_prefix)
        } else {
            None
        }
    })
}

/// Validate LP token denom or address matches outpost prefix.
pub fn validate_outpost_prefix(value: &str, prefix: &str) -> Result<(), ContractError> {
    determine_outpost_prefix(value)
        .and_then(|maybe_prefix| {
            if maybe_prefix == prefix {
                Some(maybe_prefix)
            } else {
                None
            }
        })
        .ok_or_else(|| ContractError::InvalidOutpostPrefix(value.to_string()))
        .map(|_| ())
}

/// Helper function to get outpost prefix from an IBC channel.
pub fn get_outpost_from_hub_channel(
    store: &dyn Storage,
    source_channel: String,
    get_channel_closure: impl Fn(&OutpostParams) -> &String,
) -> StdResult<String> {
    get_active_outposts(store)?
        .into_iter()
        .find_map(|(outpost_prefix, outpost)| {
            outpost.params.as_ref().and_then(|params| {
                if get_channel_closure(params).eq(&source_channel) {
                    Some(outpost_prefix.clone())
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| {
            StdError::generic_err(format!(
                "Unknown outpost with {source_channel} ics20 channel"
            ))
        })
}

#[cw_serde]
pub enum IbcHookMemo<T> {
    Wasm { contract: String, msg: T },
}

impl<T: Serialize> IbcHookMemo<T> {
    pub fn build(contract: &str, msg: T) -> StdResult<String> {
        serde_json::to_string(&IbcHookMemo::Wasm {
            contract: contract.to_string(),
            msg,
        })
        .map_err(|err| StdError::generic_err(err.to_string()))
    }
}

pub fn min_ntrn_ibc_fee(deps: Deps<NeutronQuery>) -> Result<IbcFee, ContractError> {
    let fee = query_min_ibc_fee(deps)?.min_fee;

    Ok(IbcFee {
        recv_fee: fee.recv_fee,
        ack_fee: fee
            .ack_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
        timeout_fee: fee
            .timeout_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
    })
}

/// Compose ics20 message with IBC hook memo for outpost emissions controller.
pub fn build_emission_ibc_msg(
    env: &Env,
    params: &OutpostParams,
    ibc_fee: &IbcFee,
    astro_funds: Coin,
    schedules: &[(String, InputSchedule)],
) -> StdResult<CosmosMsg<NeutronMsg>> {
    let outpost_controller_msg =
        astroport_governance::emissions_controller::msg::ExecuteMsg::Custom(
            OutpostMsg::SetEmissions {
                schedules: schedules.to_vec(),
            },
        );
    Ok(NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: params.ics20_channel.clone(),
        token: astro_funds,
        sender: env.contract.address.to_string(),
        receiver: params.emissions_controller.clone(),
        timeout_height: RequestPacketTimeoutHeight {
            revision_number: None,
            revision_height: None,
        },
        timeout_timestamp: env.block.time.plus_seconds(IBC_TIMEOUT).nanos(),
        memo: IbcHookMemo::build(&params.emissions_controller, outpost_controller_msg)?,
        fee: ibc_fee.clone(),
    }
    .into())
}

/// This function converts schedule pairs (lp_token, ASTRO amount)
/// into the incentives contract executable message.
/// It also calculates total ASTRO funds required for the emissions.
pub fn raw_emissions_to_schedules(
    env: &Env,
    raw_schedules: &[(String, Uint128)],
    schedule_denom: &str,
    hub_denom: &str,
) -> (Vec<(String, InputSchedule)>, Coin) {
    let mut total_astro = Uint128::zero();
    // Ensure emissions >=1 uASTRO per second.
    // >= 1 uASTRO per second is the requirement in the incentives contract.
    let schedules = raw_schedules
        .iter()
        .filter_map(|(pool, astro_amount)| {
            let schedule = InputSchedule {
                reward: Asset::native(schedule_denom, *astro_amount),
                duration_periods: 1,
            };
            // Schedule validation imported from the incentives contract
            IncentivesSchedule::from_input(env, &schedule).ok()?;

            total_astro += astro_amount;
            Some((pool.clone(), schedule))
        })
        .collect_vec();

    let astro_funds = coin(total_astro.u128(), hub_denom);

    (schedules, astro_funds)
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

/// Query the staking contract ASTRO balance and xASTRO total supply and derive xASTRO staking rate.
/// Return (staking rate, total xASTRO supply).
pub fn get_xastro_rate_and_share(
    querier: QuerierWrapper,
    config: &Config,
) -> Result<(Decimal, Uint128), ContractError> {
    let total_deposit = querier
        .query_balance(&config.staking, &config.astro_denom)?
        .amount;
    let total_shares = querier.query_supply(&config.xastro_denom)?.amount;
    let rate = Decimal::checked_from_ratio(total_deposit, total_shares)?;

    Ok((rate, total_shares))
}

/// Calculate the number of ASTRO tokens collected by the staking contract from the previous epoch
/// and derive emissions for the upcoming epoch.  
///
/// Calculate two-epochs EMA by the following formula:
/// (V_n-1 * 2/3 + EMA_n-1 * 1/3),  
/// where V_n is the collected ASTRO at epoch n, n is the current epoch (a starting one).
///
/// Dynamic emissions formula is:  
/// next emissions = MAX(MIN(max_astro, V_n-1 * emissions_multiple), MIN(max_astro, two-epochs EMA))
pub fn astro_emissions_curve(
    deps: Deps,
    emissions_state: EmissionsState,
    config: &Config,
) -> Result<EmissionsState, ContractError> {
    let (actual_rate, shares) = get_xastro_rate_and_share(deps.querier, config)?;
    let growth = actual_rate - emissions_state.xastro_rate;
    let collected_astro = shares * growth;

    let two_thirds = Decimal::from_ratio(2u8, 3u8);
    let one_third = Decimal::from_ratio(1u8, 3u8);
    let ema = collected_astro * two_thirds + emissions_state.ema * one_third;

    let min_1 = (emissions_state.collected_astro * config.emissions_multiple).min(config.max_astro);
    let min_2 = (ema * config.emissions_multiple).min(config.max_astro);

    Ok(EmissionsState {
        xastro_rate: actual_rate,
        collected_astro,
        ema,
        emissions_amount: min_1.max(min_2),
    })
}

/// Internal structure to pass the tune simulation result.
pub struct TuneResult {
    /// All candidates with their voting power and outpost prefix.
    pub candidates: Vec<(String, (String, Uint128))>,
    /// Dynammic emissions curve state
    pub new_emissions_state: EmissionsState,
    /// Next pools grouped by outpost prefix.
    pub next_pools_grouped: HashMap<String, Vec<(String, Uint128)>>,
}

/// Simulate the next tune outcome based on the voting power distribution at given timestamp.
/// In actual tuning context (function tune_pools) timestamp must match current epoch start.
pub fn simulate_tune(
    deps: Deps,
    voted_pools: &HashSet<String>,
    outposts: &HashMap<String, OutpostInfo>,
    timestamp: u64,
    config: &Config,
) -> Result<TuneResult, ContractError> {
    // Determine outpost prefix and filter out non-outpost pools.
    let mut candidates = voted_pools
        .iter()
        .filter_map(|pool| get_outpost_prefix(pool, outposts).map(|prefix| (prefix, pool.clone())))
        .map(|(prefix, pool)| {
            let pool_vp = VOTED_POOLS
                .may_load_at_height(deps.storage, &pool, timestamp)?
                .map(|info| info.voting_power)
                .unwrap_or_default();
            Ok((prefix, (pool, pool_vp)))
        })
        .collect::<StdResult<Vec<_>>>()?;

    candidates.sort_by(
        |(_, (_, a)), (_, (_, b))| b.cmp(a), // Sort in descending order
    );

    let total_pool_limit = config.pools_per_outpost as usize * outposts.len();

    let tune_info = TUNE_INFO.load(deps.storage)?;

    let new_emissions_state = astro_emissions_curve(deps, tune_info.emissions_state, config)?;

    // Total voting power of all selected pools
    let total_selected_vp = candidates
        .iter()
        .take(total_pool_limit)
        .fold(Uint128::zero(), |acc, (_, (_, vp))| acc + vp);
    // Calculate each pool's ASTRO emissions
    let mut next_pools = candidates
        .iter()
        .take(total_pool_limit)
        .map(|(prefix, (pool, pool_vp))| {
            let astro_for_pool = new_emissions_state
                .emissions_amount
                .multiply_ratio(*pool_vp, total_selected_vp);
            (prefix.clone(), ((*pool).clone(), astro_for_pool))
        })
        .collect_vec();

    // Add astro pools for each registered outpost
    next_pools.extend(outposts.iter().filter_map(|(prefix, outpost)| {
        outpost.astro_pool_config.as_ref().map(|astro_pool_config| {
            (
                prefix.clone(),
                (
                    astro_pool_config.astro_pool.clone(),
                    astro_pool_config.constant_emissions,
                ),
            )
        })
    }));

    let next_pools_grouped: HashMap<_, _> = next_pools
        .into_iter()
        .filter(|(_, (_, astro_for_pool))| !astro_for_pool.is_zero())
        .into_group_map()
        .into_iter()
        .filter_map(|(prefix, pools)| {
            if outposts.get(&prefix).unwrap().params.is_none() {
                // Ensure on the Hub that all LP tokens are valid.
                // Otherwise, keep ASTRO directed to invalid pools on the emissions controller.
                let pools = pools
                    .into_iter()
                    .filter(|(pool, _)| {
                        determine_asset_info(pool, deps.api)
                            .and_then(|maybe_lp| {
                                check_lp_token(deps.querier, &config.factory, &maybe_lp)
                            })
                            .is_ok()
                    })
                    .collect_vec();
                if !pools.is_empty() {
                    Some((prefix, pools))
                } else {
                    None
                }
            } else {
                Some((prefix, pools))
            }
        })
        .collect();

    Ok(TuneResult {
        candidates,
        new_emissions_state,
        next_pools_grouped,
    })
}

/// Jails outpost as well as removes all whitelisted
/// and being voted pools related to this outpost.
pub fn jail_outpost(
    storage: &mut dyn Storage,
    prefix: &str,
    env: Env,
) -> Result<(), ContractError> {
    // Remove all votable pools related to this outpost
    let voted_pools = VOTED_POOLS
        .keys(storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?;
    let prefix_some = Some(prefix.to_string());
    voted_pools
        .iter()
        .filter(|pool| determine_outpost_prefix(pool) == prefix_some)
        .try_for_each(|pool| VOTED_POOLS.remove(storage, pool, env.block.time.seconds()))?;

    // And clear whitelist
    POOLS_WHITELIST.update::<_, StdError>(storage, |mut whitelist| {
        whitelist.retain(|pool| determine_outpost_prefix(pool) != prefix_some);
        Ok(whitelist)
    })?;

    OUTPOSTS.update(storage, prefix, |outpost| {
        if let Some(outpost) = outpost {
            Ok(OutpostInfo {
                jailed: true,
                ..outpost
            })
        } else {
            Err(ContractError::OutpostNotFound {
                prefix: prefix.to_string(),
            })
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_determine_outpost_prefix() {
        assert_eq!(
            determine_outpost_prefix(&format!("factory/wasm1addr{LP_SUBDENOM}")).unwrap(),
            "wasm"
        );
        assert_eq!(determine_outpost_prefix("wasm1addr").unwrap(), "wasm");
        assert_eq!(determine_outpost_prefix("1addr"), None);
        assert_eq!(
            determine_outpost_prefix(&format!("factory/1addr{LP_SUBDENOM}")),
            None
        );
        assert_eq!(determine_outpost_prefix("factory/wasm1addr/random"), None);
        assert_eq!(
            determine_outpost_prefix(&format!("factory{LP_SUBDENOM}")),
            None
        );
    }

    #[test]
    fn test_epoch_start() {
        assert_eq!(get_epoch_start(1716163200), 1716163200);
        assert_eq!(get_epoch_start(1716163200 + 1), 1716163200);
        assert_eq!(
            get_epoch_start(1716163200 + EPOCH_LENGTH),
            1716163200 + EPOCH_LENGTH
        );
        assert_eq!(
            get_epoch_start(1716163200 + EPOCH_LENGTH + 1),
            1716163200 + EPOCH_LENGTH
        );
    }
}
