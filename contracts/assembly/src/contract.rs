use std::str::FromStr;

use astroport::asset::addr_opt_validate;
use astroport::staking;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coins, ensure, ensure_eq, wasm_execute, Addr, Api, BankMsg, CosmosMsg, Decimal, DepsMut,
    Env, MessageInfo, QuerierWrapper, Response, StdError, Storage, SubMsg, Uint128, Uint64,
    WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::must_pay;
use ibc_controller_package::ExecuteMsg as ControllerExecuteMsg;

use astroport_governance::assembly::{
    validate_links, Config, ExecuteMsg, InstantiateMsg, Proposal, ProposalStatus,
    ProposalVoteOption, UpdateConfig,
};
use astroport_governance::emissions_controller::hub::HubMsg;
use astroport_governance::utils::check_contract_supports_channel;
use astroport_governance::{emissions_controller, voting_escrow};

use crate::error::ContractError;
use crate::state::{CONFIG, PROPOSALS, PROPOSAL_COUNT, PROPOSAL_VOTERS};
use crate::utils::{calc_total_voting_power_at, calc_voting_power};

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
        vxastro_contract: None,
        emissions_controller: None,
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
/// * **ExecuteMsg::SubmitProposal { title, description, link, messages, ibc_channel }** Submits a new proposal.
///
/// * **ExecuteMsg::CheckMessages { messages }** Checks if the messages are correct.
/// Executes arbitrary messages on behalf of the Assembly contract. Always appends failing message to the end of the list.
///
/// * **ExecuteMsg::CheckMessagesPassed {}** Closing message for the `CheckMessages` endpoint.
///
/// * **ExecuteMsg::CastVote { proposal_id, vote }** Cast a vote on a specific proposal.
///
/// * **ExecuteMsg::CastVoteOutpost { voter, voting_power, proposal_id, vote }** Applies a vote on a specific proposal from outpost.
/// Only emissions controller is allowed to call this endpoint.
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
        ExecuteMsg::CastVote { proposal_id, vote } => {
            let voter = info.sender.to_string();
            let proposal = PROPOSALS.load(deps.storage, proposal_id)?;

            let voting_power = calc_voting_power(deps.as_ref(), voter.clone(), &proposal)?;
            ensure!(!voting_power.is_zero(), ContractError::NoVotingPower {});

            cast_vote(
                deps.storage,
                env,
                voter,
                voting_power,
                proposal_id,
                proposal,
                vote,
            )
        }
        ExecuteMsg::CastVoteOutpost {
            voter,
            voting_power,
            proposal_id,
            vote,
        } => {
            let config = CONFIG.load(deps.storage)?;
            ensure!(
                Some(info.sender) == config.emissions_controller,
                ContractError::Unauthorized {}
            );

            // This endpoint should never fail if called from the emissions controller.
            // Otherwise, an IBC packet will never be acknowledged.
            (|| {
                let proposal = PROPOSALS.load(deps.storage, proposal_id)?;

                cast_vote(
                    deps.storage,
                    env,
                    voter,
                    voting_power,
                    proposal_id,
                    proposal,
                    vote,
                )
            })()
            .or_else(|err| {
                Ok(Response::new()
                    .add_attribute("action", "cast_vote")
                    .add_attribute("error", err.to_string()))
            })
        }
        ExecuteMsg::EndProposal { proposal_id } => end_proposal(deps, env, proposal_id),
        ExecuteMsg::ExecuteProposal { proposal_id } => execute_proposal(deps, env, proposal_id),
        ExecuteMsg::CheckMessages(messages) => check_messages(info, deps.api, env, messages),
        ExecuteMsg::CheckMessagesPassed {} => Err(ContractError::MessagesCheckPassed {}),
        ExecuteMsg::UpdateConfig(config) => update_config(deps, env, info, config),
        ExecuteMsg::IBCProposalCompleted {
            proposal_id,
            status,
        } => update_ibc_proposal_status(deps, info, proposal_id, status),
        ExecuteMsg::ExecuteFromMultisig(proposal_messages) => {
            exec_from_multisig(deps.querier, info, env, proposal_messages)
        }
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
        against_power: Uint128::zero(),
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

    let mut response = Response::new().add_attributes([
        attr("action", "submit_proposal"),
        attr("submitter", info.sender),
        attr("proposal_id", count),
        attr(
            "proposal_end_height",
            (env.block.height + config.proposal_voting_period).to_string(),
        ),
    ]);

    if let Some(emissions_controller) = config.emissions_controller {
        // Send IBC packets to all outposts to register this proposal.
        let outposts_register_msg = wasm_execute(
            emissions_controller,
            &emissions_controller::msg::ExecuteMsg::Custom(HubMsg::RegisterProposal {
                proposal_id: count.u64(),
            }),
            vec![],
        )?;
        response = response.add_message(outposts_register_msg);
    }

    Ok(response)
}

