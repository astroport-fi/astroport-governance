use std::str::FromStr;

use astroport::asset::addr_opt_validate;
use astroport::staking;
use cosmwasm_std::{
    attr, coins, wasm_execute, BankMsg, CosmosMsg, Decimal, DepsMut, Env, MessageInfo, Response,
    StdError, SubMsg, Uint128, Uint64,
};
use cw2::set_contract_version;
use cw_utils::must_pay;
use ibc_controller_package::ExecuteMsg as ControllerExecuteMsg;

use astroport_governance::assembly::{
    helpers::validate_links, Config, ExecuteMsg, InstantiateMsg, Proposal, ProposalStatus,
    ProposalVoteOption, UpdateConfig,
};
use astroport_governance::utils::check_contract_supports_channel;

use crate::error::ContractError;
use crate::state::{CONFIG, PROPOSALS, PROPOSAL_COUNT, PROPOSAL_VOTERS};
use crate::utils::{calc_total_voting_power_at, calc_voting_power};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

// Contract name and version used for migration.
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

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
        ibc_controller: addr_opt_validate(deps.api, &msg.ibc_controller)?,
        builder_unlock_addr: deps.api.addr_validate(&msg.builder_unlock_addr)?,
        proposal_voting_period: msg.proposal_voting_period,
        proposal_effective_delay: msg.proposal_effective_delay,
        proposal_expiration_period: msg.proposal_expiration_period,
        proposal_required_deposit: msg.proposal_required_deposit,
        proposal_required_quorum: Decimal::from_str(&msg.proposal_required_quorum)?,
        proposal_required_threshold: Decimal::from_str(&msg.proposal_required_threshold)?,
        whitelisted_links: msg.whitelisted_links,
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
/// * **ExecuteMsg::SubmitProposal { title, description, link, messages, ibc_channel }** Submits a new proposal.
///
/// * **ExecuteMsg::CheckMessages { messages }** Checks if the messages are correct.
/// Executes arbitrary messages on behalf of the Assembly contract. Always appends failing message to the end of the list.
///
/// * **ExecuteMsg::CheckMessagesPassed {}** Closing message for the `CheckMessages` endpoint.
///
/// * **ExecuteMsg::CastVote { proposal_id, vote }** Cast a vote on a specific proposal.
///
/// * **ExecuteMsg::EndProposal { proposal_id }** Sets the status of an expired/finalized proposal.
///
/// * **ExecuteMsg::ExecuteProposal { proposal_id }** Executes a successful proposal.
///
/// * **ExecuteMsg::UpdateConfig(config)** Updates the contract configuration.
///
/// * **ExecuteMsg::IBCProposalCompleted { proposal_id, status }** Updates proposal status InProgress -> Executed or Failed.
/// This endpoint processes callbacks from the ibc controller.
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
        ExecuteMsg::EndProposal { proposal_id } => end_proposal(deps, env, proposal_id),
        ExecuteMsg::ExecuteProposal { proposal_id } => execute_proposal(deps, env, proposal_id),
        ExecuteMsg::CheckMessages(messages) => check_messages(env, messages),
        ExecuteMsg::CheckMessagesPassed {} => Err(ContractError::MessagesCheckPassed {}),
        ExecuteMsg::UpdateConfig(config) => update_config(deps, env, info, config),
        ExecuteMsg::IBCProposalCompleted {
            proposal_id,
            status,
        } => update_ibc_proposal_status(deps, info, proposal_id, status),
    }
}

/// Submit a brand new proposal and lock some xASTRO as an anti-spam mechanism.
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
        // Seal total voting power. Query the total voting power one second before the proposal starts because
        // this is the last up to date finalized state of token factory tracker contract.
        total_voting_power: calc_total_voting_power_at(
            deps.querier,
            &config,
            env.block.time.seconds() - 1,
        )?,
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

    let proposal_quorum =
        Decimal::checked_from_ratio(total_votes, proposal.total_voting_power).unwrap_or_default();
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
                    messages: proposal.messages.clone(),
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
            .extend(proposal.messages.iter().cloned().map(SubMsg::new))
    }

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    Ok(response)
}

/// Checks that proposal messages are correct.
pub fn check_messages(env: Env, mut messages: Vec<CosmosMsg>) -> Result<Response, ContractError> {
    messages.push(
        wasm_execute(
            env.contract.address,
            &ExecuteMsg::CheckMessagesPassed {},
            vec![],
        )?
        .into(),
    );

    Ok(Response::new()
        .add_attribute("action", "check_messages")
        .add_messages(messages))
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

    if let Some(ibc_controller) = updated_config.ibc_controller {
        config.ibc_controller = Some(deps.api.addr_validate(&ibc_controller)?)
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
