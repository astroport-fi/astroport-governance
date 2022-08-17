use crate::state::{Config, Token, DELEGATED};
use crate::ContractError;
use astroport_governance::utils::calc_voting_power;
use astroport_governance::voting_escrow::get_lock_info;
use cosmwasm_std::{Addr, Deps, Order, QuerierWrapper, StdError, StdResult, Uint128};

pub(crate) const MAX_BPS_AMOUNT: u16 = 10000u16;
pub(crate) const MIN_BPS_AMOUNT: u16 = 1u16;

/// Adjusting voting power according to the slope by specified percentage.
pub fn calc_delegation(
    not_delegated_vp: Uint128,
    block_period: u64,
    exp_period: u64,
    bps: u16,
) -> Result<Token, ContractError> {
    let vp_to_delegate = Uint128::from(bps)
        .checked_mul(not_delegated_vp)
        .map_err(|e| ContractError::Std(e.into()))?
        / Uint128::from(MAX_BPS_AMOUNT);

    let dt = Uint128::from(exp_period - block_period);
    let slope = vp_to_delegate
        .checked_div(dt)
        .map_err(|e| ContractError::Std(e.into()))?;
    let power = slope * dt;

    if power.is_zero() {
        return Err(ContractError::NotEnoughVotingPower {});
    }

    Ok(Token {
        power,
        slope,
        start: block_period,
        expire_period: exp_period,
    })
}

/// Calculates the total delegated voting power for specified account.
pub(crate) fn calc_total_delegated_vp(
    deps: Deps,
    delegator: &Addr,
    block_period: u64,
) -> StdResult<Uint128> {
    let delegates = DELEGATED
        .prefix(delegator)
        .range(deps.storage, None, None, Order::Ascending)
        .filter_map(|pair| {
            let (_, token) = match pair {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
            if token.start <= block_period && token.expire_period > block_period {
                Some(Ok(token))
            } else {
                None
            }
        })
        .collect::<Result<Vec<Token>, StdError>>()?;

    let mut total_delegated_vp = Uint128::zero();
    for delegate in delegates {
        total_delegated_vp +=
            calc_voting_power(delegate.slope, delegate.power, delegate.start, block_period);
    }

    Ok(total_delegated_vp)
}

/// Validates input parameters to create or extend a delegation.
pub fn validate_parameters(
    querier: &QuerierWrapper,
    cfg: &Config,
    delegator: &Addr,
    block_period: u64,
    exp_period: u64,
    bps: u16,
    old_delegate: Option<&Token>,
) -> Result<(), ContractError> {
    let user_lock = get_lock_info(querier, &cfg.voting_escrow_addr, delegator)?;

    // vxASTRO delegation must be at least WEEK and no more then lock end period
    if (exp_period <= block_period) || (exp_period > user_lock.end) {
        return Err(ContractError::DelegationPeriodError {});
    }

    if !(MIN_BPS_AMOUNT..=MAX_BPS_AMOUNT).contains(&bps) {
        return Err(ContractError::BPSConversionError(bps));
    }

    if let Some(old_token) = old_delegate {
        if exp_period <= old_token.expire_period {
            return Err(ContractError::DelegationExtendPeriodError {});
        }
    }

    Ok(())
}

/// Calculates available balance for a new delegation.
pub fn calc_not_delegated_vp(
    deps: Deps,
    delegator: &Addr,
    vp: Uint128,
    block_period: u64,
) -> Result<Uint128, ContractError> {
    let total_delegated_vp = calc_total_delegated_vp(deps, delegator, block_period)?;

    if vp <= total_delegated_vp {
        return Err(ContractError::AllVotingPowerIsDelegated {});
    }

    Ok(vp - total_delegated_vp)
}

/// Calculates the available balance for the specified delegation.
pub fn calc_extend_delegation(
    deps: Deps,
    delegator: &Addr,
    vp: Uint128,
    old_delegation: &Token,
    block_period: u64,
    exp_period: u64,
    bps: u16,
) -> Result<Token, ContractError> {
    let not_delegated_vp = calc_not_delegated_vp(deps, delegator, vp, block_period)?;

    // we should deduct the previous delegation balance and assign a new delegation data
    let new_delegation = if old_delegation.expire_period > block_period {
        let old_delegation_vp = calc_voting_power(
            old_delegation.slope,
            old_delegation.power,
            old_delegation.start,
            block_period,
        );

        let new_delegation = calc_delegation(
            not_delegated_vp + old_delegation_vp,
            block_period,
            exp_period,
            bps,
        )?;

        let new_delegation_vp = calc_voting_power(
            new_delegation.slope,
            new_delegation.power,
            new_delegation.start,
            block_period,
        );

        if old_delegation_vp > new_delegation_vp {
            return Err(ContractError::DecreasedDelegatedVotingPower {});
        }

        new_delegation
    } else {
        calc_delegation(not_delegated_vp, block_period, exp_period, bps)?
    };

    Ok(new_delegation)
}
