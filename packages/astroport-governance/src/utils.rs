/// Seconds in one week. Constant is intended for period number calculation.
pub const WEEK: u64 = 7 * 86400; // lock period is rounded down by week

/// Seconds in 2 years which is maximum lock period.
pub const MAX_LOCK_TIME: u64 = 2 * 365 * 86400; // 2 years (104 weeks)

/// The constant describes the maximum number of accounts for claim.
pub const CLAIM_LIMIT: u64 = 10;

/// # Description
/// Calculates how many periods are within specified time. Time should be in seconds.
pub fn get_period(time: u64) -> u64 {
    time / WEEK
}
