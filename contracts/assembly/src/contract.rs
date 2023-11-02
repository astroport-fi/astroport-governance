use cosmwasm_std::{
    attr, coin, entry_point, to_binary, wasm_execute, BankMsg, Binary, CosmosMsg, Decimal, Deps,
    DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult, Uint128, Uint64, WasmMsg,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;
use cw_utils::must_pay;
use std::str::FromStr;

use crate::astroport;
use astroport_governance::assembly::{
    helpers::validate_links, Config, ExecuteMsg, InstantiateMsg, Proposal, ProposalListResponse,
    ProposalStatus, ProposalVoteOption, ProposalVotesResponse, QueryMsg, UpdateConfig,
};

use crate::astroport::asset::addr_opt_validate;
use astroport::tokenfactory_tracker::QueryMsg as TokenFactoryTrackerQueryMsg;
use astroport_governance::assembly::ProposalVoterResponse;
use astroport_governance::builder_unlock::msg::{
    AllocationResponse, QueryMsg as BuilderUnlockQueryMsg, StateResponse,
};
use astroport_governance::utils::{
    check_contract_supports_channel, get_total_outpost_voting_power_at, WEEK,
};
use astroport_governance::voting_escrow_delegation::QueryMsg::AdjustedBalance;
use astroport_governance::voting_escrow_lite::{
    QueryMsg as VotingEscrowQueryMsg, VotingPowerResponse,
};

use crate::error::ContractError;
use crate::migration::MigrateMsg;
use crate::state::{CONFIG, PROPOSALS, PROPOSAL_COUNT, PROPOSAL_VOTERS};

use ibc_controller_package::ExecuteMsg as ControllerExecuteMsg;

// Contract name and version used for migration.
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Default pagination constants
const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 30;
const DEFAULT_VOTERS_LIMIT: u32 = 100;
const MAX_VOTERS_LIMIT: u32 = 250;

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

    // TODO: Check that the xastro_denom_tracking_address reports the tracked_denom
    // to be the same as xastro_denom

    let config = Config {
        xastro_denom: msg.xastro_denom,
        // TODO: Address?, check naming
        xastro_denom_tracking: msg.xastro_denom_tracking_address,
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
        ExecuteMsg::CheckMessages { messages } => check_messages(env, messages),
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
    messages: Option<Vec<CosmosMsg>>,
    ibc_channel: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Ensure that the correct token is sent. This will fail if
    // zero tokens are sent.
    // TODO: Remove mut
    let deposit_amount = must_pay(&info, &config.xastro_denom)?;

    if deposit_amount < config.proposal_required_deposit {
        return Err(ContractError::InsufficientDeposit {});
    }

    // Update the proposal count
    let count = PROPOSAL_COUNT.update(deps.storage, |c| -> StdResult<_> {
        Ok(c.checked_add(Uint64::new(1))?)
    })?;

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

    let mut proposal_quorum: Decimal = Decimal::zero();
    let mut proposal_threshold: Decimal = Decimal::zero();

    if !total_voting_power.is_zero() {
        proposal_quorum = Decimal::from_ratio(total_votes, total_voting_power);
    }

    if !total_votes.is_zero() {
        proposal_threshold = Decimal::from_ratio(for_votes, total_votes);
    }

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
        .add_attributes(vec![
            attr("action", "end_proposal"),
            attr("proposal_id", proposal_id.to_string()),
            attr("proposal_result", proposal.status.to_string()),
        ])
        .add_message(BankMsg::Send {
            to_address: proposal.submitter.to_string(),
            amount: vec![coin(proposal.deposit_amount.into(), config.xastro_denom)],
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

    let messages;
    if let Some(channel) = &proposal.ibc_channel {
        let config = CONFIG.load(deps.storage)?;

        messages = match &proposal.messages {
            Some(messages) => {
                if !messages.is_empty() {
                    proposal.status = ProposalStatus::InProgress;
                    vec![CosmosMsg::Wasm(wasm_execute(
                        config
                            .ibc_controller
                            .ok_or(ContractError::MissingIBCController {})?,
                        &ControllerExecuteMsg::IbcExecuteProposal {
                            channel_id: channel.to_string(),
                            proposal_id,
                            messages: messages.to_vec(),
                        },
                        vec![],
                    )?)]
                } else {
                    proposal.status = ProposalStatus::Executed;
                    vec![]
                }
            }
            None => {
                proposal.status = ProposalStatus::Executed;
                vec![]
            }
        };

        PROPOSALS.save(deps.storage, proposal_id, &proposal)?;
    } else {
        proposal.status = ProposalStatus::Executed;
        PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

        messages = proposal.messages.unwrap_or_default()
    }

    Ok(Response::new()
        .add_attribute("action", "execute_proposal")
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_messages(messages))
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
        messages: Some(messages),
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
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::CheckMessagesPassed {})?,
        funds: vec![],
    }));

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
        config.voting_escrow_delegator_addr = Some(
            deps.api
                .addr_validate(voting_escrow_delegator_addr.as_str())?,
        )
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
    let mut proposal_quorum: Decimal = Decimal::zero();
    let mut proposal_threshold: Decimal = Decimal::zero();

    if !total_voting_power.is_zero() {
        proposal_quorum = Decimal::from_ratio(total_votes, total_voting_power);
    }

    if !total_votes.is_zero() {
        proposal_threshold = Decimal::from_ratio(proposal.for_power, total_votes);
    }

    // Determine the proposal result
    proposal.status = if proposal_quorum >= config.proposal_required_quorum
        && proposal_threshold > config.proposal_required_threshold
    {
        ProposalStatus::Passed
    } else {
        ProposalStatus::Rejected
    };

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    let response = Response::new().add_attributes(vec![
        attr("action", "remove_outpost_votes"),
        attr("proposal_id", proposal_id.to_string()),
        attr("proposal_result", proposal.status.to_string()),
    ]);

    Ok(response)
}

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
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Proposals { start, limit } => to_binary(&query_proposals(deps, start, limit)?),
        QueryMsg::Proposal { proposal_id } => {
            to_binary(&PROPOSALS.load(deps.storage, proposal_id)?)
        }
        QueryMsg::ProposalVotes { proposal_id } => {
            to_binary(&query_proposal_votes(deps, proposal_id)?)
        }
        QueryMsg::UserVotingPower { user, proposal_id } => {
            let proposal = PROPOSALS.load(deps.storage, proposal_id)?;

            deps.api.addr_validate(&user)?;

            to_binary(&calc_voting_power(deps, user, &proposal)?)
        }
        QueryMsg::TotalVotingPower { proposal_id } => {
            let proposal = PROPOSALS.load(deps.storage, proposal_id)?;
            to_binary(&calc_total_voting_power_at(deps, &proposal)?)
        }
        QueryMsg::ProposalVoters {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&query_proposal_voters(
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

/// Calculates an address' voting power at the specified block.
///
/// * **sender** address whose voting power we calculate.
///
/// * **proposal** proposal for which we want to compute the `sender` (voter) voting power.
pub fn calc_voting_power(deps: Deps, sender: String, proposal: &Proposal) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    // This is the address' xASTRO balance at the previous block (proposal.start_block - 1).
    // We use the previous block because it always has an up-to-date checkpoint.
    // BalanceAt will always return the balance information in the previous block,
    // so we don't subtract one block from proposal.start_block.
    // let xastro_amount: BalanceResponse = deps.querier.query_wasm_smart(
    //     config.xastro_token_addr,
    //     &XAstroTokenQueryMsg::BalanceAt {
    //         address: sender.clone(),
    //         block: proposal.start_block,
    //     },
    // )?;

    // TODO: Comment, we query the balance tracking contract for xASTRO
    let xastro_amount: Uint128 = deps.querier.query_wasm_smart(
        config.xastro_denom_tracking,
        &TokenFactoryTrackerQueryMsg::BalanceAt {
            address: sender.clone(),
            timestamp: Some(Uint64::from(proposal.start_time)),
        },
    )?;

    let mut total = xastro_amount;

    let locked_amount: AllocationResponse = deps.querier.query_wasm_smart(
        config.builder_unlock_addr,
        &BuilderUnlockQueryMsg::Allocation {
            account: sender.clone(),
        },
    )?;

    if !locked_amount.params.amount.is_zero() {
        total = total
            .checked_add(locked_amount.params.amount)?
            .checked_sub(locked_amount.status.astro_withdrawn)?;
    }

    if let Some(vxastro_token_addr) = config.vxastro_token_addr {
        let vxastro_amount: Uint128 =
            if let Some(voting_escrow_delegator_addr) = config.voting_escrow_delegator_addr {
                deps.querier.query_wasm_smart(
                    voting_escrow_delegator_addr,
                    &AdjustedBalance {
                        account: sender.clone(),
                        timestamp: Some(proposal.start_time - WEEK),
                    },
                )?
            } else {
                // For vxASTRO lite, this will always be 0
                let res: VotingPowerResponse = deps.querier.query_wasm_smart(
                    &vxastro_token_addr,
                    &VotingEscrowQueryMsg::UserVotingPowerAt {
                        user: sender.clone(),
                        time: proposal.start_time - WEEK,
                    },
                )?;
                res.voting_power
            };

        if !vxastro_amount.is_zero() {
            total = total.checked_add(vxastro_amount)?;
        }

        let locked_xastro: Uint128 = deps.querier.query_wasm_smart(
            vxastro_token_addr,
            &VotingEscrowQueryMsg::UserDepositAt {
                user: sender,
                timestamp: Uint64::from(proposal.start_time),
            },
        )?;

        total = total.checked_add(locked_xastro)?;
    }

    Ok(total)
}

/// Calculates the total voting power at a specified block (that is relevant for a specific proposal).
///
/// * **proposal** proposal for which we calculate the total voting power.
pub fn calc_total_voting_power_at(deps: Deps, proposal: &Proposal) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    // This is the address' xASTRO balance at the previous block (proposal.start_block - 1).
    // We use the previous block because it always has an up-to-date checkpoint.
    // let mut total: Uint128 = deps.querier.query_wasm_smart(
    //     &config.xastro_token_addr,
    //     &XAstroTokenQueryMsg::TotalSupplyAt {
    //         block: proposal.start_block - 1,
    //     },
    // )?;
    let mut total: Uint128 = deps.querier.query_wasm_smart(
        config.xastro_denom_tracking,
        &TokenFactoryTrackerQueryMsg::TotalSupplyAt {
            timestamp: Some(Uint64::from(proposal.start_time)),
        },
    )?;

    // Total amount of ASTRO locked in the initial builder's unlock schedule
    let builder_state: StateResponse = deps
        .querier
        .query_wasm_smart(config.builder_unlock_addr, &BuilderUnlockQueryMsg::State {})?;

    if !builder_state.remaining_astro_tokens.is_zero() {
        total = total.checked_add(builder_state.remaining_astro_tokens)?;
    }

    if let Some(vxastro_token_addr) = config.vxastro_token_addr {
        // Total vxASTRO voting power
        // For vxASTRO lite, this will always be 0
        let vxastro: VotingPowerResponse = deps.querier.query_wasm_smart(
            vxastro_token_addr,
            &VotingEscrowQueryMsg::TotalVotingPowerAt {
                time: proposal.start_time - WEEK,
            },
        )?;
        if !vxastro.voting_power.is_zero() {
            total = total.checked_add(vxastro.voting_power)?;
        }
    }

    Ok(total)
}

/// Manages contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Err(ContractError::Std(StdError::generic_err(
        "This contract cannot be migrated.",
    )))
}
