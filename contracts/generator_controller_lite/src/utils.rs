use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::ops::RangeInclusive;

use astroport_governance::generator_controller_lite::{
    ConfigResponse, GaugeInfoResponse, NetworkInfo,
};
use cosmwasm_std::{Order, StdError, StdResult, Storage, Uint128};
use cw_storage_plus::Bound;

use crate::bps::BasicPoints;
use crate::error::ContractError;
use crate::state::{VotedPoolInfo, POOLS, POOL_PERIODS, POOL_VOTES};

/// Pools limit should be within the range `[2, 100]`
const POOL_NUMBER_LIMIT: RangeInclusive<u64> = 2..=100;

/// The enum defines math operations with voting power and slope.
#[derive(Debug)]
pub(crate) enum Operation {
    Add,
    Sub,
}

impl Operation {
    pub fn calc_voting_power(&self, cur_vp: Uint128, vp: Uint128, bps: BasicPoints) -> Uint128 {
        match self {
            Operation::Add => cur_vp + bps * vp,
            Operation::Sub => cur_vp.saturating_sub(bps * vp),
        }
    }
}

/// Enum wraps [`VotedPoolInfo`] so the contract can leverage storage operations efficiently.
#[derive(Debug)]
pub(crate) enum VotedPoolInfoResult {
    Unchanged(VotedPoolInfo),
    New(VotedPoolInfo),
}

/// Filters pairs (LP token address, voting parameters) by only taking up to
/// pool_limit
/// We can no longer validate the pools as they might be on a different chain
pub(crate) fn filter_pools(
    pools: Vec<(String, Uint128)>,
    pools_limit: u64,
) -> StdResult<Vec<(String, Uint128)>> {
    let pools = pools
        .into_iter()
        .map(|(pool_addr, vxastro_amount)| (pool_addr, vxastro_amount))
        .take(pools_limit as usize)
        .collect();
    Ok(pools)
}

/// Cancels user changes using old voting parameters for a given pool.  
/// Firstly, it removes slope change scheduled for previous lockup end period.  
/// Secondly, it updates voting parameters for the given period, but without user's vote.
pub(crate) fn cancel_user_changes(
    storage: &mut dyn Storage,
    period: u64,
    pool_addr: &str,
    old_bps: BasicPoints,
    old_vp: Uint128,
) -> StdResult<()> {
    update_pool_info(
        storage,
        period,
        pool_addr,
        Some((old_bps, old_vp, Operation::Sub)),
    )
    .map(|_| ())
}

/// Applies user's vote for a given pool.   
/// It updates voting parameters with applied user's vote.
pub(crate) fn vote_for_pool(
    storage: &mut dyn Storage,
    period: u64,
    pool_addr: &str,
    bps: BasicPoints,
    vp: Uint128,
) -> StdResult<()> {
    update_pool_info(storage, period, pool_addr, Some((bps, vp, Operation::Add))).map(|_| ())
}

