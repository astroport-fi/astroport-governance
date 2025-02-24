use astroport::asset::AssetInfoExt;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, Order, StdResult};
use cw_storage_plus::Bound;

use astroport_governance::emissions_controller::consts::EPOCH_LENGTH;
use astroport_governance::emissions_controller::utils::get_epoch_start;
use astroport_governance::tributes::QueryMsg;
use astroport_governance::DEFAULT_LIMIT;

use crate::state::{CONFIG, TRIBUTES};
use crate::utils::{asset_info_key, calculate_user_rewards, from_key_to_asset_info};

/// Expose available contract queries.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => CONFIG
            .load(deps.storage)
            .and_then(|config| to_json_binary(&config)),
        QueryMsg::IsFeeExpected {
            lp_token,
            asset_info,
        } => {
            let next_epoch_start = get_epoch_start(env.block.time.seconds()) + EPOCH_LENGTH;
            let asset_key = asset_info_key(&asset_info);
            let tribute_key = (next_epoch_start, lp_token.as_str(), asset_key.as_slice());

            to_json_binary(&!TRIBUTES.has(deps.storage, tribute_key))
        }
        QueryMsg::QueryPoolTributes { epoch_ts, lp_token } => {
            let epoch_start = get_epoch_start(
                epoch_ts.unwrap_or_else(|| env.block.time.seconds() + EPOCH_LENGTH),
            );

            let tribute_tokens = TRIBUTES
                .prefix((epoch_start, &lp_token))
                .range(deps.storage, None, None, Order::Ascending)
                .map(|item| {
                    item.and_then(|(asset_info_key, tribute_info)| {
                        let asset_info = from_key_to_asset_info(asset_info_key)?;
                        Ok(asset_info.with_balance(tribute_info.allocated))
                    })
                })
                .collect::<StdResult<Vec<_>>>()?;

            to_json_binary(&tribute_tokens)
        }
        QueryMsg::QueryAllEpochTributes {
            epoch_ts,
            start_after,
            limit,
        } => {
            let epoch_start = get_epoch_start(
                epoch_ts.unwrap_or_else(|| env.block.time.seconds() + EPOCH_LENGTH),
            );

            let limit = limit.unwrap_or(DEFAULT_LIMIT);

            let prefixed_storage = TRIBUTES.sub_prefix(epoch_start);
            let range = if let Some((lp_token, asset_info)) = start_after {
                let asset_key = asset_info_key(&asset_info);
                prefixed_storage.range(
                    deps.storage,
                    Some(Bound::exclusive((lp_token.as_str(), asset_key.as_slice()))),
                    None,
                    Order::Ascending,
                )
            } else {
                prefixed_storage.range(deps.storage, None, None, Order::Ascending)
            };

            let tributes = range
                .map(|item| {
                    item.and_then(|((lp_token, asset_info_key), tribute_info)| {
                        let asset_info = from_key_to_asset_info(asset_info_key)?;
                        Ok((
                            lp_token.to_string(),
                            asset_info.with_balance(tribute_info.allocated),
                        ))
                    })
                })
                .take(limit as usize)
                .collect::<StdResult<Vec<_>>>()?;

            to_json_binary(&tributes)
        }
        QueryMsg::SimulateClaim { address } => {
            let config = CONFIG.load(deps.storage)?;
            calculate_user_rewards(deps, &config, &address, env.block.time.seconds())
                .and_then(|(rewards, _)| to_json_binary(&rewards))
        }
    }
}
