use astroport_governance::voting_escrow::QueryMsg::{
    LockInfo, TotalVotingPower, TotalVotingPowerAt, UserVotingPower, UserVotingPowerAt,
};
use astroport_governance::voting_escrow::{LockInfoResponse, VotingPowerResponse};
use cosmwasm_std::{Addr, QuerierWrapper, StdResult, Uint128};

/// ## Description
/// Queries current user's voting power from the voting escrow contract.
pub fn get_voting_power(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &UserVotingPower {
            user: user.to_string(),
        },
    )?;
    Ok(vp.voting_power)
}

/// ## Description
/// Queries current user's voting power from the voting escrow contract by timestamp.
pub fn get_voting_power_at(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
    timestamp: u64,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &UserVotingPowerAt {
            user: user.to_string(),
            time: timestamp,
        },
    )?;

    Ok(vp.voting_power)
}

/// ## Description
/// Queries current total voting power from the voting escrow contract.
pub fn get_total_voting_power(querier: QuerierWrapper, escrow_addr: &Addr) -> StdResult<Uint128> {
    let vp: VotingPowerResponse =
        querier.query_wasm_smart(escrow_addr.clone(), &TotalVotingPower {})?;

    Ok(vp.voting_power)
}

/// ## Description
/// Queries total voting power from the voting escrow contract by timestamp.
pub fn get_total_voting_power_at(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    timestamp: u64,
) -> StdResult<Uint128> {
    let vp: VotingPowerResponse =
        querier.query_wasm_smart(escrow_addr.clone(), &TotalVotingPowerAt { time: timestamp })?;

    Ok(vp.voting_power)
}

/// ## Description
/// Queries user's lockup information from the voting escrow contract.
pub fn get_lock_info(
    querier: QuerierWrapper,
    escrow_addr: &Addr,
    user: &Addr,
) -> StdResult<LockInfoResponse> {
    let lock_info: LockInfoResponse = querier.query_wasm_smart(
        escrow_addr.clone(),
        &LockInfo {
            user: user.to_string(),
        },
    )?;
    Ok(lock_info)
}
