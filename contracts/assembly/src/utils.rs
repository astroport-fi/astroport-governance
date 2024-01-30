use astroport::tokenfactory_tracker;
use astroport_governance::assembly::Config;
use cosmwasm_std::{Deps, QuerierWrapper, StdResult, Uint128, Uint64};

use astroport_governance::assembly::Proposal;
use astroport_governance::builder_unlock::msg::{
    AllocationResponse, QueryMsg as BuilderUnlockQueryMsg, StateResponse,
};
use astroport_governance::utils::WEEK;
use astroport_governance::voting_escrow_delegation::QueryMsg::AdjustedBalance;
use astroport_governance::voting_escrow_lite::{
    QueryMsg as VotingEscrowQueryMsg, VotingPowerResponse,
};

use crate::state::CONFIG;

/// Calculates an address' voting power at the specified block.
///
/// * **sender** address whose voting power we calculate.
///
/// * **proposal** proposal for which we want to compute the `sender` (voter) voting power.
pub fn calc_voting_power(deps: Deps, sender: String, proposal: &Proposal) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let xastro_amount: Uint128 = deps.querier.query_wasm_smart(
        &config.xastro_denom_tracking,
        &tokenfactory_tracker::QueryMsg::BalanceAt {
            address: sender.clone(),
            // Get voting power at the block before the proposal starts
            timestamp: Some(proposal.start_time - 1),
        },
    )?;

    let mut total = xastro_amount;

    let locked_amount: AllocationResponse = deps.querier.query_wasm_smart(
        config.builder_unlock_addr,
        &BuilderUnlockQueryMsg::Allocation {
            account: sender.clone(),
        },
    )?;

    total += locked_amount.params.amount - locked_amount.status.astro_withdrawn;

    if let Some(vxastro_token_addr) = config.vxastro_token_addr {
        let vxastro_amount =
            if let Some(voting_escrow_delegator_addr) = config.voting_escrow_delegator_addr {
                deps.querier.query_wasm_smart::<Uint128>(
                    voting_escrow_delegator_addr,
                    &AdjustedBalance {
                        account: sender.clone(),
                        // TODO: why minus WEEK?
                        timestamp: Some(proposal.start_time - WEEK),
                    },
                )?
            } else {
                // TODO: why?
                // For vxASTRO lite, this will always be 0
                let res: VotingPowerResponse = deps.querier.query_wasm_smart(
                    &vxastro_token_addr,
                    &VotingEscrowQueryMsg::UserVotingPowerAt {
                        user: sender.clone(),
                        // TODO: why minus WEEK?
                        time: proposal.start_time - WEEK,
                    },
                )?;
                res.voting_power
            };

        total += vxastro_amount;

        let locked_xastro: Uint128 = deps.querier.query_wasm_smart(
            vxastro_token_addr,
            &VotingEscrowQueryMsg::UserDepositAt {
                user: sender,
                timestamp: Uint64::from(proposal.start_time),
            },
        )?;

        total += locked_xastro;
    }

    Ok(total)
}

/// Calculates the combined total voting power at a specified timestamp (that is relevant for a specific proposal).
/// Combined voting power includes:
/// * xASTRO total supply
/// * ASTRO tokens which still locked in the builder's unlock contract
/// * vxASTRO total supply
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

    // TODO: remove it since it is always 0?
    if let Some(vxastro_token_addr) = &config.vxastro_token_addr {
        // Total vxASTRO voting power
        // For vxASTRO lite, this will always be 0
        let vxastro: VotingPowerResponse = querier.query_wasm_smart(
            vxastro_token_addr,
            &VotingEscrowQueryMsg::TotalVotingPowerAt {
                time: timestamp - WEEK,
            },
        )?;

        total += vxastro.voting_power;
    }

    Ok(total)
}
