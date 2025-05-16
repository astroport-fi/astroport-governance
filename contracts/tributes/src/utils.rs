use std::collections::HashMap;

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use cosmwasm_std::{Addr, Decimal, Deps, Order, QuerierWrapper, StdError, StdResult, Uint128};
use itertools::Itertools;

use astroport_governance::emissions_controller;
use astroport_governance::emissions_controller::consts::EPOCH_LENGTH;
use astroport_governance::emissions_controller::hub::{UserInfoResponse, VotedPoolInfo};
use astroport_governance::emissions_controller::utils::get_epoch_start;
use astroport_governance::tributes::Config;

use crate::state::{TRIBUTES, USER_LAST_CLAIM_EPOCH};

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

pub fn query_voting_power_per_pool(
    querier: QuerierWrapper,
    em_controller: &Addr,
    user: &str,
    timestamp: u64,
) -> StdResult<Vec<(String, Decimal)>> {
    querier
        .query_wasm_smart::<UserInfoResponse>(
            em_controller,
            &emissions_controller::hub::QueryMsg::UserInfo {
                user: user.to_string(),
                timestamp: Some(timestamp),
            },
        )
        .and_then(|res| {
            res.votes
                .into_iter()
                .map(|(pool, share)| {
                    let total_vp = querier
                        .query_wasm_smart::<VotedPoolInfo>(
                            em_controller,
                            &emissions_controller::hub::QueryMsg::VotedPool {
                                pool: pool.clone(),
                                timestamp: Some(timestamp),
                            },
                        )?
                        .voting_power;

                    Ok((
                        pool,
                        Decimal::from_ratio(share * res.voting_power, total_vp),
                    ))
                })
                .collect()
        })
}

pub type RawClaimResponse = HashMap<u64, HashMap<String, HashMap<AssetInfo, Uint128>>>;

pub fn calculate_user_rewards(
    deps: Deps,
    config: &Config,
    user: &str,
    block_ts: u64,
) -> StdResult<RawClaimResponse> {
    // If a user has never interacted with the tributes contract
    // they iterate over all passed epochs
    let last_claim_ts = USER_LAST_CLAIM_EPOCH
        .may_load(deps.storage, user)?
        .unwrap_or(config.initial_epoch)
        + EPOCH_LENGTH;

    let mut rewards: HashMap<u64, HashMap<String, HashMap<AssetInfo, Uint128>>> = HashMap::new();

    for epoch_start_ts in (last_claim_ts..=block_ts).step_by(EPOCH_LENGTH as usize) {
        let mut epoch: HashMap<String, HashMap<AssetInfo, Uint128>> = HashMap::new();

        let user_share_per_pool = query_voting_power_per_pool(
            deps.querier,
            &config.emissions_controller,
            user,
            // Query voting power 1 second before epoch started.
            // Because tributes are paid in the next epoch.
            epoch_start_ts - 1,
        )?;

        for (lp_token, user_share) in user_share_per_pool {
            let mut pool: HashMap<AssetInfo, Uint128> = HashMap::new();

            let tributes = TRIBUTES
                .prefix((epoch_start_ts, &lp_token))
                .range(deps.storage, None, None, Order::Ascending)
                .collect::<StdResult<Vec<_>>>()?;

            for (asset_info_key, tribute_info) in tributes {
                let asset_info = from_key_to_asset_info(asset_info_key)?;

                let user_amount = tribute_info.allocated * user_share;

                if !user_amount.is_zero() {
                    pool.insert(asset_info.clone(), user_amount);
                }
            }

            if !pool.is_empty() {
                epoch.insert(lp_token.clone(), pool);
            }
        }

        if !epoch.is_empty() {
            rewards.insert(epoch_start_ts, epoch);
        }
    }

    Ok(rewards)
}

pub fn get_orphaned_tributes(
    deps: Deps,
    em_controller: &Addr,
    epoch_ts: u64,
) -> StdResult<HashMap<String, Vec<Asset>>> {
    let epoch_start = get_epoch_start(epoch_ts);

    let orphaned: HashMap<_, Vec<_>> = TRIBUTES
        .sub_prefix(epoch_start)
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            item.and_then(|((lp_token, asset_info_key), tribute_info)| {
                let total_vp = deps
                    .querier
                    .query_wasm_smart::<VotedPoolInfo>(
                        em_controller,
                        &emissions_controller::hub::QueryMsg::VotedPool {
                            pool: lp_token.clone(),
                            timestamp: Some(epoch_start - 1),
                        },
                    )?
                    .voting_power;

                if total_vp.is_zero() {
                    let asset_info = from_key_to_asset_info(asset_info_key)?;
                    Ok(Some((
                        lp_token.to_string(),
                        asset_info.with_balance(tribute_info.allocated),
                    )))
                } else {
                    Ok(None)
                }
            })
        })
        .collect::<StdResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .into_group_map()
        .into_iter()
        .collect();

    Ok(orphaned)
}

#[cfg(test)]
mod unit_tests {
    use astroport::asset::AssetInfo;

    use super::*;

    #[test]
    fn test_asset_info_binary_key() {
        let asset_infos = vec![
            AssetInfo::native("uusd"),
            AssetInfo::cw20_unchecked("wasm1contractxxx"),
        ];

        for asset_info in asset_infos {
            let key = asset_info_key(&asset_info);
            assert_eq!(from_key_to_asset_info(key).unwrap(), asset_info);
        }
    }

    #[test]
    fn test_deserialize_asset_info_from_malformed_data() {
        let asset_infos = vec![
            AssetInfo::native("uusd"),
            AssetInfo::cw20_unchecked("wasm1contractxxx"),
        ];

        for asset_info in asset_infos {
            let mut key = asset_info_key(&asset_info);
            key[0] = 2;

            assert_eq!(
                from_key_to_asset_info(key).unwrap_err(),
                StdError::generic_err("Failed to deserialize asset info key")
            );
        }

        let key = vec![0, u8::MAX];
        assert_eq!(
            from_key_to_asset_info(key).unwrap_err().to_string(),
            "Cannot decode UTF8 bytes into string: invalid utf-8 sequence of 1 bytes from index 0"
        );
    }
}
