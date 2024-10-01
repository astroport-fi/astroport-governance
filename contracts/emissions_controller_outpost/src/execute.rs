use std::collections::HashMap;

use astroport::asset::determine_asset_info;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::incentives;
use astroport::incentives::{IncentivesSchedule, InputSchedule};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, coins, ensure, wasm_execute, Addr, Coin, Decimal, DepsMut, Env, IbcMsg,
    MessageInfo, Response, StdError, Uint128,
};
use cw_utils::{must_pay, nonpayable};
use itertools::Itertools;

use astroport_governance::assembly::ProposalVoteOption;
use astroport_governance::emissions_controller::consts::{IBC_TIMEOUT, MAX_POOLS_TO_VOTE};
use astroport_governance::emissions_controller::msg::ExecuteMsg;
use astroport_governance::emissions_controller::msg::VxAstroIbcMsg;
use astroport_governance::emissions_controller::outpost::{Config, OutpostMsg};
use astroport_governance::emissions_controller::utils::{
    check_lp_token, get_total_voting_power, get_voting_power,
};
use astroport_governance::utils::check_contract_supports_channel;

use crate::error::ContractError;
use crate::state::{CONFIG, OWNERSHIP_PROPOSAL, PROPOSAL_VOTERS, REGISTERED_PROPOSALS};
use crate::utils::prepare_ibc_packet;

/// Exposes all execute endpoints available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg<OutpostMsg>,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Vote { votes } => handle_vote(deps, env, info, votes),
        ExecuteMsg::UpdateUserVotes { user, is_unlock } => {
            let config = CONFIG.load(deps.storage)?;
            ensure!(
                info.sender == config.vxastro,
                ContractError::Unauthorized {}
            );
            let voting_power = get_voting_power(deps.querier, &config.vxastro, &user, None)?;
            handle_update_user(
                deps,
                env,
                Addr::unchecked(user),
                voting_power,
                is_unlock,
                config,
            )
        }
        ExecuteMsg::RefreshUserVotes {} => {
            nonpayable(&info)?;
            let config = CONFIG.load(deps.storage)?;
            let voting_power = get_voting_power(deps.querier, &config.vxastro, &info.sender, None)?;

            // Blocking updates if new_voting_power is zero.
            // Potentially reduces IBC spam attack vector
            ensure!(!voting_power.is_zero(), ContractError::ZeroVotingPower {});
            handle_update_user(deps, env, info.sender, voting_power, false, config)
        }
        ExecuteMsg::ProposeNewOwner {
            new_owner,
            expires_in,
        } => {
            nonpayable(&info)?;
            let config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                new_owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            nonpayable(&info)?;
            let config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            nonpayable(&info)?;
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.owner = new_owner;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
        ExecuteMsg::Custom(outpost_msg) => match outpost_msg {
            OutpostMsg::SetEmissions { schedules } => set_emissions(deps, env, info, schedules),
            OutpostMsg::PermissionedSetEmissions { schedules } => {
                permissioned_set_emissions(deps, env, info, schedules)
            }
            OutpostMsg::UpdateConfig {
                voting_ibc_channel,
                hub_emissions_controller,
                ics20_channel,
            } => update_config(
                deps,
                env,
                info,
                voting_ibc_channel,
                hub_emissions_controller,
                ics20_channel,
            ),
            OutpostMsg::CastVote { proposal_id, vote } => {
                governance_vote(deps, env, info, proposal_id, vote)
            }
        },
    }
}

/// Permissionless endpoint to set emissions for given pools.
/// Caller must send exact amount of ASTRO to cover all emissions.
pub fn set_emissions(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    schedules: Vec<(String, InputSchedule)>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let amount = must_pay(&info, &config.astro_denom)?;

    // Ensure we received exact amount of ASTRO
    let schedules_total: Uint128 = schedules
        .iter()
        .map(|(_, schedule)| schedule.reward.amount)
        .sum();
    ensure!(
        amount == schedules_total,
        ContractError::InvalidAstroAmount {
            expected: schedules_total,
            actual: amount
        }
    );

    let funds = coin(amount.u128(), &config.astro_denom);
    execute_emissions(deps, env, funds, config, schedules)
}

