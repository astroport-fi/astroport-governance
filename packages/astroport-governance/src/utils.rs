use cosmwasm_std::{
    Addr, ChannelResponse, IbcQuery, QuerierWrapper, StdError, StdResult, Uint128, Uint64,
};

use crate::hub::HubBalance;

/// Seconds in one week. It is intended for period number calculation.
pub const WEEK: u64 = 7 * 86400; // lock period is rounded down by week

/// Default unlock period for a vxASTRO lite lock
pub const DEFAULT_UNLOCK_PERIOD: u64 = 2 * WEEK;

pub const LITE_VOTING_PERIOD: u64 = 2 * WEEK;

/// Seconds in 2 years which is the maximum lock period.
pub const MAX_LOCK_TIME: u64 = 2 * 365 * 86400; // 2 years (104 weeks)

/// The constant describes the maximum number of accounts for which to claim accrued staking rewards in a single transaction.
pub const CLAIM_LIMIT: u64 = 10;

/// The constant describes the minimum number of accounts for claim.
pub const MIN_CLAIM_LIMIT: u64 = 2;

/// Feb 28 2022 00:00 UTC, Monday
pub const EPOCH_START: u64 = 1646006400;

/// Calculates the period number. Time should be formatted as a timestamp.
pub fn get_period(time: u64) -> StdResult<u64> {
    if time < EPOCH_START {
        Err(StdError::generic_err("Invalid time"))
    } else {
        Ok((time - EPOCH_START) / WEEK)
    }
}

/// Calculates the voting period number for vxASTRO lite. Time should be formatted as a timestamp.
pub fn get_lite_period(time: u64) -> StdResult<u64> {
    if time < EPOCH_START {
        Err(StdError::generic_err("Invalid time"))
    } else {
        Ok((time - EPOCH_START) / LITE_VOTING_PERIOD)
    }
}

/// Calculates how many periods are in the specified time interval. The time should be in seconds.
pub fn get_periods_count(interval: u64) -> u64 {
    interval / WEEK
}

/// Calculates how many periods are in the specified time interval for vxASTRO lite. The time should be in seconds.
pub fn get_lite_periods_count(interval: u64) -> u64 {
    interval / LITE_VOTING_PERIOD
}

/// Main function used to calculate a user's voting power at a specific period as: previous_power - slope*(x - previous_x).
pub fn calc_voting_power(
    slope: Uint128,
    old_vp: Uint128,
    start_period: u64,
    end_period: u64,
) -> Uint128 {
    let shift = slope
        .checked_mul(Uint128::from(end_period - start_period))
        .unwrap_or_else(|_| Uint128::zero());
    old_vp.saturating_sub(shift)
}

/// Checks that a contract supports a given IBC-channel.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **contract** is the contract to check channel support on.
///
/// * **given_channel** is an IBC channel id the function needs to check.
pub fn check_contract_supports_channel(
    querier: QuerierWrapper,
    contract: &Addr,
    given_channel: &String,
) -> StdResult<()> {
    let port_id = Some(format!("wasm.{contract}"));
    let ChannelResponse { channel } = querier.query(
        &IbcQuery::Channel {
            channel_id: given_channel.to_string(),
            port_id,
        }
        .into(),
    )?;
    channel.map(|_| ()).ok_or_else(|| {
        StdError::generic_err(format!(
            "The contract does not have channel {given_channel}"
        ))
    })
}

/// Retrieves the total amount of voting power held by all Outposts at a given time
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **contract** is the Hub contract address
///
/// * **timestamp** The unix timestamp at which to query the total voting power
pub fn get_total_outpost_voting_power_at(
    querier: QuerierWrapper,
    contract: &Addr,
    timestamp: u64,
) -> Result<Uint128, StdError> {
    let response: HubBalance = querier.query_wasm_smart(
        contract,
        &crate::hub::QueryMsg::TotalChannelBalancesAt {
            timestamp: Uint64::from(timestamp),
        },
    )?;
    Ok(response.balance)
}
