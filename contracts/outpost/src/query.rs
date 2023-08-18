use cosmwasm_std::{entry_point, to_binary, Addr, Binary, Deps, Env, StdResult, Uint128};

use astroport::xastro_outpost_token::get_voting_power_at_time;
use astroport_governance::outpost::QueryMsg;
use astroport_governance::voting_escrow_lite::get_user_deposit_at_time;

use crate::error::ContractError;
use crate::state::{CONFIG, VOTES};

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the config of the Outpost
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::ProposalVoted { proposal_id, user } => {
            let user_address = deps.api.addr_validate(&user)?;
            to_binary(&VOTES.load(deps.storage, (&user_address, proposal_id))?)
        }
    }
}

/// Get the user's voting power in total for xASTRO and vxASTRO
///
/// xASTRO is taken at the time the proposal was added
/// vxASTRO is taken at the current time
pub fn get_user_voting_power(
    deps: Deps,
    user: Addr,
    proposal_start: u64,
) -> Result<Uint128, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Get the user's xASTRO balance at the time the proposal was added
    let voting_power = get_voting_power_at_time(
        &deps.querier,
        config.xastro_token_addr.clone(),
        user.clone(),
        proposal_start,
    )
    .unwrap_or(Uint128::zero());

    // Get the user's underlying xASTRO deposit at the time the proposal was added
    let vxastro_balance = get_user_deposit_at_time(
        &deps.querier,
        config.vxastro_token_addr,
        user,
        proposal_start,
    )
    .unwrap_or(Uint128::zero());

    Ok(voting_power.checked_add(vxastro_balance)?)
}

#[cfg(test)]
mod tests {

    use super::*;

    use cosmwasm_std::{testing::mock_info, StdError, Uint64};

    use crate::{
        contract::instantiate,
        execute::execute,
        mock::{mock_all, setup_channel, HUB, OWNER, VXASTRO_TOKEN, XASTRO_TOKEN},
        query::query,
        state::PROPOSALS_CACHE,
    };
    use astroport_governance::{assembly::ProposalVoteOption, interchain::ProposalSnapshot};

    // Test Cases:
    //
    // Expect Success
    //      - Can query for a vote already cast
    //
    // Expect Error
    //      - Must fail if the vote doesn't exist
    //
    #[test]
    fn query_votes() {
        let (mut deps, env, info) = mock_all(OWNER);

        let proposal_id = 1u64;
        let user = "user";
        let ibc_timeout_seconds = 10u64;

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::outpost::InstantiateMsg {
                owner: OWNER.to_string(),
                xastro_token_addr: XASTRO_TOKEN.to_string(),
                vxastro_token_addr: VXASTRO_TOKEN.to_string(),
                hub_addr: HUB.to_string(),
                ibc_timeout_seconds,
            },
        )
        .unwrap();

        // Set up valid Hub
        setup_channel(deps.as_mut(), env.clone());

        // Update config with new channel
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::outpost::ExecuteMsg::UpdateConfig {
                hub_addr: None,
                hub_channel: Some("channel-3".to_string()),
                ibc_timeout_seconds: None,
            },
        )
        .unwrap();

        // Add a proposal to the cache
        PROPOSALS_CACHE
            .save(
                &mut deps.storage,
                proposal_id,
                &ProposalSnapshot {
                    id: Uint64::from(proposal_id),
                    start_time: 1689939457,
                },
            )
            .unwrap();

        // Cast a vote with a proposal in the cache
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user, &[]),
            astroport_governance::outpost::ExecuteMsg::CastAssemblyVote {
                proposal_id,
                vote: astroport_governance::assembly::ProposalVoteOption::For,
            },
        )
        .unwrap();

        // Check that we can query the vote that was cast
        let vote_data = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::outpost::QueryMsg::ProposalVoted {
                proposal_id,
                user: user.to_string(),
            },
        )
        .unwrap();

        assert_eq!(vote_data, to_binary(&ProposalVoteOption::For).unwrap());

        // Check that we receive an error when querying a vote that doesn't exist
        let err = query(
            deps.as_ref(),
            env,
            astroport_governance::outpost::QueryMsg::ProposalVoted {
                proposal_id,
                user: "other_user".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(
            err,
            StdError::NotFound {
                kind: "astroport_governance::assembly::ProposalVoteOption".to_string()
            }
        );
    }
}