/// Fetches voting parameters for a given pool at specific period, applies new changes, saves it in storage
/// and returns new voting parameters in [`VotedPoolInfo`] object.
/// If there are no changes in 'changes' parameter
/// and voting parameters were already calculated before the function just returns [`VotedPoolInfo`].
pub(crate) fn update_pool_info(
    storage: &mut dyn Storage,
    period: u64,
    pool_addr: &str,
    changes: Option<(BasicPoints, Uint128, Operation)>,
) -> StdResult<VotedPoolInfo> {
    if POOLS.may_load(storage, pool_addr)?.is_none() {
        POOLS.save(storage, pool_addr, &())?
    }
    let period_key = period;
    let pool_info = match get_pool_info_mut(storage, period, pool_addr)? {
        VotedPoolInfoResult::Unchanged(mut pool_info) | VotedPoolInfoResult::New(mut pool_info)
            if changes.is_some() =>
        {
            if let Some((bps, vp, op)) = changes {
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

/// Returns pool info at specified period
pub(crate) fn get_pool_info_mut(
    storage: &mut dyn Storage,
    period: u64,
    pool_addr: &str,
) -> StdResult<VotedPoolInfoResult> {
    let pool_info_result =
        if let Some(pool_info) = POOL_VOTES.may_load(storage, (period, pool_addr))? {
            VotedPoolInfoResult::Unchanged(pool_info)
        } else {
            let pool_info_result =
                if let Some(prev_period) = fetch_last_pool_period(storage, period, pool_addr)? {
                    let pool_info = POOL_VOTES.load(storage, (prev_period, pool_addr))?;
                    VotedPoolInfo {
                        vxastro_amount: pool_info.vxastro_amount,
                        ..pool_info
                    }
                } else {
                    VotedPoolInfo::default()
                };

            VotedPoolInfoResult::New(pool_info_result)
        };

    Ok(pool_info_result)
}

/// Returns pool info at specified period.
pub(crate) fn get_pool_info(
    storage: &dyn Storage,
    period: u64,
    pool_addr: &str,
) -> StdResult<VotedPoolInfo> {
    let pool_info = if let Some(pool_info) = POOL_VOTES.may_load(storage, (period, pool_addr))? {
        pool_info
    } else if let Some(prev_period) = fetch_last_pool_period(storage, period, pool_addr)? {
        let pool_info = POOL_VOTES.load(storage, (prev_period, pool_addr))?;
        VotedPoolInfo {
            vxastro_amount: pool_info.vxastro_amount,
            ..pool_info
        }
    } else {
        VotedPoolInfo::default()
    };

    Ok(pool_info)
}

/// Fetches last period for specified pool which has saved result in [`POOL_PERIODS`].
pub(crate) fn fetch_last_pool_period(
    storage: &dyn Storage,
    period: u64,
    pool_addr: &str,
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

/// Input validation for pools limit.
pub(crate) fn validate_pools_limit(number: u64) -> Result<u64, ContractError> {
    if !POOL_NUMBER_LIMIT.contains(&number) {
        Err(ContractError::InvalidPoolNumber(number))
    } else {
        Ok(number)
    }
}

/// Check if a pool isn't the main pool. Check if a pool is an LP token.
/// In the lite version this no longer validates if a pool is an LP token
/// or that it is registered in the factory. That is because in the lite
/// version we are dealing with multi chain addresses
pub fn validate_pool(config: &ConfigResponse, pool: &str) -> Result<(), ContractError> {
    // Voting for the main pool or updating it is prohibited
    if let Some(main_pool) = &config.main_pool {
        if pool == *main_pool {
            return Err(ContractError::MainPoolVoteOrWhitelistingProhibited(
                main_pool.to_string(),
            ));
        }
    }
    Ok(())
}

/// Checks for duplicate items in a slice
pub fn check_duplicated<T: Eq + Hash>(items: &[T]) -> Result<(), ContractError> {
    let mut uniq = HashSet::new();
    if !items.iter().all(|item| uniq.insert(item)) {
        return Err(ContractError::DuplicatedPools {});
    }

    Ok(())
}

/// Filters pools by network prefixes to enable sending the message to the
/// correct contracts
pub fn group_pools_by_network<'a>(
    networks: &'a [NetworkInfo],
    gauge_info: &GaugeInfoResponse,
) -> HashMap<&'a NetworkInfo, Vec<(String, Uint128)>> {
    networks
        .iter()
        .map(|network_info| {
            let matching_pools: Vec<_> = gauge_info
                .pool_alloc_points
                .iter()
                .filter(|(address, _)| address.starts_with(network_info.address_prefix.as_str()))
                .cloned()
                .collect();

            (network_info, matching_pools)
        })
        .collect()
}

/// Finds the prefix by returning all the characters before the first instance
/// of the first instance of "1" as Cosmos addresses are all based on prefix1restofaddress
/// If the prefix could not be determined, an error is returned
pub fn determine_address_prefix(s: &str) -> Result<String, ContractError> {
    let prefix: String = s.chars().take_while(|&c| c != '1').collect();
    if prefix.is_empty() {
        Err(ContractError::Std(StdError::GenericErr {
            msg: "Invalid prefix".to_string(),
        }))
    } else {
        Ok(prefix)
    }
}

#[test]
fn test_determine_address_prefix() {
    // Test that the prefix is determined correctly, format is
    // (expected_prefix, address)
    let test_addresses = vec![
        ("inj", "inj19aenkaj6qhymmt746av8ck4r8euthq3zmxr2r6"),
        ("inj", "inj1z354nkau8f0dukgwctq9mladvdwu6zcj8k4928"),
        (
            "neutron",
            "neutron1eeyntmsq448c68ez06jsy6h2mtjke5tpuplnwtjfwcdznqmw72kswnlmm0",
        ),
        (
            "neutron",
            "neutron1unc0549k2f0d7mjjyfm94fuz2x53wrx3px0pr55va27grdgmspcqgzfr8p",
        ),
        (
            "sei",
            "sei1suhgf5svhu4usrurvxzlgn54ksxmn8gljarjtxqnapv8kjnp4nrsgshtdj",
        ),
        (
            "terra",
            "terra15hlvnufpk8a3gcex09djzkhkz3jg9dpqvv6fvgd0ynudtu2z0qlq2fyfaq",
        ),
        ("terra", "terra174gu7kg8ekk5gsxdma5jlfcedm653tyg6ayppw"),
        ("contract", "contract"),
        ("contract", "contract1"),
        ("contract", "contract1abc"),
        ("wasm", "wasm12345"),
    ];

    for (expected_prefix, address) in test_addresses {
        let prefix = determine_address_prefix(address).unwrap();
        assert_eq!(expected_prefix, prefix);
    }
}
