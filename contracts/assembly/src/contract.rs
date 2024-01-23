use std::str::FromStr;

use astroport::asset::addr_opt_validate;
use astroport::staking;
use cosmwasm_std::{
    attr, coins, entry_point, wasm_execute, BankMsg, CosmosMsg, Decimal, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, SubMsg, Uint128, Uint64,
};
use cw2::set_contract_version;
use cw_utils::must_pay;
use ibc_controller_package::ExecuteMsg as ControllerExecuteMsg;

use astroport_governance::assembly::{
    helpers::validate_links, Config, ExecuteMsg, InstantiateMsg, Proposal, ProposalStatus,
    ProposalVoteOption, UpdateConfig,
};
use astroport_governance::utils::{
    check_contract_supports_channel, get_total_outpost_voting_power_at,
};

use crate::error::ContractError;
use crate::state::{CONFIG, PROPOSALS, PROPOSAL_COUNT, PROPOSAL_VOTERS};
use crate::utils::{calc_total_voting_power_at, calc_voting_power};

// Contract name and version used for migration.
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters in the `msg` variable.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    if msg.whitelisted_links.is_empty() {
        return Err(ContractError::WhitelistEmpty {});
    }

    validate_links(&msg.whitelisted_links)?;

    let staking_config = deps
        .querier
        .query_wasm_smart::<staking::Config>(&msg.staking_addr, &staking::QueryMsg::Config {})?;

    let tracker_config = deps.querier.query_wasm_smart::<staking::TrackerData>(
        &msg.staking_addr,
        &staking::QueryMsg::TrackerConfig {},
    )?;

    let config = Config {
        xastro_denom: staking_config.xastro_denom,
        xastro_denom_tracking: tracker_config.tracker_addr,
        vxastro_token_addr: addr_opt_validate(deps.api, &msg.vxastro_token_addr)?,
        voting_escrow_delegator_addr: addr_opt_validate(
            deps.api,
            &msg.voting_escrow_delegator_addr,
        )?,
        ibc_controller: addr_opt_validate(deps.api, &msg.ibc_controller)?,
        generator_controller: addr_opt_validate(deps.api, &msg.generator_controller_addr)?,
        hub: addr_opt_validate(deps.api, &msg.hub_addr)?,
        builder_unlock_addr: deps.api.addr_validate(&msg.builder_unlock_addr)?,
        proposal_voting_period: msg.proposal_voting_period,
        proposal_effective_delay: msg.proposal_effective_delay,
        proposal_expiration_period: msg.proposal_expiration_period,
        proposal_required_deposit: msg.proposal_required_deposit,
        proposal_required_quorum: Decimal::from_str(&msg.proposal_required_quorum)?,
        proposal_required_threshold: Decimal::from_str(&msg.proposal_required_threshold)?,
        whitelisted_links: msg.whitelisted_links,
        // Guardian is set to None so that Assembly must explicitly allow it
        guardian_addr: None,
    };

    #[cfg(not(feature = "testnet"))]
    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    PROPOSAL_COUNT.save(deps.storage, &Uint64::zero())?;

    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::Receive(cw20_msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::CastVote { proposal_id, vote }** Cast a vote on a specific proposal.
///
/// * **ExecuteMsg::CastOutpostVote { proposal_id, voter, vote, voting_power }** Cast a vote on a specific proposal from an Outpost.
///
/// * **ExecuteMsg::EndProposal { proposal_id }** Sets the status of an expired/finalized proposal.
///
/// * **ExecuteMsg::ExecuteProposal { proposal_id }** Executes a successful proposal.
///
/// * **ExecuteMsg::ExecuteEmissionsProposal { title, description, link, messages, ibc_channel }** Loads and executes an
/// emissions proposal from the generator controller
///
/// * **ExecuteMsg::RemoveCompletedProposal { proposal_id }** Removes a finalized proposal from the proposal list.
///
/// * **ExecuteMsg::UpdateConfig(config)** Updates the contract configuration.
///
/// * **ExecuteMsg::CancelOutpostVotes(proposal_id)** Removes all votes cast from all Outposts on a specific proposal
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::SubmitProposal {
            title,
            description,
            link,
            messages,
            ibc_channel,
        } => submit_proposal(
            deps,
            env,
            info,
            title,
            description,
            link,
            messages,
            ibc_channel,
        ),
        ExecuteMsg::CastVote { proposal_id, vote } => cast_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::CastOutpostVote {
            proposal_id,
            voter,
            vote,
            voting_power,
        } => cast_outpost_vote(deps, env, info, proposal_id, voter, vote, voting_power),
        ExecuteMsg::EndProposal { proposal_id } => end_proposal(deps, env, proposal_id),
        ExecuteMsg::ExecuteProposal { proposal_id } => execute_proposal(deps, env, proposal_id),
        ExecuteMsg::ExecuteEmissionsProposal {
            title,
            description,
            messages,
            ibc_channel,
        } => submit_execute_emissions_proposal(
            deps,
            env,
            info,
            title,
            description,
            messages,
            ibc_channel,
        ),
        ExecuteMsg::CheckMessages(messages) => check_messages(env, messages),
        ExecuteMsg::CheckMessagesPassed {} => Err(ContractError::MessagesCheckPassed {}),
        ExecuteMsg::RemoveCompletedProposal { proposal_id } => {
            remove_completed_proposal(deps, env, proposal_id)
        }
        ExecuteMsg::UpdateConfig(config) => update_config(deps, env, info, config),
        ExecuteMsg::IBCProposalCompleted {
            proposal_id,
            status,
        } => update_ibc_proposal_status(deps, info, proposal_id, status),
        ExecuteMsg::RemoveOutpostVotes { proposal_id } => {
            remove_outpost_votes(deps, env, info, proposal_id)
        }
    }
}