/// Permissioned endpoint to set emissions for given pools.
/// Only contract owner can call this function.
/// Caller may or may not send ASTRO to cover all emissions.
/// Contract uses whole available ASTRO balance.
pub fn permissioned_set_emissions(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    schedules: Vec<(String, InputSchedule)>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    let balance = deps
        .querier
        .query_balance(&env.contract.address, &config.astro_denom)?;

    // Ensure we have enough ASTRO in balance
    let schedules_total: Uint128 = schedules
        .iter()
        .map(|(_, schedule)| schedule.reward.amount)
        .sum();
    ensure!(
        balance.amount >= schedules_total,
        ContractError::InvalidAstroAmount {
            expected: schedules_total,
            actual: balance.amount
        }
    );

    execute_emissions(deps, env, balance, config, schedules)
}

/// Main function to set emissions for given pools.
/// Filters out not eligible pools and sends leftover funds back to the Hub.
pub fn execute_emissions(
    deps: DepsMut,
    env: Env,
    astro_balance: Coin,
    config: Config,
    schedules: Vec<(String, InputSchedule)>,
) -> Result<Response, ContractError> {
    // Filter not eligible pools and send leftover funds back to the Hub
    let mut expected_amount = 0u128;
    let schedules = schedules
        .into_iter()
        .filter(|(pool, schedule)| {
            determine_asset_info(pool, deps.api)
                .and_then(|maybe_lp| check_lp_token(deps.querier, &config.factory, &maybe_lp))
                .and_then(|_| IncentivesSchedule::from_input(&env, schedule))
                .map(|_| {
                    expected_amount += schedule.reward.amount.u128();
                })
                .is_ok()
        })
        .collect_vec();

    ensure!(!schedules.is_empty(), ContractError::NoValidSchedules {});

    let incentives_msg = wasm_execute(
        config.incentives_addr,
        &incentives::ExecuteMsg::IncentivizeMany(schedules),
        coins(expected_amount, &config.astro_denom),
    )?;

    let excess_amount = astro_balance.amount.checked_sub(expected_amount.into())?;

    let mut response = Response::default()
        .add_message(incentives_msg)
        .add_attribute("action", "set_emissions");
    if !excess_amount.is_zero() {
        // Send excess funds back to the Hub
        let ibc_transfer_msg = IbcMsg::Transfer {
            channel_id: config.ics20_channel,
            to_address: config.hub_emissions_controller,
            amount: coin(excess_amount.u128(), &config.astro_denom),
            timeout: env.block.time.plus_seconds(IBC_TIMEOUT).into(),
        };
        response = response
            .add_message(ibc_transfer_msg)
            .add_attribute("excess_amount", excess_amount);
    }

    Ok(response)
}

/// This function performs vote basic validation and sends an IBC packet to the Hub.
/// Emissions Controller on the Hub is responsible for checking whether user is eligible to vote again
/// as well as validates pools are whitelisted and correspond to a specific outpost.
pub fn handle_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    votes: Vec<(String, Decimal)>,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;

    let votes_map: HashMap<_, _> = votes.iter().cloned().collect();
    ensure!(
        votes.len() == votes_map.len(),
        ContractError::DuplicatedVotes {}
    );

    ensure!(
        votes_map.len() <= MAX_POOLS_TO_VOTE,
        ContractError::ExceededMaxPoolsToVote {}
    );

    let mut total_weight = Decimal::zero();
    for weight in votes_map.values() {
        total_weight += weight;
        ensure!(
            total_weight <= Decimal::one(),
            ContractError::InvalidTotalWeight {}
        );
    }

    let config = CONFIG.load(deps.storage)?;
    let voting_power = get_voting_power(deps.querier, &config.vxastro, &info.sender, None)?;
    ensure!(!voting_power.is_zero(), ContractError::ZeroVotingPower {});

    let total_voting_power = get_total_voting_power(deps.querier, &config.vxastro, None)?;

    let vote_ibc_msg = prepare_ibc_packet(
        deps.storage,
        &env,
        info.sender.as_str(),
        VxAstroIbcMsg::EmissionsVote {
            voter: info.sender.to_string(),
            voting_power,
            total_voting_power,
            votes: votes_map,
        },
        config.voting_ibc_channel,
    )?;

    Ok(Response::default()
        .add_attributes([("action", "vote")])
        .add_message(vote_ibc_msg))
}

