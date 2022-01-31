use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Binary, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, Order, Response, StdResult, Uint128, Uint64, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_storage_plus::{Bound, U64Key};

use astroport::asset::addr_validate_to_lower;
use astroport_governance::assembly::{
    Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, Proposal, ProposalListResponse,
    ProposalMessage, ProposalStatus, ProposalVoteOption, ProposalVotesResponse, QueryMsg,
    UpdateConfig,
};

use astroport::xastro_token::QueryMsg as XAstroTokenQueryMsg;

use crate::error::ContractError;
use crate::state::{CONFIG, PROPOSALS, PROPOSAL_COUNT};

// version info for migration info
const CONTRACT_NAME: &str = "astro-assembly";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Proposal validation attributes
const MIN_TITLE_LENGTH: usize = 4;
const MAX_TITLE_LENGTH: usize = 64;
const MIN_DESC_LENGTH: usize = 4;
const MAX_DESC_LENGTH: usize = 1024;
const MIN_LINK_LENGTH: usize = 12;
const MAX_LINK_LENGTH: usize = 128;

// Default pagination constants
const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 30;

/// ## Description
/// Creates a new contract with the specified parameters in the `msg` variable.
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`]
///
/// * **_info** is the object of type [`MessageInfo`]
///
/// * **msg**  is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        xastro_token_addr: addr_validate_to_lower(deps.api, &msg.xastro_token_addr)?,
        staking_addr: addr_validate_to_lower(deps.api, &msg.staking_addr)?,
        proposal_voting_period: msg.proposal_voting_period,
        proposal_effective_delay: msg.proposal_effective_delay,
        proposal_expiration_period: msg.proposal_expiration_period,
        proposal_required_deposit: msg.proposal_required_deposit,
        proposal_required_quorum: Decimal::percent(msg.proposal_required_quorum),
        proposal_required_threshold: Decimal::percent(msg.proposal_required_threshold),
    };

    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    PROPOSAL_COUNT.save(deps.storage, &Uint64::zero())?;

    Ok(Response::default())
}

/// ## Description
/// Available the execute messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::Receive(cw20_msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::CastVote { proposal_id, vote }** Gets vote for an active propose.
///
/// * **ExecuteMsg::EndProposal { proposal_id }** Ends expired propose.
///
/// * **ExecuteMsg::ExecuteProposal { proposal_id }** Executes messages of the passed propose.
///
/// * **ExecuteMsg::RemoveCompletedProposal { proposal_id }** Removes completed specified proposal.
///
/// * **ExecuteMsg::UpdateConfig(config)** Updates contract configuration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(cw20_msg) => receive_cw20(deps, env, info, cw20_msg),
        ExecuteMsg::CastVote { proposal_id, vote } => cast_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::EndProposal { proposal_id } => end_proposal(deps, env, info, proposal_id),
        ExecuteMsg::ExecuteProposal { proposal_id } => {
            execute_proposal(deps, env, info, proposal_id)
        }
        ExecuteMsg::RemoveCompletedProposal { proposal_id } => {
            remove_completed_proposal(deps, env, info, proposal_id)
        }
        ExecuteMsg::UpdateConfig(config) => update_config(deps, env, info, config),
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise returns the [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **cw20_msg** is the object of type [`Cw20ReceiveMsg`].
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::SubmitProposal {
            title,
            description,
            link,
            messages,
        } => submit_proposal(
            deps,
            env,
            info,
            Addr::unchecked(cw20_msg.sender),
            cw20_msg.amount,
            title,
            description,
            link,
            messages,
        ),
    }
}

/// ## Description
/// Performs a submitting of proposal.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **sender** is the object of type [`Addr`]. Submitter of proposal.
///
/// * **deposit_amount** is the object of type [`Uint128`]. Deposited amount of proposal.
///
/// * **title** is the object of type [`String`]. Title of proposal.
///
/// * **description** is the object of type [`String`]. Description of proposal.
///
/// * **link** is the object of type [`Option<String>`]. Link of proposal.
///
/// * **messages** is the object of type [`Option<Vec<ProposalMessage>>`]. Messages of proposal.
#[allow(clippy::too_many_arguments)]
pub fn submit_proposal(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    deposit_amount: Uint128,
    title: String,
    description: String,
    link: Option<String>,
    messages: Option<Vec<ProposalMessage>>,
) -> Result<Response, ContractError> {
    // Validate title
    if title.len() < MIN_TITLE_LENGTH {
        return Err(ContractError::InvalidProposal(
            "Title too short".to_string(),
        ));
    }

    if title.len() > MAX_TITLE_LENGTH {
        return Err(ContractError::InvalidProposal("Title too long".to_string()));
    }

    // Validate description
    if description.len() < MIN_DESC_LENGTH {
        return Err(ContractError::InvalidProposal(
            "Description too short".to_string(),
        ));
    }
    if description.len() > MAX_DESC_LENGTH {
        return Err(ContractError::InvalidProposal(
            "Description too long".to_string(),
        ));
    }

    // Validate Link
    if let Some(link) = &link {
        if link.len() < MIN_LINK_LENGTH {
            return Err(ContractError::InvalidProposal("Link too short".to_string()));
        }
        if link.len() > MAX_LINK_LENGTH {
            return Err(ContractError::InvalidProposal("Link too long".to_string()));
        }
    }

    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.xastro_token_addr {
        return Err(ContractError::Unauthorized {});
    }

    if deposit_amount < config.proposal_required_deposit {
        return Err(ContractError::InsufficientDeposit {});
    }

    // Update proposal count
    let count = PROPOSAL_COUNT.update(deps.storage, |c| -> StdResult<_> {
        Ok(c.checked_add(Uint64::from(1u32))?)
    })?;

    PROPOSALS.save(
        deps.storage,
        U64Key::new(count.u64()),
        &Proposal {
            proposal_id: count,
            submitter: sender.clone(),
            status: ProposalStatus::Active,
            for_power: Uint128::zero(),
            against_power: Uint128::zero(),
            for_voters: Vec::new(),
            against_voters: Vec::new(),
            start_block: env.block.height,
            end_block: env.block.height + config.proposal_expiration_period,
            title,
            description,
            link,
            messages,
            deposit_amount,
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "submit_proposal")
        .add_attribute("submitter", sender.to_string())
        .add_attribute("proposal_id", count.to_string())
        .add_attribute(
            "proposal_end_height",
            (env.block.height + config.proposal_expiration_period).to_string(),
        ))
}

/// ## Description
/// Accepts the vote.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **proposal_id** is the identifier of the proposal.
///
/// * **vote_option** is the object of type [`ProposalVoteOption`]. Contains voting option.
pub fn cast_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote_option: ProposalVoteOption,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, U64Key::new(proposal_id))?;

    if proposal.status != ProposalStatus::Active {
        return Err(ContractError::ProposalNotActive {});
    }

    if proposal.submitter == info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if env.block.height > proposal.end_block {
        return Err(ContractError::VotingPeriodEnded {});
    }

    if proposal.for_voters.contains(&info.sender) || proposal.against_voters.contains(&info.sender)
    {
        return Err(ContractError::UserAlreadyVoted {});
    }

    let voting_power = calc_voting_power(&deps, info.sender.to_string(), proposal.start_block)?;

    if voting_power.is_zero() {
        return Err(ContractError::NoVotingPower {});
    }

    match vote_option {
        ProposalVoteOption::For => {
            proposal.for_power = proposal.for_power.checked_add(voting_power)?;
            proposal.for_voters.push(info.sender.clone());
        }
        ProposalVoteOption::Against => {
            proposal.against_power = proposal.against_power.checked_add(voting_power)?;
            proposal.against_voters.push(info.sender.clone());
        }
    };

    PROPOSALS.save(deps.storage, U64Key::new(proposal_id), &proposal)?;

    Ok(Response::new()
        .add_attribute("action", "cast_vote")
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("voter", &info.sender)
        .add_attribute("vote", vote_option.to_string())
        .add_attribute("voting_power", voting_power))
}

/// ## Description
/// Ends proposal voting.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **proposal_id** is the identifier of the proposal.
pub fn end_proposal(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, U64Key::new(proposal_id))?;

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

    let total_voting_power = calc_total_voting_power_at(&deps, proposal.start_block - 1)?;

    let mut proposal_quorum: Decimal = Decimal::zero();
    let mut proposal_threshold: Decimal = Decimal::zero();

    if !total_voting_power.is_zero() {
        proposal_quorum = Decimal::from_ratio(total_votes, total_voting_power);
    }

    if !total_votes.is_zero() {
        proposal_threshold = Decimal::from_ratio(for_votes, total_votes);
    }

    // Determine proposal result
    let msg = if proposal_quorum >= config.proposal_required_quorum
        && proposal_threshold > config.proposal_required_threshold
    {
        // if quorum and threshold are met then proposal passes
        // refund deposit amount to submitter
        proposal.status = ProposalStatus::Passed;

        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.xastro_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: proposal.submitter.to_string(),
                amount: proposal.deposit_amount,
            })?,
            funds: vec![],
        })
    } else {
        proposal.status = ProposalStatus::Rejected;

        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.xastro_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: config.staking_addr.to_string(),
                amount: proposal.deposit_amount,
            })?,
            funds: vec![],
        })
    };

    PROPOSALS.save(deps.storage, U64Key::new(proposal_id), &proposal)?;

    let response = Response::new()
        .add_attributes(vec![
            attr("action", "end_proposal"),
            attr("proposal_id", proposal_id.to_string()),
            attr("proposal_result", proposal.status.to_string()),
        ])
        .add_message(msg);

    Ok(response)
}

/// ## Description
/// Executes passed.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **proposal_id** is the identifier of the proposal.
pub fn execute_proposal(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, U64Key::new(proposal_id))?;

    if proposal.status != ProposalStatus::Passed {
        return Err(ContractError::ProposalNotPassed {});
    }

    let config = CONFIG.load(deps.storage)?;

    if env.block.height < (proposal.end_block + config.proposal_effective_delay) {
        return Err(ContractError::ProposalDelayNotEnded {});
    }

    if env.block.height
        > (proposal.end_block + config.proposal_effective_delay + config.proposal_expiration_period)
    {
        return Err(ContractError::ExecuteProposalExpired {});
    }

    proposal.status = ProposalStatus::Executed;

    PROPOSALS.save(deps.storage, U64Key::new(proposal_id), &proposal)?;

    let messages = match proposal.messages {
        Some(mut messages) => {
            messages.sort_by(|a, b| a.order.cmp(&b.order));
            messages.into_iter().map(|message| message.msg).collect()
        }
        None => vec![],
    };

    Ok(Response::new()
        .add_attribute("action", "execute_proposal")
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_messages(messages))
}

/// ## Description
/// Removes expired or rejected proposal from list.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **proposal_id** is the identifier of the proposal.
pub fn remove_completed_proposal(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut proposal = PROPOSALS.load(deps.storage, U64Key::new(proposal_id))?;

    if env.block.height
        > (proposal.end_block + config.proposal_effective_delay + config.proposal_expiration_period)
    {
        proposal.status = ProposalStatus::Expired;
    }

    if proposal.status != ProposalStatus::Expired && proposal.status != ProposalStatus::Rejected {
        return Err(ContractError::ProposalNotCompleted {});
    }

    PROPOSALS.remove(deps.storage, U64Key::new(proposal_id));

    Ok(Response::new()
        .add_attribute("action", "remove_completed_proposal")
        .add_attribute("proposal_id", proposal_id.to_string()))
}

/// ## Description
/// Updates config of assembly contract.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **updated_config** is the object of type [`UpdateConfig`].
pub fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    updated_config: UpdateConfig,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // In council, config can be updated only by itself (through an approved proposal)
    // instead of by it's owner
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    config.xastro_token_addr = updated_config
        .xastro_token_addr
        .map(|addr| addr_validate_to_lower(deps.api, &addr))
        .transpose()?
        .unwrap_or(config.xastro_token_addr);

    config.staking_addr = updated_config
        .staking_addr
        .map(|addr| addr_validate_to_lower(deps.api, &addr))
        .transpose()?
        .unwrap_or(config.staking_addr);

    config.proposal_voting_period = updated_config
        .proposal_voting_period
        .unwrap_or(config.proposal_voting_period);

    config.proposal_effective_delay = updated_config
        .proposal_effective_delay
        .unwrap_or(config.proposal_effective_delay);

    config.proposal_expiration_period = updated_config
        .proposal_expiration_period
        .unwrap_or(config.proposal_expiration_period);

    config.proposal_required_deposit = updated_config
        .proposal_required_deposit
        .map(Uint128::from)
        .unwrap_or(config.proposal_required_deposit);

    config.proposal_required_quorum = updated_config
        .proposal_required_quorum
        .map(Decimal::percent)
        .unwrap_or(config.proposal_required_quorum);

    config.proposal_required_threshold = updated_config
        .proposal_required_threshold
        .map(Decimal::percent)
        .unwrap_or(config.proposal_required_threshold);

    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

/// ## Description
/// Available the query messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns controls settings that specified in [`Config`] structure.
///
/// * **QueryMsg::Proposals { start, limit }** Returns the [`ProposalListResponse`] according to the specified input parameters.
///
/// * **QueryMsg::Proposal { proposal_id }** Returns an array that contains items of [`PairInfo`]
/// according to the specified input parameters.
///
/// * **QueryMsg::ProposalVotes { proposal_id }** Returns votes of the proposal that specified in [`ProposalVotesResponse`] structure.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Proposals { start, limit } => to_binary(&query_proposals(deps, start, limit)?),
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, proposal_id)?),
        QueryMsg::ProposalVotes { proposal_id } => {
            to_binary(&query_proposal_votes(deps, proposal_id)?)
        }
    }
}

/// ## Description
/// Returns the assembly contract configuration in [`Config`] structure.
/// ## Params
/// * **deps** is the object of type [`Deps`].
pub fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

/// ## Description
/// Returns list of proposals.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **start_after** is an [`Option`] type. Sets the index to start reading.
///
/// * **limit** is a [`Option`] type. Sets the number of items to be read.
pub fn query_proposals(
    deps: Deps,
    start: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let proposal_count = PROPOSAL_COUNT.load(deps.storage)?;

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start.map(|start| Bound::inclusive(U64Key::new(start)));

    let proposals_list: StdResult<Vec<_>> = PROPOSALS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (_k, v) = item?;
            Ok(v)
        })
        .collect();

    Ok(ProposalListResponse {
        proposal_count,
        proposal_list: proposals_list?,
    })
}

/// ## Description
/// Returns a proposal information specified in the [`Proposal`] structure.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **proposal_id** is the proposal identifier.
pub fn query_proposal(deps: Deps, proposal_id: u64) -> StdResult<Proposal> {
    let proposal = PROPOSALS.load(deps.storage, U64Key::new(proposal_id))?;
    Ok(proposal)
}

/// ## Description
/// Returns proposal votes specified in the custom [`ProposalVotesResponse`] structure.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **proposal_id** is the proposal identifier.
pub fn query_proposal_votes(deps: Deps, proposal_id: u64) -> StdResult<ProposalVotesResponse> {
    let proposal = PROPOSALS.load(deps.storage, U64Key::from(proposal_id))?;

    Ok(ProposalVotesResponse {
        proposal_id,
        for_power: proposal.for_power.u128(),
        against_power: proposal.against_power.u128(),
    })
}

/// ## Description
/// Calculates sender voting power at specified block.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **sender** is the object of type [`String`].
///
/// * **block** is the block height.
pub fn calc_voting_power(deps: &DepsMut, sender: String, block: u64) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let xastro_amount: BalanceResponse = deps.querier.query_wasm_smart(
        config.xastro_token_addr,
        &XAstroTokenQueryMsg::BalanceAt {
            address: sender,
            block,
        },
    )?;

    Ok(xastro_amount.balance)
}

/// ## Description
/// Calculates total voting power at specified block.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **block** is the block height.
pub fn calc_total_voting_power_at(deps: &DepsMut, block: u64) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let total_supply: Uint128 = deps.querier.query_wasm_smart(
        config.xastro_token_addr,
        &XAstroTokenQueryMsg::TotalSupplyAt { block },
    )?;

    Ok(total_supply)
}