/// Submit a brand new proposal and locks some xASTRO as an anti-spam mechanism.
///
/// * **sender** proposal submitter.
///
/// * **deposit_amount**  amount of xASTRO to deposit in order to submit the proposal.
///
/// * **title** proposal title.
///
/// * **description** proposal description.
///
/// * **link** proposal link.
///
/// * **messages** executable messages (actions to perform if the proposal passes).
#[allow(clippy::too_many_arguments)]
pub fn submit_proposal(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    link: Option<String>,
    messages: Vec<CosmosMsg>,
    ibc_channel: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Ensure that the correct token is sent. This will fail if
    // zero tokens are sent.
    let deposit_amount = must_pay(&info, &config.xastro_denom)?;

    if deposit_amount < config.proposal_required_deposit {
        return Err(ContractError::InsufficientDeposit {});
    }

    // Update the proposal count
    let count = PROPOSAL_COUNT.update::<_, StdError>(deps.storage, |c| Ok(c + Uint64::one()))?;

    // Check that controller exists and it supports this channel
    if let Some(ibc_channel) = &ibc_channel {
        if let Some(ibc_controller) = &config.ibc_controller {
            check_contract_supports_channel(deps.querier, ibc_controller, ibc_channel)?;
        } else {
            return Err(ContractError::MissingIBCController {});
        }
    }

    let proposal = Proposal {
        proposal_id: count,
        submitter: info.sender.clone(),
        status: ProposalStatus::Active,
        for_power: Uint128::zero(),
        outpost_for_power: Uint128::zero(),
        against_power: Uint128::zero(),
        outpost_against_power: Uint128::zero(),
        start_block: env.block.height,
        start_time: env.block.time.seconds(),
        end_block: env.block.height + config.proposal_voting_period,
        delayed_end_block: env.block.height
            + config.proposal_voting_period
            + config.proposal_effective_delay,
        expiration_block: env.block.height
            + config.proposal_voting_period
            + config.proposal_effective_delay
            + config.proposal_expiration_period,
        title,
        description,
        link,
        messages,
        deposit_amount,
        ibc_channel,
    };

    proposal.validate(config.whitelisted_links)?;

    PROPOSALS.save(deps.storage, count.u64(), &proposal)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "submit_proposal"),
        attr("submitter", info.sender),
        attr("proposal_id", count),
        attr(
            "proposal_end_height",
            (env.block.height + config.proposal_voting_period).to_string(),
        ),
    ]))
}

