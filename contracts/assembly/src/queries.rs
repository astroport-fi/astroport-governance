#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, Order, StdResult};
use cw_storage_plus::Bound;

use astroport_governance::assembly::{
    ProposalListResponse, ProposalVoterResponse, ProposalVotesResponse, QueryMsg,
};

use crate::state::{CONFIG, PROPOSALS, PROPOSAL_COUNT, PROPOSAL_VOTERS};
use crate::utils::calc_voting_power;

// Default pagination constants
const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 30;
const DEFAULT_VOTERS_LIMIT: u32 = 100;
const MAX_VOTERS_LIMIT: u32 = 250;

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns core contract settings stored in the [`Config`] structure.
///
/// * **QueryMsg::Proposals { start, limit }** Returns a [`ProposalListResponse`] according to the specified input parameters.
///
/// * **QueryMsg::Proposal { proposal_id }** Returns a [`Proposal`] according to the specified `proposal_id`.
///
/// * **QueryMsg::ProposalVotes { proposal_id }** Returns proposal vote counts that are stored in the [`ProposalVotesResponse`] structure.
///
/// * **QueryMsg::UserVotingPower { user, proposal_id }** Returns user voting power for a specific proposal.
///
/// * **QueryMsg::TotalVotingPower { proposal_id }** Returns total voting power for a specific proposal.
///
/// * **QueryMsg::ProposalVoters {
///             proposal_id,
///             vote_option,
///             start,
///             limit,
///         }** Returns a vector of proposal voters according to the specified input parameters.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Proposals { start, limit } => {
            to_json_binary(&query_proposals(deps, start, limit)?)
        }
        QueryMsg::Proposal { proposal_id } => {
            to_json_binary(&PROPOSALS.load(deps.storage, proposal_id)?)
        }
        QueryMsg::ProposalVotes { proposal_id } => {
            to_json_binary(&query_proposal_votes(deps, proposal_id)?)
        }
        QueryMsg::UserVotingPower { user, proposal_id } => {
            let proposal = PROPOSALS.load(deps.storage, proposal_id)?;
            to_json_binary(&calc_voting_power(deps, user, &proposal)?)
        }
        QueryMsg::TotalVotingPower { proposal_id } => {
            let proposal = PROPOSALS.load(deps.storage, proposal_id)?;
            to_json_binary(&proposal.total_voting_power)
        }
        QueryMsg::ProposalVoters {
            proposal_id,
            start_after,
            limit,
        } => to_json_binary(&query_proposal_voters(
            deps,
            proposal_id,
            start_after,
            limit,
        )?),
    }
}

/// Returns the current proposal list.
pub fn query_proposals(
    deps: Deps,
    start: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let proposal_count = PROPOSAL_COUNT.load(deps.storage)?;

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start.map(Bound::inclusive);

    let proposal_list = PROPOSALS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (_, v) = item?;
            Ok(v)
        })
        .collect::<StdResult<Vec<_>>>()?;

    Ok(ProposalListResponse {
        proposal_count,
        proposal_list,
    })
}

/// Returns a proposal's voters
pub fn query_proposal_voters(
    deps: Deps,
    proposal_id: u64,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<ProposalVoterResponse>> {
    let limit = limit.unwrap_or_else(|| DEFAULT_VOTERS_LIMIT.min(MAX_VOTERS_LIMIT)) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into_bytes()));

    let voters = PROPOSAL_VOTERS
        .prefix(proposal_id)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(address, vote_option)| ProposalVoterResponse {
                address,
                vote_option,
            })
        })
        .collect::<StdResult<Vec<ProposalVoterResponse>>>()?;
    Ok(voters)
}

/// Returns proposal votes stored in the [`ProposalVotesResponse`] structure.
pub fn query_proposal_votes(deps: Deps, proposal_id: u64) -> StdResult<ProposalVotesResponse> {
    let proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    Ok(ProposalVotesResponse {
        proposal_id,
        for_power: proposal.for_power,
        against_power: proposal.against_power,
    })
}
