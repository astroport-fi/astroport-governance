use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Deps, Order, StdError, StdResult, Storage, Uint128};
use cw_storage_plus::{Bound, Item, Map};

use astroport::common::OwnershipProposal;
use astroport_governance::hub::Config;

use crate::error::ContractError;

/// Holds temporary data used in the staking/unstaking replies
#[cw_serde]
pub struct ReplyData {
    /// The address that should receive the staked/unstaked tokens
    pub receiver: String,
    /// The IBC channel the original request was received on
    pub receiving_channel: String,
    /// A generic value to store balances
    pub value: Uint128,
    /// The original value of a request
    pub original_value: Uint128,
}

/// Holds the IBC channels that are allowed to communicate with the Hub
#[cw_serde]
pub struct OutpostChannels {
    /// The channel of the Outpost contract on the remote chain
    pub outpost: String,
    /// The channel to send ASTRO CW20-ICS20 tokens through
    pub cw20_ics20: String,
}

/// Stores the contract config
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores data for reply endpoint.
pub const REPLY_DATA: Item<ReplyData> = Item::new("reply_data");

/// Stores funds that got stuck on the Hub chain due to IBC transfer failures
/// when using cross-chain actions
pub const USER_FUNDS: Map<&Addr, Uint128> = Map::new("user_funds");

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Contains a map of outpost addresses to their IBC channels that are allowed
/// to communicate with the Hub over IBC
pub const OUTPOSTS: Map<&str, OutpostChannels> = Map::new("channel_map");

/// Contains a map of Outpost channels to their balances at timestamps. That is, the amount
/// of xASTRO minted via an Outpost at a specific time
pub const OUTPOST_CHANNEL_BALANCES: Map<(&str, u64), Uint128> =
    Map::new("outpost_channel_balances");

pub const TOTAL_OUTPOST_CHANNEL_BALANCE: Map<u64, Uint128> =
    Map::new("total_outpost_channel_balances");

/// Get the Outpost channels for a given CW20-ICS20 channel
///
/// The Outposts must be configured and connected before this will return any values
pub fn get_outpost_from_cw20ics20_channel(
    deps: Deps,
    cw20ics20_channel: &str,
) -> Result<OutpostChannels, ContractError> {
    OUTPOSTS
        .range(deps.storage, None, None, Order::Ascending)
        .find_map(|item| {
            let (_, value) = item.ok()?;
            if value.cw20_ics20 == cw20ics20_channel {
                Some(value)
            } else {
                None
            }
        })
        .ok_or(ContractError::UnknownOutpost {})
}

/// Get the Outpost channels for a given contract channel
///
/// The Outposts must be configured and connected before this will return any values
pub fn get_transfer_channel_from_outpost_channel(
    deps: Deps,
    outpost_channel: &str,
) -> Result<OutpostChannels, ContractError> {
    OUTPOSTS
        .range(deps.storage, None, None, Order::Ascending)
        .find_map(|item| {
            let (_, value) = item.ok()?;
            if value.outpost == outpost_channel {
                Some(value)
            } else {
                None
            }
        })
        .ok_or(ContractError::UnknownOutpost {})
}

/// Increase the balance of xASTRO minted via a specific Outpost
pub(crate) fn increase_channel_balance(
    storage: &mut dyn Storage,
    timestamp: u64,
    outpost_channel: &str,
    amount: Uint128,
) -> Result<(), StdError> {
    let last_balance = channel_balance_at(storage, outpost_channel, timestamp)?;
    OUTPOST_CHANNEL_BALANCES.save(
        storage,
        (outpost_channel, timestamp),
        &last_balance.checked_add(amount)?,
    )?;

    let last_total_balance = total_balance_at(storage, timestamp)?;
    TOTAL_OUTPOST_CHANNEL_BALANCE.save(storage, timestamp, &last_total_balance.checked_add(amount)?)
}

/// Decrease the balance of xASTRO minted via a specific Outpost
/// This will return an error if the balance is insufficient
pub(crate) fn decrease_channel_balance(
    storage: &mut dyn Storage,
    timestamp: u64,
    outpost_channel: &str,
    amount: Uint128,
) -> Result<(), StdError> {
    let last_balance = channel_balance_at(storage, outpost_channel, timestamp)?;
    OUTPOST_CHANNEL_BALANCES.save(
        storage,
        (outpost_channel, timestamp),
        &last_balance.checked_sub(amount)?,
    )?;

    let last_total_balance = total_balance_at(storage, timestamp)?;
    TOTAL_OUTPOST_CHANNEL_BALANCE.save(storage, timestamp, &last_total_balance.checked_sub(amount)?)
}

/// Fetches last known balance of a channel before or on timestamp
pub(crate) fn channel_balance_at(
    storage: &dyn Storage,
    outpost_channel: &str,
    timestamp: u64,
) -> StdResult<Uint128> {
    let balance_opt = OUTPOST_CHANNEL_BALANCES
        .prefix(outpost_channel)
        .range(
            storage,
            None,
            Some(Bound::inclusive(timestamp)),
            Order::Descending,
        )
        .next()
        .transpose()?
        .map(|(_, value)| value);

    Ok(balance_opt.unwrap_or_else(Uint128::zero))
}

/// Returns the total channel balances at a specific time
pub fn total_balance_at(storage: &dyn Storage, timestamp: u64) -> StdResult<Uint128> {
    // Look for the last value recorded before the current block (if none then value is zero)
    let end = Bound::inclusive(timestamp);
    let last_value = TOTAL_OUTPOST_CHANNEL_BALANCE
        .range(storage, None, Some(end), Order::Descending)
        .next();

    if let Some(value) = last_value {
        let (_, v) = value?;
        return Ok(v);
    }

    Ok(Uint128::zero())
}
