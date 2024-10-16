use std::ops::RangeInclusive;

use cosmwasm_std::IbcOrder;

/// vxASTRO voting epoch starts on Mon May 27 00:00:00 UTC 2024
pub const EPOCHS_START: u64 = 1716768000;
pub const DAY: u64 = 86400;
/// vxASTRO voting epoch lasts 14 days
pub const EPOCH_LENGTH: u64 = DAY * 14;
/// Timeout for IBC messages in seconds. Used for both `ics20` and `vxastro-ibc-v1` packets.
pub const IBC_TIMEOUT: u64 = 3600;
/// Denom used to pay IBC fees
pub const FEE_DENOM: &str = "untrn";
/// Max number of pools allowed per outpost added
pub const POOL_NUMBER_LIMIT: RangeInclusive<u64> = 1..=10;
/// Maximum number of pools that can be voted for
pub const MAX_POOLS_TO_VOTE: usize = 5;
/// Max items per page in queries
pub const MAX_PAGE_LIMIT: u8 = 50;
/// vxASTRO IBC version
pub const IBC_APP_VERSION: &str = "vxastro-ibc-v1";
/// IBC ordering
pub const IBC_ORDERING: IbcOrder = IbcOrder::Unordered;
