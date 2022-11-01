use std::cmp::min;

use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, DepsMut, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use ap_voting_escrow::{get_lock_info, QueryMsg as VotingQueryMsg, VotingPowerResponse};

use crate::error::ContractError;
use crate::state::{LAST_CLAIM_PERIOD, REWARDS_PER_WEEK};

pub const DEFAULT_PERIODS_LIMIT: u64 = 20;

/// Transfer tokens to another address.
///
/// * **contract_addr** address of the token contract.
///
/// * **recipient** address of the token recipient.
///
/// * **amount** token amount to transfer.
pub(crate) fn transfer_token_amount(
    contract_addr: &Addr,
    recipient: &Addr,
    amount: Uint128,
) -> Result<Vec<CosmosMsg>, ContractError> {
    let messages = if !amount.is_zero() {
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: recipient.to_string(),
                amount,
            })?,
            funds: vec![],
        })]
    } else {
        vec![]
    };

    Ok(messages)
}

/// Returns the amount of rewards distributed to a user for a specific period.
///
/// * **period** period for which we calculate the user's reward.
///
/// * **user_vp** user's voting power for the specified period.
///
/// * **total_vp** total voting power for the specified period.
pub(crate) fn calculate_reward(
    storage: &dyn Storage,
    period: u64,
    user_vp: Uint128,
    total_vp: Uint128,
) -> StdResult<Uint128> {
    let rewards_per_week = REWARDS_PER_WEEK
        .may_load(storage, period)?
        .unwrap_or_default();

    user_vp
        .checked_multiply_ratio(rewards_per_week, total_vp)
        .map_err(|e| StdError::generic_err(format!("{:?}", e)))
}

/// Calculates the amount of ASTRO available to claim by a specific address.
///
/// * **current_period** current epoch number.
///
/// * **account** account for which we calculate the amount of ASTRO rewards available to claim.
///
/// * **voting_escrow_addr** vxASTRO contract address.
///
/// * **max_periods** maximum number of periods to claim.
pub(crate) fn calc_claim_amount(
    deps: DepsMut,
    current_period: u64,
    account: &Addr,
    voting_escrow_addr: &Addr,
    max_periods: Option<u64>,
) -> StdResult<Uint128> {
    let user_lock_info = get_lock_info(&deps.querier, voting_escrow_addr, account)?;

    let mut claim_period = LAST_CLAIM_PERIOD
        .may_load(deps.storage, account)?
        .unwrap_or(user_lock_info.start);

    let lock_end_period = user_lock_info.end;
    let mut claim_amount: Uint128 = Default::default();
    let max_period = min(
        max_periods.unwrap_or(DEFAULT_PERIODS_LIMIT) + claim_period,
        current_period,
    );

    loop {
        // User cannot claim for the current period/
        if claim_period >= max_period {
            break;
        }

        // User cannot claim after their max lock period
        if claim_period > lock_end_period {
            break;
        }

        let user_voting_power: VotingPowerResponse = deps.querier.query_wasm_smart(
            voting_escrow_addr,
            &VotingQueryMsg::UserVotingPowerAtPeriod {
                user: account.to_string(),
                period: claim_period,
            },
        )?;

        let total_voting_power: VotingPowerResponse = deps.querier.query_wasm_smart(
            voting_escrow_addr,
            &VotingQueryMsg::TotalVotingPowerAtPeriod {
                period: claim_period,
            },
        )?;

        if !user_voting_power.voting_power.is_zero() && !total_voting_power.voting_power.is_zero() {
            claim_amount = claim_amount.checked_add(calculate_reward(
                deps.storage,
                claim_period,
                user_voting_power.voting_power,
                total_voting_power.voting_power,
            )?)?;
        }

        claim_period += 1;
    }

    LAST_CLAIM_PERIOD.save(deps.storage, account, &claim_period)?;

    Ok(claim_amount)
}