/// Cast a vote on a proposal.
///
/// * **proposal_id** is the identifier of the proposal.
///
/// * **vote_option** contains the vote option.
pub fn cast_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote_option: ProposalVoteOption,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    if proposal.status != ProposalStatus::Active {
        return Err(ContractError::ProposalNotActive {});
    }

    // TODO: remove this restriction?
    if proposal.submitter == info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if env.block.height > proposal.end_block {
        return Err(ContractError::VotingPeriodEnded {});
    }

    if PROPOSAL_VOTERS.has(deps.storage, (proposal_id, info.sender.to_string())) {
        return Err(ContractError::UserAlreadyVoted {});
    }

    let voting_power = calc_voting_power(deps.as_ref(), info.sender.to_string(), &proposal)?;

    if voting_power.is_zero() {
        return Err(ContractError::NoVotingPower {});
    }

    match vote_option {
        ProposalVoteOption::For => {
            proposal.for_power = proposal.for_power.checked_add(voting_power)?;
        }
        ProposalVoteOption::Against => {
            proposal.against_power = proposal.against_power.checked_add(voting_power)?;
        }
    };
    PROPOSAL_VOTERS.save(
        deps.storage,
        (proposal_id, info.sender.to_string()),
        &vote_option,
    )?;

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "cast_vote"),
        attr("proposal_id", proposal_id.to_string()),
        attr("voter", &info.sender),
        attr("vote", vote_option.to_string()),
        attr("voting_power", voting_power),
    ]))
}

/// Cast a vote on a proposal from an Outpost.
/// This is a special case of `cast_vote` that allows Outposts to forward votes on
/// behalf of their users. The Hub contract is the only one allowed to call this method.
///
/// * **proposal_id** is the identifier of the proposal.
///
/// * **voter** is the address of the voter on the Outpost.
///
/// * **vote_option** contains the vote option.
///
/// * **voting_power** contains the voting power applied to this vote.
pub fn cast_outpost_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    voter: String,
    vote_option: ProposalVoteOption,
    voting_power: Uint128,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    let config = CONFIG.load(deps.storage)?;

    // We only allow the Hub to submit votes on behalf of Outpost user
    // The Hub is responsible for validating the Hub vote with the Outpost
    let hub = match config.hub {
        Some(hub) => hub,
        None => return Err(ContractError::InvalidHub {}),
    };

    if info.sender != hub {
        return Err(ContractError::Unauthorized {});
    }

    if proposal.status != ProposalStatus::Active {
        return Err(ContractError::ProposalNotActive {});
    }

    // TODO: Remove this restriction?
    if proposal.submitter == voter {
        return Err(ContractError::Unauthorized {});
    }

    if env.block.height > proposal.end_block {
        return Err(ContractError::VotingPeriodEnded {});
    }

    if PROPOSAL_VOTERS.has(deps.storage, (proposal_id, voter.clone())) {
        return Err(ContractError::UserAlreadyVoted {});
    }

    if voting_power.is_zero() {
        return Err(ContractError::NoVotingPower {});
    }

    // Voting power provided is used as is from the Hub. Validation of the voting
    // power is done by the Hub contract with the Outpost.
    // We track voting power from Outposts separately as well so as to have a
    // way to cancel votes should a vulnerability be found in IBC or the Hub/Outpost
    // implementation
    match vote_option {
        ProposalVoteOption::For => {
            proposal.for_power = proposal.for_power.checked_add(voting_power)?;
            proposal.outpost_for_power = proposal.outpost_for_power.checked_add(voting_power)?;
        }
        ProposalVoteOption::Against => {
            proposal.against_power = proposal.against_power.checked_add(voting_power)?;
            proposal.outpost_against_power =
                proposal.outpost_against_power.checked_add(voting_power)?;
        }
    };
    PROPOSAL_VOTERS.save(deps.storage, (proposal_id, voter.clone()), &vote_option)?;

    // Assert that the total amount of power from Outposts is not greater than the
    // total amount of power that was available at the time of proposal creation
    let current_outpost_power = proposal
        .outpost_for_power
        .checked_add(proposal.outpost_against_power)?;
    let max_outpost_power =
        get_total_outpost_voting_power_at(deps.querier, &hub, proposal.start_time)?;
    if current_outpost_power > max_outpost_power {
        return Err(ContractError::InvalidVotingPower {});
    }

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "cast_outpost_vote"),
        attr("proposal_id", proposal_id.to_string()),
        attr("voter", &voter),
        attr("vote", vote_option.to_string()),
        attr("voting_power", voting_power),
    ]))
}

