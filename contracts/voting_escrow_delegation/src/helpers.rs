use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContractError;
use astroport_governance::voting_escrow::get_lock_info;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, DepsMut, Order, QuerierWrapper, StdResult, Uint128, WasmMsg,
    WasmQuery,
};
use serde::de::DeserializeOwned;

use crate::msg::{ExecuteMsg, QueryMsg};
use crate::state::{Config, Token, DELEGATED, DELEGATION_MAX_PERCENT, DELEGATION_MIN_PERCENT};

/// CwTemplateContract is a wrapper around Addr that provides a lot of helpers
/// for working with this.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct DelegationHelper(pub Addr);

impl DelegationHelper {
    pub fn addr(&self) -> Addr {
        self.0.clone()
    }

    pub fn call<T: Into<ExecuteMsg>>(&self, msg: T) -> StdResult<CosmosMsg> {
        let msg = to_binary(&msg.into())?;
        Ok(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg,
            funds: vec![],
        }
        .into())
    }

    pub fn query<T: DeserializeOwned>(
        &self,
        querier: &QuerierWrapper,
        req: QueryMsg,
    ) -> StdResult<T> {
        let query = WasmQuery::Smart {
            contract_addr: self.addr().into(),
            msg: to_binary(&req)?,
        }
        .into();
        querier.query(&query)
    }

    pub fn calc_delegate_vp(&self, token: &Token, block_period: u64) -> StdResult<Uint128> {
        let dt = Uint128::from(block_period - token.start);
        Ok(token.bias - token.slope.checked_mul(dt)?)
    }

    pub fn calc_delegate_bias_slope(
        &self,
        balance: Uint128,
        block_period: u64,
        exp_period: u64,
        percent: Uint128,
    ) -> Result<Token, ContractError> {
        let delegated_balance = balance.multiply_ratio(percent, DELEGATION_MAX_PERCENT);
        let dt = Uint128::from(exp_period - block_period);
        let slope = delegated_balance
            .checked_div(dt)
            .map_err(|e| ContractError::Std(e.into()))?;
        let bias = slope * dt;

        Ok(Token {
            bias,
            slope,
            start: block_period,
            expire_period: exp_period,
        })
    }

    pub(crate) fn calc_total_delegated_vp(
        &self,
        deps: Deps,
        user: &Addr,
        block_period: u64,
    ) -> StdResult<Uint128> {
        let delegates = DELEGATED
            .prefix(user.clone())
            .range(deps.storage, None, None, Order::Ascending)
            .collect::<StdResult<Vec<_>>>()?;

        let mut total_delegated_vp = Uint128::zero();
        for delegate in delegates {
            if delegate.1.start <= block_period && delegate.1.expire_period >= block_period {
                total_delegated_vp += self.calc_delegate_vp(&delegate.1, block_period)?;
            }
        }

        Ok(total_delegated_vp)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn checks_parameters(
        &self,
        deps: &DepsMut,
        cfg: &Config,
        user: &Addr,
        block_period: u64,
        exp_period: u64,
        percent: Uint128,
        old_delegate: Option<&Token>,
    ) -> Result<(), ContractError> {
        let user_lock = get_lock_info(&deps.querier, &cfg.voting_escrow_addr, user)?;

        // vxASTRO delegation must be at least WEEK and no more then lock end period
        if (exp_period <= block_period) || (exp_period > user_lock.end) {
            return Err(ContractError::DelegationPeriodError {});
        }

        if percent.lt(&DELEGATION_MIN_PERCENT) || percent.gt(&DELEGATION_MAX_PERCENT) {
            return Err(ContractError::PercentageError {});
        }

        if let Some(old_token) = old_delegate {
            if exp_period <= old_token.expire_period {
                return Err(ContractError::DelegationExtendPeriodError {});
            }
        }

        Ok(())
    }

    pub fn calc_new_balance(
        &self,
        deps: &DepsMut,
        user: &Addr,
        mut balance: Uint128,
        block_period: u64,
    ) -> Result<Uint128, ContractError> {
        let total_delegated_vp = self.calc_total_delegated_vp(deps.as_ref(), user, block_period)?;

        if balance <= total_delegated_vp {
            return Err(ContractError::DelegationVotingPowerNotAllowed {});
        } else {
            balance -= total_delegated_vp;
        }

        Ok(balance)
    }

    pub fn calc_extend_balance(
        &self,
        deps: &DepsMut,
        user: &Addr,
        balance: Uint128,
        old_delegate: &Token,
        block_period: u64,
    ) -> Result<Uint128, ContractError> {
        let mut delegated_vp = self.calc_total_delegated_vp(deps.as_ref(), user, block_period)?;

        // we must subtract delegated voting power for specify token ID and reassign a new delegation
        if old_delegate.expire_period >= block_period {
            delegated_vp -= self.calc_delegate_vp(old_delegate, block_period)?;
        }

        if balance <= delegated_vp {
            return Err(ContractError::DelegationVotingPowerNotAllowed {});
        }

        Ok(balance - delegated_vp)
    }

    pub fn update_info(
        &self,
        deps: Deps,
        user: &Addr,
        mut balance: Uint128,
        block_period: u64,
    ) -> Result<Uint128, ContractError> {
        let total_delegated_vp = self.calc_total_delegated_vp(deps, user, block_period)?;

        if balance <= total_delegated_vp {
            return Err(ContractError::DelegationVotingPowerNotAllowed {});
        } else {
            balance -= total_delegated_vp;
        }

        Ok(balance)
    }
}
