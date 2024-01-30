use astroport::tokenfactory_tracker;
use cosmwasm_std::{Deps, QuerierWrapper, StdResult, Uint128};

use astroport_governance::assembly::Config;
use astroport_governance::assembly::Proposal;
use astroport_governance::builder_unlock::msg::{
    AllocationResponse, QueryMsg as BuilderUnlockQueryMsg, StateResponse,
};

use crate::state::CONFIG;

/// Calculates an address' voting power at the specified block.
///
/// * **sender** address whose voting power we calculate.
///
/// * **proposal** proposal for which we want to compute the `sender` (voter) voting power.
pub fn calc_voting_power(deps: Deps, sender: String, proposal: &Proposal) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let mut total: Uint128 = deps.querier.query_wasm_smart(
        &config.xastro_denom_tracking,
        &tokenfactory_tracker::QueryMsg::BalanceAt {
            address: sender.clone(),
            // Get voting power at the block before the proposal starts
            timestamp: Some(proposal.start_time - 1),
        },
    )?;

    let locked_amount: AllocationResponse = deps.querier.query_wasm_smart(
        config.builder_unlock_addr,
        &BuilderUnlockQueryMsg::Allocation { account: sender },
    )?;

    total += locked_amount.params.amount - locked_amount.status.astro_withdrawn;

    Ok(total)
}

/// Calculates the combined total voting power at a specified timestamp (that is relevant for a specific proposal).
/// Combined voting power includes:
/// * xASTRO total supply
/// * ASTRO tokens which still locked in the builder's unlock contract
///
/// ## Parameters
/// * **config** contract settings.
/// * **timestamp** timestamp for which we calculate the total voting power.
pub fn calc_total_voting_power_at(
    querier: QuerierWrapper,
    config: &Config,
    timestamp: u64,
) -> StdResult<Uint128> {
    let mut total: Uint128 = querier.query_wasm_smart(
        &config.xastro_denom_tracking,
        &tokenfactory_tracker::QueryMsg::TotalSupplyAt {
            timestamp: Some(timestamp),
        },
    )?;

    // Total amount of ASTRO locked in the initial builder's unlock schedule
    let builder_state: StateResponse = querier.query_wasm_smart(
        &config.builder_unlock_addr,
        &BuilderUnlockQueryMsg::State {},
    )?;

    total += builder_state.remaining_astro_tokens;

    Ok(total)
}