/// This function sends an IBC packet to the Hub to update user's contribution to emissions voting.
/// The 'is_unlock' flag is used to force relock user in case of IBC error.
pub fn handle_update_user(
    deps: DepsMut,
    env: Env,
    voter: Addr,
    voting_power: Uint128,
    is_unlock: bool,
    config: Config,
) -> Result<Response, ContractError> {
    let attrs = vec![
        attr("action", "update_user_votes"),
        attr("voter", &voter),
        attr("new_voting_power", voting_power),
    ];

    let total_voting_power = get_total_voting_power(deps.querier, &config.vxastro, None)?;

    let ibc_msg = prepare_ibc_packet(
        deps.storage,
        &env,
        voter.as_str(),
        VxAstroIbcMsg::UpdateUserVotes {
            voter: voter.to_string(),
            voting_power,
            total_voting_power,
            is_unlock,
        },
        config.voting_ibc_channel,
    )?;

    Ok(Response::default()
        .add_attributes(attrs)
        .add_message(ibc_msg))
}

/// Only contract owner can call this function.
///
/// * voting_ibc_channel: new IBC channel to send votes to the Hub.
/// The contract must be connected to this channel.
/// * hub_emissions_controller: new address of the Hub Emissions Controller contract.
/// * ics20_channel: new ICS20 channel to send ASTRO tokens to the Hub.
fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    voting_ibc_channel: Option<String>,
    hub_emissions_controller: Option<String>,
    ics20_channel: Option<String>,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    let mut config = CONFIG.load(deps.storage)?;

    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    let mut attrs = vec![attr("action", "update_config")];

    if let Some(voting_ibc_channel) = voting_ibc_channel {
        check_contract_supports_channel(deps.querier, &env.contract.address, &voting_ibc_channel)?;
        attrs.push(attr("new_voting_ibc_channel", &voting_ibc_channel));
        config.voting_ibc_channel = voting_ibc_channel;
    }

    if let Some(hub_emissions_controller) = hub_emissions_controller {
        attrs.push(attr(
            "new_hub_emissions_controller",
            &hub_emissions_controller,
        ));
        config.hub_emissions_controller = hub_emissions_controller;
    }

    if let Some(ics20_channel) = ics20_channel {
        attrs.push(attr("new_ics20_channel", &ics20_channel));
        config.ics20_channel = ics20_channel;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default().add_attributes(attrs))
}

pub fn governance_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote: ProposalVoteOption,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let voter = info.sender.to_string();

    ensure!(
        !PROPOSAL_VOTERS.has(deps.storage, (proposal_id, voter.clone())),
        ContractError::AlreadyVoted {}
    );

    let start_time = REGISTERED_PROPOSALS.load(deps.storage, proposal_id)?;

    let voting_power =
        get_voting_power(deps.querier, &config.vxastro, &voter, Some(start_time - 1))?;
    ensure!(!voting_power.is_zero(), ContractError::ZeroVotingPower {});

    let total_voting_power =
        get_total_voting_power(deps.querier, &config.vxastro, Some(start_time - 1))?;

    let attrs = vec![
        attr("action", "governance_vote"),
        attr("voter", &info.sender),
        attr("voting_power", voting_power),
    ];

    let ibc_msg = prepare_ibc_packet(
        deps.storage,
        &env,
        &voter,
        VxAstroIbcMsg::GovernanceVote {
            voter: voter.clone(),
            voting_power,
            total_voting_power,
            proposal_id,
            vote,
        },
        config.voting_ibc_channel,
    )?;

    Ok(Response::default()
        .add_attributes(attrs)
        .add_message(ibc_msg))
}
