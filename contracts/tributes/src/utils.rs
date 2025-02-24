use std::collections::HashMap;

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use cosmwasm_std::{
    attr, Addr, Decimal, Deps, Event, Order, QuerierWrapper, StdError, StdResult, Uint128,
};

use astroport_governance::emissions_controller;
use astroport_governance::emissions_controller::consts::EPOCH_LENGTH;
use astroport_governance::emissions_controller::hub::{UserInfoResponse, VotedPoolInfo};
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

pub fn calculate_user_rewards(
    deps: Deps,
    config: &Config,
    user: &str,
    block_ts: u64,
) -> StdResult<(Vec<Asset>, Vec<Event>)> {
    // If a user has never interacted with the tributes contract
    // they iterate over all passed epochs
    let last_claim_ts = USER_LAST_CLAIM_EPOCH
        .may_load(deps.storage, user)?
        .unwrap_or(config.initial_epoch)
        + EPOCH_LENGTH;

    let mut rewards: HashMap<AssetInfo, Uint128> = HashMap::new();

    let mut events = vec![];
    for epoch_start_ts in (last_claim_ts..=block_ts).step_by(EPOCH_LENGTH as usize) {
        let user_share_per_pool = query_voting_power_per_pool(
            deps.querier,
            &config.emissions_controller,
            user,
            // Query voting power 1 second before epoch started.
            // Because tributes are paid in the next epoch.
            epoch_start_ts - 1,
        )?;

        let mut attrs = vec![];

        for (lp_token, user_share) in user_share_per_pool {
            attrs.push(attr("lp_token", &lp_token));

            let tributes = TRIBUTES
                .prefix((epoch_start_ts, &lp_token))
                .range(deps.storage, None, None, Order::Ascending)
                .collect::<StdResult<Vec<_>>>()?;

            for (asset_info_key, mut tribute_info) in tributes {
                let asset_info = from_key_to_asset_info(asset_info_key)?;
                let asset_reward = rewards
                    .entry(asset_info.clone())
                    .or_insert_with(Uint128::zero);

                let user_amount = tribute_info.allocated * user_share;

                *asset_reward += user_amount;
                tribute_info.available = tribute_info.available.checked_sub(user_amount)?;

                attrs.push(attr(
                    "tribute",
                    asset_info.with_balance(user_amount).to_string(),
                ));
            }
        }

        events.push(Event::new(format!("epoch_{epoch_start_ts}")).add_attributes(attrs));
    }

    let rewards = rewards
        .into_iter()
        .filter_map(|(asset_info, amount)| {
            if !amount.is_zero() {
                Some(asset_info.with_balance(amount))
            } else {
                None
            }
        })
        .collect();

    Ok((rewards, events))
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