/// Ends proposal voting period, sets the proposal status by id and returns
/// xASTRO submitted for the proposal.
pub fn end_proposal(deps: DepsMut, env: Env, proposal_id: u64) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    if proposal.status != ProposalStatus::Active {
        return Err(ContractError::ProposalNotActive {});
    }

    if env.block.height <= proposal.end_block {
        return Err(ContractError::VotingPeriodNotEnded {});
    }

    let config = CONFIG.load(deps.storage)?;

    let for_votes = proposal.for_power;
    let against_votes = proposal.against_power;
    let total_votes = for_votes + against_votes;

    let total_voting_power = calc_total_voting_power_at(deps.as_ref(), &proposal)?;

    let proposal_quorum =
        Decimal::checked_from_ratio(total_votes, total_voting_power).unwrap_or_default();
    let proposal_threshold =
        Decimal::checked_from_ratio(for_votes, total_votes).unwrap_or_default();

    // Determine the proposal result
    proposal.status = if proposal_quorum >= config.proposal_required_quorum
        && proposal_threshold > config.proposal_required_threshold
    {
        ProposalStatus::Passed
    } else {
        ProposalStatus::Rejected
    };

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    let response = Response::new()
        .add_attributes([
            attr("action", "end_proposal"),
            attr("proposal_id", proposal_id.to_string()),
            attr("proposal_result", proposal.status.to_string()),
        ])
        .add_message(BankMsg::Send {
            to_address: proposal.submitter.to_string(),
            amount: coins(proposal.deposit_amount.into(), config.xastro_denom),
        });

    Ok(response)
}

/// Executes a successful proposal by id.
pub fn execute_proposal(
    deps: DepsMut,
    env: Env,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    if proposal.status != ProposalStatus::Passed {
        return Err(ContractError::ProposalNotPassed {});
    }

    if env.block.height < proposal.delayed_end_block {
        return Err(ContractError::ProposalDelayNotEnded {});
    }

    if env.block.height > proposal.expiration_block {
        return Err(ContractError::ExecuteProposalExpired {});
    }

    let mut response = Response::new().add_attributes([
        attr("action", "execute_proposal"),
        attr("proposal_id", proposal_id.to_string()),
    ]);

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    if let Some(channel) = &proposal.ibc_channel {
        if !proposal.messages.is_empty() {
            let config = CONFIG.load(deps.storage)?;

            proposal.status = ProposalStatus::InProgress;
            response.messages.push(SubMsg::new(wasm_execute(
                config
                    .ibc_controller
                    .ok_or(ContractError::MissingIBCController {})?,
                &ControllerExecuteMsg::IbcExecuteProposal {
                    channel_id: channel.to_string(),
                    proposal_id,
                    messages: proposal.messages,
                },
                vec![],
            )?))
        } else {
            proposal.status = ProposalStatus::Executed;
        }
    } else {
        proposal.status = ProposalStatus::Executed;
        response
            .messages
            .extend(proposal.messages.into_iter().map(SubMsg::new))
    }

    Ok(response)
}

/// Load and execute a special emissions proposal. This proposal is passed
/// immediately and is not subject to voting as it is coming from the
/// generator controller based on emission votes.
#[allow(clippy::too_many_arguments)]
pub fn submit_execute_emissions_proposal(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    messages: Vec<CosmosMsg>,
    ibc_channel: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Verify that only the generator controller has been set
    let generator_controller = match config.generator_controller {
        Some(config_generator_controller) => config_generator_controller,
        None => return Err(ContractError::InvalidGeneratorController {}),
    };

    // Only the generator controller may create these proposals. These proposals
    // are typically for setting alloc points on Outposts
    if info.sender != generator_controller {
        return Err(ContractError::Unauthorized {});
    }

    // Ensure that we have messages to execute
    if messages.is_empty() {
        return Err(ContractError::InvalidProposalMessages {});
    }

    // Check that controller exists and it supports this channel
    if let Some(ibc_channel) = &ibc_channel {
        if let Some(ibc_controller) = &config.ibc_controller {
            check_contract_supports_channel(deps.querier, ibc_controller, ibc_channel)?;
        } else {
            return Err(ContractError::MissingIBCController {});
        }
    }

    // Update the proposal count
    let count = PROPOSAL_COUNT.update(deps.storage, |c| -> StdResult<_> {
        Ok(c.checked_add(Uint64::new(1))?)
    })?;

    let proposal = Proposal {
        proposal_id: count,
        submitter: info.sender,
        status: ProposalStatus::Passed,
        for_power: Uint128::zero(),
        outpost_for_power: Uint128::zero(),
        against_power: Uint128::zero(),
        outpost_against_power: Uint128::zero(),
        start_block: env.block.height,
        start_time: env.block.time.seconds(),
        end_block: env.block.height,
        delayed_end_block: env.block.height,
        expiration_block: env.block.height + config.proposal_expiration_period,
        title,
        description,
        link: None,
        messages,
        deposit_amount: Uint128::zero(),
        ibc_channel,
    };
    PROPOSAL_VOTERS.save(
        deps.storage,
        (proposal.proposal_id.u64(), generator_controller.to_string()),
        &ProposalVoteOption::For,
    )?;

    proposal.validate(config.whitelisted_links)?;

    PROPOSALS.save(deps.storage, count.u64(), &proposal)?;

    execute_proposal(deps, env, proposal.proposal_id.u64())
}

