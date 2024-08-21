use astroport_governance::emissions_controller::consts::MAX_PAGE_LIMIT;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, Order, StdResult};
use cw_storage_plus::Bound;
use itertools::Itertools;

use astroport_governance::emissions_controller::outpost::{
    QueryMsg, RegisteredProposal, UserIbcStatus,
};

use crate::state::{
    CONFIG, PENDING_MESSAGES, PROPOSAL_VOTERS, REGISTERED_PROPOSALS, USER_IBC_ERROR,
};

/// Expose available contract queries.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::QueryUserIbcStatus { user } => to_json_binary(&UserIbcStatus {
            pending_msg: PENDING_MESSAGES.may_load(deps.storage, &user)?,
            error: USER_IBC_ERROR.may_load(deps.storage, &user)?,
        }),
        QueryMsg::QueryRegisteredProposals { limit, start_after } => REGISTERED_PROPOSALS
            .range(
                deps.storage,
                start_after.map(Bound::exclusive),
                None,
                Order::Ascending,
            )
            .take(limit.unwrap_or(MAX_PAGE_LIMIT) as usize)
            .map(|item| item.map(|(id, start_time)| RegisteredProposal { id, start_time }))
            .collect::<StdResult<Vec<_>>>()
            .and_then(|proposals| to_json_binary(&proposals)),
        QueryMsg::QueryProposalVoters {
            proposal_id,
            limit,
            start_after,
        } => PROPOSAL_VOTERS
            .prefix(proposal_id)
            .range(
                deps.storage,
                start_after.map(Bound::exclusive),
                None,
                Order::Ascending,
            )
            .take(limit.unwrap_or(MAX_PAGE_LIMIT) as usize)
            .collect::<StdResult<Vec<_>>>()
            .and_then(|voters| {
                let voters = voters.into_iter().map(|(voter, _)| voter).collect_vec();
                to_json_binary(&voters)
            }),
    }
}
