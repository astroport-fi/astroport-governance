/// Seconds in one week. Constant is intended for period number calculation.
pub const WEEK: u64 = 7 * 86400; // lock period is rounded down by week

/// Seconds in 2 years which is maximum lock period.
pub const MAX_LOCK_TIME: u64 = 2 * 365 * 86400; // 2 years (104 weeks)

/// The constant describes the deadline for token checkpoint.
pub const TOKEN_CHECKPOINT_DEADLINE: u64 = 86400; // one day in seconds

/// The constant describes the maximum number of accounts for claim.
pub const MAX_LIMIT_OF_CLAIM: u64 = 10;

/// # Description
/// Calculates how many periods are withing specified time. Time should be in seconds.
pub fn get_period(time: u64) -> u64 {
    time / WEEK
}