/// Checks that proposal messages are correct.
pub fn check_messages(env: Env, mut messages: Vec<CosmosMsg>) -> Result<Response, ContractError> {
    messages.push(
        wasm_execute(
            &env.contract.address,
            &ExecuteMsg::CheckMessagesPassed {},
            vec![],
        )?
        .into(),
    );

    Ok(Response::new()
        .add_attribute("action", "check_messages")
        .add_messages(messages))
}

/// Removes an expired or rejected proposal from the general proposal list.
pub fn remove_completed_proposal(
    deps: DepsMut,
    env: Env,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    if env.block.height
        > (proposal.end_block + config.proposal_effective_delay + config.proposal_expiration_period)
    {
        proposal.status = ProposalStatus::Expired;
    }

    if proposal.status != ProposalStatus::Expired && proposal.status != ProposalStatus::Rejected {
        return Err(ContractError::ProposalNotCompleted {});
    }

    PROPOSALS.remove(deps.storage, proposal_id);

    Ok(Response::new()
        .add_attribute("action", "remove_completed_proposal")
        .add_attribute("proposal_id", proposal_id.to_string()))
}

/// Updates Assembly contract parameters.
///
/// * **updated_config** new contract configuration.
pub fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    updated_config: Box<UpdateConfig>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Only the Assembly is allowed to update its own parameters (through a successful proposal)
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(xastro_denom) = updated_config.xastro_denom {
        config.xastro_denom = xastro_denom;
    }

    if let Some(vxastro_token_addr) = updated_config.vxastro_token_addr {
        config.vxastro_token_addr = Some(deps.api.addr_validate(&vxastro_token_addr)?);
    }

    if let Some(voting_escrow_delegator_addr) = updated_config.voting_escrow_delegator_addr {
        config.voting_escrow_delegator_addr =
            Some(deps.api.addr_validate(&voting_escrow_delegator_addr)?)
    }

    if let Some(ibc_controller) = updated_config.ibc_controller {
        config.ibc_controller = Some(deps.api.addr_validate(&ibc_controller)?)
    }

    if let Some(generator_controller) = updated_config.generator_controller {
        config.generator_controller = Some(deps.api.addr_validate(&generator_controller)?)
    }

    if let Some(hub) = updated_config.hub {
        config.hub = Some(deps.api.addr_validate(&hub)?)
    }

    if let Some(builder_unlock_addr) = updated_config.builder_unlock_addr {
        config.builder_unlock_addr = deps.api.addr_validate(&builder_unlock_addr)?;
    }

    if let Some(proposal_voting_period) = updated_config.proposal_voting_period {
        config.proposal_voting_period = proposal_voting_period;
    }

    if let Some(proposal_effective_delay) = updated_config.proposal_effective_delay {
        config.proposal_effective_delay = proposal_effective_delay;
    }

    if let Some(proposal_expiration_period) = updated_config.proposal_expiration_period {
        config.proposal_expiration_period = proposal_expiration_period;
    }

    if let Some(proposal_required_deposit) = updated_config.proposal_required_deposit {
        config.proposal_required_deposit = Uint128::from(proposal_required_deposit);
    }

    if let Some(proposal_required_quorum) = updated_config.proposal_required_quorum {
        config.proposal_required_quorum = Decimal::from_str(&proposal_required_quorum)?;
    }

    if let Some(proposal_required_threshold) = updated_config.proposal_required_threshold {
        config.proposal_required_threshold = Decimal::from_str(&proposal_required_threshold)?;
    }

    if let Some(whitelist_add) = updated_config.whitelist_add {
        validate_links(&whitelist_add)?;

        config.whitelisted_links.append(
            &mut whitelist_add
                .into_iter()
                .filter(|link| !config.whitelisted_links.contains(link))
                .collect(),
        );
    }

    if let Some(whitelist_remove) = updated_config.whitelist_remove {
        config
            .whitelisted_links
            .retain(|link| !whitelist_remove.contains(link));

        if config.whitelisted_links.is_empty() {
            return Err(ContractError::WhitelistEmpty {});
        }
    }

    if let Some(guardian_addr) = updated_config.guardian_addr {
        config.guardian_addr = Some(deps.api.addr_validate(&guardian_addr)?);
    }

    #[cfg(not(feature = "testnet"))]
    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