/// Cast a vote on a proposal.
///
/// * **voter** is the bech32 address of the voter from any of the supported outposts.
///
/// * **voting_power** is the voting power of the voter.
///
/// * **proposal_id** is the identifier of the proposal.
///
/// * **proposal** is [`Proposal`] object.
///
/// * **vote_option** contains the vote option.
pub fn cast_vote(
    storage: &mut dyn Storage,
    env: Env,
    voter: String,
    voting_power: Uint128,
    proposal_id: u64,
    mut proposal: Proposal,
    vote_option: ProposalVoteOption,
) -> Result<Response, ContractError> {
    if proposal.status != ProposalStatus::Active {
        return Err(ContractError::ProposalNotActive {});
    }

    if env.block.height > proposal.end_block {
        return Err(ContractError::VotingPeriodEnded {});
    }

    if PROPOSAL_VOTERS.has(storage, (proposal_id, voter.clone())) {
        return Err(ContractError::UserAlreadyVoted {});
    }

    match vote_option {
        ProposalVoteOption::For => {
            proposal.for_power = proposal.for_power.checked_add(voting_power)?;
        }
        ProposalVoteOption::Against => {
            proposal.against_power = proposal.against_power.checked_add(voting_power)?;
        }
    };
    PROPOSAL_VOTERS.save(storage, (proposal_id, voter.clone()), &vote_option)?;

    PROPOSALS.save(storage, proposal_id, &proposal)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "cast_vote"),
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

    let mut response = Response::new().add_attributes([
        attr("action", "execute_proposal"),
        attr("proposal_id", proposal_id.to_string()),
    ]);

    if env.block.height > proposal.expiration_block {
        proposal.status = ProposalStatus::Expired;
    } else if let Some(channel) = &proposal.ibc_channel {
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

    Ok(response.add_attribute("proposal_status", proposal.status.to_string()))
}

/// Checks that proposal messages are correct.
pub fn check_messages(
    info: MessageInfo,
    api: &dyn Api,
    env: Env,
    mut messages: Vec<CosmosMsg>,
) -> Result<Response, ContractError> {
    ensure_eq!(
        info.sender,
        env.contract.address,
        ContractError::Unauthorized {}
    );

    messages.iter().try_for_each(|msg| match msg {
        CosmosMsg::Wasm(
            WasmMsg::Migrate { contract_addr, .. } | WasmMsg::UpdateAdmin { contract_addr, .. },
        ) if api.addr_validate(contract_addr)? == env.contract.address => {
            Err(StdError::generic_err(
                "Can't check messages with a migration or update admin message of the contract itself",
            ))
        }
        CosmosMsg::Stargate { type_url, .. } if type_url.contains("MsgGrant") => Err(
            StdError::generic_err("Can't check messages with a MsgGrant message"),
        ),
        _ => Ok(()),
    })?;

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

    let mut attrs = vec![attr("action", "update_config")];

    if let Some(ibc_controller) = updated_config.ibc_controller {
        config.ibc_controller = Some(deps.api.addr_validate(&ibc_controller)?);
        attrs.push(attr("new_ibc_controller", ibc_controller));
    }

    if let Some(builder_unlock_addr) = updated_config.builder_unlock_addr {
        config.builder_unlock_addr = deps.api.addr_validate(&builder_unlock_addr)?;
        attrs.push(attr("new_builder_unlock_addr", builder_unlock_addr));
    }

    if let Some(proposal_voting_period) = updated_config.proposal_voting_period {
        config.proposal_voting_period = proposal_voting_period;
        attrs.push(attr(
            "new_proposal_voting_period",
            proposal_voting_period.to_string(),
        ));
    }

    if let Some(proposal_effective_delay) = updated_config.proposal_effective_delay {
        config.proposal_effective_delay = proposal_effective_delay;
        attrs.push(attr(
            "new_proposal_effective_delay",
            proposal_effective_delay.to_string(),
        ));
    }

    if let Some(proposal_expiration_period) = updated_config.proposal_expiration_period {
        config.proposal_expiration_period = proposal_expiration_period;
        attrs.push(attr(
            "new_proposal_expiration_period",
            proposal_expiration_period.to_string(),
        ));
    }

    if let Some(proposal_required_deposit) = updated_config.proposal_required_deposit {
        config.proposal_required_deposit = proposal_required_deposit;
        attrs.push(attr(
            "new_proposal_required_deposit",
            proposal_required_deposit.to_string(),
        ));
    }

    if let Some(proposal_required_quorum) = updated_config.proposal_required_quorum {
        config.proposal_required_quorum = proposal_required_quorum;
        attrs.push(attr(
            "new_proposal_required_quorum",
            proposal_required_quorum.to_string(),
        ));
    }

    if let Some(proposal_required_threshold) = updated_config.proposal_required_threshold {
        config.proposal_required_threshold = proposal_required_threshold;
        attrs.push(attr(
            "new_proposal_required_threshold",
            proposal_required_threshold.to_string(),
        ));
    }

    if let Some(whitelist_add) = updated_config.whitelist_add {
        validate_links(&whitelist_add)?;

        let mut new_links = whitelist_add
            .into_iter()
            .filter(|link| !config.whitelisted_links.contains(link))
            .collect::<Vec<_>>();

        attrs.push(attr("new_whitelisted_links", new_links.join(", ")));

        config.whitelisted_links.append(&mut new_links);
    }

    if let Some(whitelist_remove) = updated_config.whitelist_remove {
        config
            .whitelisted_links
            .retain(|link| !whitelist_remove.contains(link));

        attrs.push(attr(
            "removed_whitelisted_links",
            whitelist_remove.join(", "),
        ));

        if config.whitelisted_links.is_empty() {
            return Err(ContractError::WhitelistEmpty {});
        }
    }

    if let Some(vxastro) = updated_config.vxastro {
        let emissions_controller = deps
            .querier
            .query_wasm_smart::<voting_escrow::Config>(
                &vxastro,
                &voting_escrow::QueryMsg::Config {},
            )?
            .emissions_controller;

        config.emissions_controller = Some(Addr::unchecked(&emissions_controller));
        config.vxastro_contract = Some(Addr::unchecked(&vxastro));

        attrs.push(attr("new_emissions_controller", emissions_controller));
        attrs.push(attr("new_vxastro_contract", vxastro));
    }

    #[cfg(not(feature = "testnet"))]
    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attrs))
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

pub fn exec_from_multisig(
    querier: QuerierWrapper,
    info: MessageInfo,
    env: Env,
    messages: Vec<CosmosMsg>,
) -> Result<Response, ContractError> {
    match querier
        .query_wasm_contract_info(&env.contract.address)?
        .admin
    {
        None => Err(ContractError::Unauthorized {}),
        // Don't allow to execute this endpoint if the contract is admin of itself
        Some(admin) if admin != info.sender || admin == env.contract.address => {
            Err(ContractError::Unauthorized {})
        }
        _ => Ok(()),
    }?;

    Ok(Response::new().add_messages(messages))
}