/// Updates proposal status InProgress -> Executed or Failed. Intended to be called in the end of
/// the ibc execution cycle via ibc-controller. Only ibc controller is able to call this function.
///
/// * **id** proposal's id,
///
/// * **status** a new proposal status reported by ibc controller.
fn update_ibc_proposal_status(
    deps: DepsMut,
    info: MessageInfo,
    id: u64,
    new_status: ProposalStatus,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if Some(info.sender) == config.ibc_controller {
        let mut proposal = PROPOSALS.load(deps.storage, id)?;

        if proposal.status != ProposalStatus::InProgress {
            return Err(ContractError::WrongIbcProposalStatus(
                proposal.status.to_string(),
            ));
        }

        match new_status {
            ProposalStatus::Executed {} | ProposalStatus::Failed {} => {
                proposal.status = new_status;
                PROPOSALS.save(deps.storage, id, &proposal)?;
                Ok(Response::new().add_attribute("action", "ibc_proposal_completed"))
            }
            _ => Err(ContractError::InvalidRemoteIbcProposalStatus(
                new_status.to_string(),
            )),
        }
    } else {
        Err(ContractError::InvalidIBCController {})
    }
}

/// Remove all votes cast from all Outposts in case of a vulnerability
/// in IBC or the contracts that allow manipulation of governance. This is the
/// last line of defence against a malicious actor.
///
/// This can only be called by the guardian.
fn remove_outpost_votes(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    let config = CONFIG.load(deps.storage)?;

    // Only the guardian may execute this
    if Some(info.sender) != config.guardian_addr {
        return Err(ContractError::Unauthorized {});
    }

    // EndProposal must be called first to return xASTRO to the proposer
    if proposal.status == ProposalStatus::Active {
        return Err(ContractError::ProposalNotInDelayPeriod {});
    }

    // This may only be called during the "delay" period for a proposal. That is,
    // the config.proposal_effective_delay blocks between when the voting period
    // ends and the proposal can be executed. If we allow the removal of votes during
    // the voting period, we can end up in a battle with the attacker where we
    // remove the votes and they exploit and vote again.
    if env.block.height <= proposal.end_block || env.block.height > proposal.delayed_end_block {
        return Err(ContractError::ProposalNotInDelayPeriod {});
    }

    // Remove the voting power from Outposts
    let new_for_power = proposal
        .for_power
        .saturating_sub(proposal.outpost_for_power);
    let new_against_power = proposal
        .against_power
        .saturating_sub(proposal.outpost_against_power);

    proposal.for_power = new_for_power;
    proposal.against_power = new_against_power;

    // Zero out the Outpost voting power after removal
    proposal.outpost_for_power = Uint128::zero();
    proposal.outpost_against_power = Uint128::zero();

    let total_votes = proposal.for_power.saturating_add(proposal.against_power);
    let total_voting_power = calc_total_voting_power_at(deps.as_ref(), &proposal)?;

    // Recalculate proposal state
    let proposal_quorum =
        Decimal::checked_from_ratio(total_votes, total_voting_power).unwrap_or_default();
    let proposal_threshold =
        Decimal::checked_from_ratio(proposal.for_power, total_votes).unwrap_or_default();

    // Determine the proposal result
    proposal.status = if proposal_quorum >= config.proposal_required_quorum
        && proposal_threshold > config.proposal_required_threshold
    {
        ProposalStatus::Passed
    } else {
        ProposalStatus::Rejected
    };

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    let response = Response::new().add_attributes([
        attr("action", "remove_outpost_votes"),
        attr("proposal_id", proposal_id.to_string()),
        attr("proposal_result", proposal.status.to_string()),
    ]);

    Ok(response)
}
