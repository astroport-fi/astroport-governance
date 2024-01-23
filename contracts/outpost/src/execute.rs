use astroport_governance::interchain::{MAX_IBC_TIMEOUT_SECONDS, MIN_IBC_TIMEOUT_SECONDS};
use astroport_governance::utils::check_contract_supports_channel;
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, to_json_binary, Addr, CosmosMsg, DepsMut, Env, IbcMsg, MessageInfo, Response,
    StdError, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport_governance::outpost::Config;
use astroport_governance::{
    assembly::ProposalVoteOption,
    interchain::Hub,
    outpost::{Cw20HookMsg, ExecuteMsg},
    voting_escrow_lite::get_emissions_voting_power,
};

use crate::query::get_user_voting_power;
use crate::state::VOTES;
use crate::{
    error::ContractError,
    state::{PendingVote, CONFIG, OWNERSHIP_PROPOSAL, PENDING_VOTES, PROPOSALS_CACHE},
};

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::Receive(cw20_msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// RemoveOutpost { outpost_addr } Removes an outpost from the hub but does not close the channel, but all messages will be rejected
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::UpdateConfig { hub_addr }** Update parameters in the Outpost contract. Only the owner is allowed to
/// update the config
///
/// * **ExecuteMsg::CastAssemblyVote { proposal_id, vote }** Cast a vote on an Assembly proposal from an Outpost
///
/// * **ExecuteMsg::CastEmissionsVote { votes }** Cast a vote during an emissions voting period
///
/// * **ExecuteMsg::KickUnlocked { user }** Kick an unlocked voter's voting power from the Generator Controller on the Hub
///
/// * **ExecuteMsg::WithdrawHubFunds {}** Withdraw stuck funds from the Hub in case of specific IBC failures
///
/// * **ExecuteMsg::ProposeNewOwner { new_owner, expires_in }** Creates a new request to change
/// contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::UpdateConfig {
            hub_addr,
            hub_channel,
            ibc_timeout_seconds,
        } => update_config(deps, env, info, hub_addr, hub_channel, ibc_timeout_seconds),
        ExecuteMsg::CastAssemblyVote { proposal_id, vote } => {
            cast_assembly_vote(deps, env, info, proposal_id, vote)
        }
        ExecuteMsg::CastEmissionsVote { votes } => cast_emissions_vote(deps, env, info, votes),
        ExecuteMsg::KickUnlocked { user } => kick_unlocked(deps, env, info, user),
        ExecuteMsg::KickBlacklisted { user } => kick_blacklisted(deps, env, info, user),
        ExecuteMsg::WithdrawHubFunds {} => withdraw_hub_funds(deps, env, info),
        ExecuteMsg::ProposeNewOwner {
            new_owner,
            expires_in,
        } => {
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
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
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
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on
/// the received template
///
/// Funds received here must be from the xASTRO contract and is used for
/// unstaking.
///
/// * **cw20_msg** CW20 message to process
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // We only allow xASTRO tokens to be sent here
    if info.sender != config.xastro_token_addr {
        return Err(ContractError::Unauthorized {});
    }

    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::Unstake {} => execute_remote_unstake(deps, env, cw20_msg),
    }
}

/// Start the process of unstaking xASTRO from the Hub
///
/// This burns the xASTRO we previously received and sends the unstake message
/// to the Hub where to original xASTRO will be unstaked and ASTRO returned
/// to the sender of this transaction.
///
/// Note: Incase of IBC failures they xASTRO will be returned to the user or
/// they'll need to withdraw the unstaked ASTRO from the Hub using ExecuteMsg::WithdrawHubFunds
fn execute_remote_unstake(
    deps: DepsMut,
    env: Env,
    msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Burn the xASTRO tokens we previously minted
    let burn_msg = Cw20ExecuteMsg::Burn { amount: msg.amount };
    let wasm_msg = WasmMsg::Execute {
        contract_addr: config.xastro_token_addr.to_string(),
        msg: to_json_binary(&burn_msg)?,
        funds: vec![],
    };

    let hub_channel = config
        .hub_channel
        .ok_or(ContractError::MissingHubChannel {})?;

    // Construct the unstake message to send to the Hub
    let unstake = Hub::Unstake {
        receiver: msg.sender.to_string(),
        amount: msg.amount,
    };
    let hub_unstake_msg: CosmosMsg = CosmosMsg::Ibc(IbcMsg::SendPacket {
        channel_id: hub_channel.clone(),
        data: to_json_binary(&unstake)?,
        timeout: env
            .block
            .time
            .plus_seconds(config.ibc_timeout_seconds)
            .into(),
    });

    Ok(Response::default()
        .add_message(wasm_msg)
        .add_message(hub_unstake_msg)
        .add_attribute("action", unstake.to_string())
        .add_attribute("amount", msg.amount.to_string())
        .add_attribute("channel", hub_channel))
}

/// Update the Outpost config
fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    hub_addr: Option<String>,
    hub_channel: Option<String>,
    ibc_timeout_seconds: Option<u64>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Only owner can update the config
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(hub_addr) = hub_addr {
        // We can't validate the Hub address
        config.hub_addr = hub_addr;
        // If a new Hub address is set, we clear the channel as we
        // must create a new IBC channel
        config.hub_channel = None;
    }

    if let Some(hub_channel) = hub_channel {
        // Ensure we have the channel that is being set
        check_contract_supports_channel(deps.querier, &env.contract.address, &hub_channel)?;

        // Update the channel to the correct one
        config.hub_channel = Some(hub_channel);
    }

    if let Some(ibc_timeout_seconds) = ibc_timeout_seconds {
        if !(MIN_IBC_TIMEOUT_SECONDS..=MAX_IBC_TIMEOUT_SECONDS).contains(&ibc_timeout_seconds) {
            return Err(ContractError::InvalidIBCTimeout {
                timeout: ibc_timeout_seconds,
                min: MIN_IBC_TIMEOUT_SECONDS,
                max: MAX_IBC_TIMEOUT_SECONDS,
            });
        }
        config.ibc_timeout_seconds = ibc_timeout_seconds;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// Cast a vote on a proposal from an Outpost
///
/// To validate the xASTRO holdings at the time the proposal was created we first
/// query the Hub for the proposal information if it hasn't been queried yet. Once
/// the proposal information is received we validate the vote and submit it
fn cast_assembly_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote_option: ProposalVoteOption,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let hub_channel = config
        .hub_channel
        .ok_or(ContractError::MissingHubChannel {})?;

    // Check if this user has voted already
    if VOTES.has(deps.storage, (&info.sender, proposal_id)) {
        return Err(ContractError::AlreadyVoted {});
    }

    // If we have this proposal in our local cached already, we can continue
    // with fetching the voting power and submitting the vote
    if let Some(proposal) = PROPOSALS_CACHE.may_load(deps.storage, proposal_id)? {
        let voting_power =
            get_user_voting_power(deps.as_ref(), info.sender.clone(), proposal.start_time)?;

        if voting_power.is_zero() {
            return Err(ContractError::NoVotingPower {
                address: info.sender.to_string(),
            });
        }

        // Construct the vote message and submit it to the Hub
        let cast_vote = Hub::CastAssemblyVote {
            proposal_id: proposal.id.u64(),
            vote_option: vote_option.clone(),
            voter: info.sender.clone(),
            voting_power,
        };
        let hub_msg = CosmosMsg::Ibc(IbcMsg::SendPacket {
            channel_id: hub_channel,
            data: to_json_binary(&cast_vote)?,
            timeout: env
                .block
                .time
                .plus_seconds(config.ibc_timeout_seconds)
                .into(),
        });

        // Log the vote to prevent spamming
        VOTES.save(deps.storage, (&info.sender, proposal_id), &vote_option)?;

        return Ok(Response::new()
            .add_message(hub_msg)
            .add_attribute("action", cast_vote.to_string())
            .add_attribute("user", info.sender.to_string()));
    }

    // If we don't have the proposal in our local cache it means that no
    // vote has been cast from this Outpost for this proposal
    // In this case we temporarily store the vote and submit an IBC transaction
    // to fetch the proposal information. When the information is received via
    // an IBC reply, we validate the data and submit the actual vote

    // If we already have a pending vote for this proposal we return an error
    // as we're waiting for the proposal IBC query to return. We can't store
    // lots of votes as we have no way to automatically submit them without
    // the risk of running out of gas

    if PENDING_VOTES.has(deps.storage, proposal_id) {
        return Err(ContractError::PendingVoteExists { proposal_id });
    }

    // Temporarily store the vote
    let pending_vote = PendingVote {
        proposal_id,
        voter: info.sender,
        vote_option,
    };
    PENDING_VOTES.save(deps.storage, proposal_id, &pending_vote)?;

    // Query for proposal
    let query_proposal = Hub::QueryProposal { id: proposal_id };
    let hub_query_msg = CosmosMsg::Ibc(IbcMsg::SendPacket {
        channel_id: hub_channel,
        data: to_json_binary(&query_proposal)?,
        timeout: env
            .block
            .time
            .plus_seconds(config.ibc_timeout_seconds)
            .into(),
    });

    Ok(Response::default()
        .add_message(hub_query_msg)
        .add_attribute("action", query_proposal.to_string())
        .add_attribute("id", proposal_id.to_string()))
}

/// Cast a vote on emissions during a vxASTRO voting period
///
/// We validate the voting power by checking the vxASTRO power at this
/// moment as vxASTRO lite does not have any warmup period
fn cast_emissions_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    votes: Vec<(String, u16)>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Validate vxASTRO voting power
    let vxastro_voting_power =
        get_emissions_voting_power(&deps.querier, config.vxastro_token_addr, &info.sender)?;

    if vxastro_voting_power.is_zero() {
        return Err(ContractError::NoVotingPower {
            address: info.sender.to_string(),
        });
    }

    let hub_channel = config
        .hub_channel
        .ok_or(ContractError::MissingHubChannel {})?;

    // Construct the vote message and submit it to the Hub
    let cast_vote = Hub::CastEmissionsVote {
        voter: info.sender.clone(),
        voting_power: vxastro_voting_power,
        votes,
    };
    let hub_msg = CosmosMsg::Ibc(IbcMsg::SendPacket {
        channel_id: hub_channel,
        data: to_json_binary(&cast_vote)?,
        timeout: env
            .block
            .time
            .plus_seconds(config.ibc_timeout_seconds)
            .into(),
    });
    Ok(Response::new()
        .add_message(hub_msg)
        .add_attribute("action", cast_vote.to_string())
        .add_attribute("user", info.sender.to_string()))
}

/// Kick an unlocked voter from the Generator Controller on the Hub
/// which will remove their voting power immediately.
///
/// We only finalise the unlock in the vxASTRO contract when this kick is
/// successful
fn kick_unlocked(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    user: Addr,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // This may only be called from the vxASTRO lite contract
    if info.sender != config.vxastro_token_addr {
        return Err(ContractError::Unauthorized {});
    }

    let hub_channel = config
        .hub_channel
        .ok_or(ContractError::MissingHubChannel {})?;

    // Construct the kick message and submit it to the Hub
    let kick_unlocked = Hub::KickUnlockedVoter {
        voter: user.clone(),
    };
    let hub_msg = CosmosMsg::Ibc(IbcMsg::SendPacket {
        channel_id: hub_channel,
        data: to_json_binary(&kick_unlocked)?,
        timeout: env
            .block
            .time
            .plus_seconds(config.ibc_timeout_seconds)
            .into(),
    });

    Ok(Response::new()
        .add_message(hub_msg)
        .add_attribute("action", kick_unlocked.to_string())
        .add_attribute("user", user))
}

/// Kick a blacklisted voter from the Generator Controller on the Hub
/// which will remove their voting power immediately.
///
/// This can be called multiple times without unintended side effects
fn kick_blacklisted(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    user: Addr,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // This may only be called from the vxASTRO lite contract
    if info.sender != config.vxastro_token_addr {
        return Err(ContractError::Unauthorized {});
    }

    let hub_channel = config
        .hub_channel
        .ok_or(ContractError::MissingHubChannel {})?;

    // Construct the kick message and submit it to the Hub
    let kick_blacklisted = Hub::KickBlacklistedVoter {
        voter: user.clone(),
    };
    let hub_msg = CosmosMsg::Ibc(IbcMsg::SendPacket {
        channel_id: hub_channel,
        data: to_json_binary(&kick_blacklisted)?,
        timeout: env
            .block
            .time
            .plus_seconds(config.ibc_timeout_seconds)
            .into(),
    });

    Ok(Response::new()
        .add_message(hub_msg)
        .add_attribute("action", kick_blacklisted.to_string())
        .add_attribute("user", user))
}

/// Submit a request to withdraw / retry sending funds stuck on the Hub
/// back to the sender address. This is possible because of IBC failures.
///
/// This will only return the funds of the user executing this transaction.
fn withdraw_hub_funds(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let hub_channel = config
        .hub_channel
        .ok_or(ContractError::MissingHubChannel {})?;

    // Construct the withdraw message and submit it to the Hub
    let withdraw = Hub::WithdrawFunds {
        user: info.sender.clone(),
    };
    let hub_msg = CosmosMsg::Ibc(IbcMsg::SendPacket {
        channel_id: hub_channel,
        data: to_json_binary(&withdraw)?,
        timeout: env
            .block
            .time
            .plus_seconds(config.ibc_timeout_seconds)
            .into(),
    });

    Ok(Response::new()
        .add_message(hub_msg)
        .add_attribute("action", withdraw.to_string())
        .add_attribute("user", info.sender.to_string()))
}

#[cfg(test)]
mod tests {

    use super::*;

    use cosmwasm_std::{testing::mock_info, IbcMsg, ReplyOn, SubMsg, Uint128, Uint64};

    use crate::{
        contract::instantiate,
        mock::{mock_all, setup_channel, HUB, OWNER, VXASTRO_TOKEN, XASTRO_TOKEN},
        query::query,
    };
    use astroport_governance::interchain::{Hub, ProposalSnapshot};

    // Test Cases:
    //
    // Expect Success
    //      - An unstake IBC message is emitted
    //
    // Expect Error
    //      - No xASTRO is sent to the contract
    //      - The funds sent to the contract is not xASTRO
    //      - The Hub address and channel isn't set
    //
    #[test]
    fn unstake() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user = "user";
        let user_funds = Uint128::from(1000u128);
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
                ibc_timeout_seconds: 10,
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

        // Attempt to unstake with an incorrect token
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not_xastro", &[]),
            astroport_governance::outpost::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: user.to_string(),
                amount: user_funds,
                msg: to_json_binary(&astroport_governance::outpost::Cw20HookMsg::Unstake {})
                    .unwrap(),
            }),
        )
        .unwrap_err();

        assert_eq!(err, ContractError::Unauthorized {});

        // Attempt to unstake correctly
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(XASTRO_TOKEN, &[]),
            astroport_governance::outpost::ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: user.to_string(),
                amount: user_funds,
                msg: to_json_binary(&astroport_governance::outpost::Cw20HookMsg::Unstake {})
                    .unwrap(),
            }),
        )
        .unwrap();

        // Build the expected message
        let ibc_message = to_json_binary(&Hub::Unstake {
            receiver: user.to_string(),
            amount: user_funds,
        })
        .unwrap();

        // We should have two messages
        assert_eq!(res.messages.len(), 2);

        // First message must be the burn of the amount of xASTRO sent
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: WasmMsg::Execute {
                    contract_addr: XASTRO_TOKEN.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::Burn { amount: user_funds }).unwrap(),
                    funds: vec![],
                }
                .into(),
            }
        );

        // Second message must be the IBC unstake
        assert_eq!(
            res.messages[1],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: IbcMsg::SendPacket {
                    channel_id: "channel-3".to_string(),
                    data: ibc_message,
                    timeout: env.block.time.plus_seconds(ibc_timeout_seconds).into(),
                }
                .into(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - The config is updated
    //
    // Expect Error
    //      - When the config is updated by a non-owner
    //
    #[test]
    fn update_config() {
        let (mut deps, env, info) = mock_all(OWNER);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::outpost::InstantiateMsg {
                owner: OWNER.to_string(),
                xastro_token_addr: XASTRO_TOKEN.to_string(),
                vxastro_token_addr: VXASTRO_TOKEN.to_string(),
                hub_addr: HUB.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        setup_channel(deps.as_mut(), env.clone());

        // Attempt to update the hub address by a non-owner
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not_owner", &[]),
            astroport_governance::outpost::ExecuteMsg::UpdateConfig {
                hub_addr: Some("new_hub".to_string()),
                hub_channel: None,
                ibc_timeout_seconds: None,
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let config = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::outpost::QueryMsg::Config {},
        )
        .unwrap();

        // Ensure the config set during instantiation is still there
        assert_eq!(
            config,
            to_json_binary(&astroport_governance::outpost::Config {
                owner: Addr::unchecked(OWNER),
                xastro_token_addr: Addr::unchecked(XASTRO_TOKEN),
                vxastro_token_addr: Addr::unchecked(VXASTRO_TOKEN),
                hub_addr: HUB.to_string(),
                hub_channel: None,
                ibc_timeout_seconds: 10,
            })
            .unwrap()
        );

        // Attempt to update the hub address by the owner
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::outpost::ExecuteMsg::UpdateConfig {
                hub_addr: Some("new_owner_hub".to_string()),
                hub_channel: None,
                ibc_timeout_seconds: None,
            },
        )
        .unwrap();

        let config = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::outpost::QueryMsg::Config {},
        )
        .unwrap();

        // Ensure the config set after the update is correct
        // Once a new Hub is set, the Hub channel is cleared to allow a new
        // connection
        assert_eq!(
            config,
            to_json_binary(&astroport_governance::outpost::Config {
                owner: Addr::unchecked(OWNER),
                xastro_token_addr: Addr::unchecked(XASTRO_TOKEN),
                vxastro_token_addr: Addr::unchecked(VXASTRO_TOKEN),
                hub_addr: "new_owner_hub".to_string(),
                hub_channel: None,
                ibc_timeout_seconds: 10,
            })
            .unwrap()
        );

        // Update the hub channel
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::outpost::ExecuteMsg::UpdateConfig {
                hub_addr: None,
                hub_channel: Some("channel-15".to_string()),
                ibc_timeout_seconds: None,
            },
        )
        .unwrap();

        let config = query(
            deps.as_ref(),
            env.clone(),
            astroport_governance::outpost::QueryMsg::Config {},
        )
        .unwrap();

        // Ensure the config set after the update is correct
        // Once a new Hub is set, the Hub channel is cleared to allow a new
        // connection
        assert_eq!(
            config,
            to_json_binary(&astroport_governance::outpost::Config {
                owner: Addr::unchecked(OWNER),
                xastro_token_addr: Addr::unchecked(XASTRO_TOKEN),
                vxastro_token_addr: Addr::unchecked(VXASTRO_TOKEN),
                hub_addr: "new_owner_hub".to_string(),
                hub_channel: Some("channel-15".to_string()),
                ibc_timeout_seconds: 10,
            })
            .unwrap()
        );

        // Update the IBC timeout
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::outpost::ExecuteMsg::UpdateConfig {
                hub_addr: None,
                hub_channel: None,
                ibc_timeout_seconds: Some(35),
            },
        )
        .unwrap();

        let config = query(
            deps.as_ref(),
            env,
            astroport_governance::outpost::QueryMsg::Config {},
        )
        .unwrap();

        // Ensure the config set after the update is correct
        // Once a new Hub is set, the Hub channel is cleared to allow a new
        // connection
        assert_eq!(
            config,
            to_json_binary(&astroport_governance::outpost::Config {
                owner: Addr::unchecked(OWNER),
                xastro_token_addr: Addr::unchecked(XASTRO_TOKEN),
                vxastro_token_addr: Addr::unchecked(VXASTRO_TOKEN),
                hub_addr: "new_owner_hub".to_string(),
                hub_channel: Some("channel-15".to_string()),
                ibc_timeout_seconds: 35,
            })
            .unwrap()
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - A proposal query is emitted when the proposal is not in the cache
    //      - A vote is emitted when the proposal is in the cache
    //
    // Expect Error
    //      - User has no voting power at the time of the proposal
    //
    #[test]
    fn vote_on_proposal() {
        let (mut deps, env, info) = mock_all(OWNER);

        let proposal_id = 1u64;
        let user = "user";
        let voting_power = 1000u64;
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

        // Cast a vote with no proposal in the cache
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user, &[]),
            astroport_governance::outpost::ExecuteMsg::CastAssemblyVote {
                proposal_id: 1,
                vote: astroport_governance::assembly::ProposalVoteOption::For,
            },
        )
        .unwrap();

        // Wrap the query
        let ibc_message = to_json_binary(&Hub::QueryProposal { id: proposal_id }).unwrap();

        // Ensure a query is emitted
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: IbcMsg::SendPacket {
                    channel_id: "channel-3".to_string(),
                    data: ibc_message,
                    timeout: env.block.time.plus_seconds(ibc_timeout_seconds).into(),
                }
                .into(),
            }
        );

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
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user, &[]),
            astroport_governance::outpost::ExecuteMsg::CastAssemblyVote {
                proposal_id,
                vote: astroport_governance::assembly::ProposalVoteOption::For,
            },
        )
        .unwrap();

        // Build the expected message
        let ibc_message = to_json_binary(&Hub::CastAssemblyVote {
            proposal_id,
            voter: Addr::unchecked(user),
            vote_option: astroport_governance::assembly::ProposalVoteOption::For,
            voting_power: Uint128::from(voting_power),
        })
        .unwrap();

        // We should only have 1 message
        assert_eq!(res.messages.len(), 1);

        // Ensure a vote is emitted
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: IbcMsg::SendPacket {
                    channel_id: "channel-3".to_string(),
                    data: ibc_message,
                    timeout: env.block.time.plus_seconds(ibc_timeout_seconds).into(),
                }
                .into(),
            }
        );

        // Cast a vote on a proposal already voted on
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user, &[]),
            astroport_governance::outpost::ExecuteMsg::CastAssemblyVote {
                proposal_id,
                vote: astroport_governance::assembly::ProposalVoteOption::For,
            },
        )
        .unwrap_err();

        assert_eq!(err, ContractError::AlreadyVoted {});

        // Check that we can query the vote
        let vote_data = query(
            deps.as_ref(),
            env,
            astroport_governance::outpost::QueryMsg::ProposalVoted {
                proposal_id,
                user: user.to_string(),
            },
        )
        .unwrap();

        assert_eq!(vote_data, to_json_binary(&ProposalVoteOption::For).unwrap());
    }

    // Test Cases:
    //
    // Expect Success
    //      - An emissions vote is emitted is the user has voting power
    //
    // Expect Error
    //      - User has no voting power
    //
    #[test]
    fn vote_on_emissions() {
        let (mut deps, env, info) = mock_all(OWNER);

        let user = "user";
        let votes = vec![("pool".to_string(), 10000u16)];
        let voting_power = 1000u64;
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

        // Cast a vote on emissions
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user, &[]),
            astroport_governance::outpost::ExecuteMsg::CastEmissionsVote {
                votes: votes.clone(),
            },
        )
        .unwrap();

        // Build the expected message
        let ibc_message = to_json_binary(&Hub::CastEmissionsVote {
            voter: Addr::unchecked(user),
            votes,
            voting_power: Uint128::from(voting_power),
        })
        .unwrap();

        // We should only have 1 message
        assert_eq!(res.messages.len(), 1);

        // Ensure a vote is emitted
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: IbcMsg::SendPacket {
                    channel_id: "channel-3".to_string(),
                    data: ibc_message,
                    timeout: env.block.time.plus_seconds(ibc_timeout_seconds).into(),
                }
                .into(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - The kick message is forwarded
    //
    // Expect Error
    //      - When the sender is not the vxASTRO contract
    //
    #[test]
    fn kick_unlocked() {
        let (mut deps, env, info) = mock_all(OWNER);

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

        // Kick a user as another user, not allowed
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user, &[]),
            astroport_governance::outpost::ExecuteMsg::KickUnlocked {
                user: Addr::unchecked(user),
            },
        )
        .unwrap_err();

        assert_eq!(err, ContractError::Unauthorized {});

        // Kick a user as the vxASTRO contract
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(VXASTRO_TOKEN, &[]),
            astroport_governance::outpost::ExecuteMsg::KickUnlocked {
                user: Addr::unchecked(user),
            },
        )
        .unwrap();

        // Build the expected message
        let ibc_message = to_json_binary(&Hub::KickUnlockedVoter {
            voter: Addr::unchecked(user),
        })
        .unwrap();

        // We should only have 1 message
        assert_eq!(res.messages.len(), 1);

        // Ensure a kick is emitted
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: IbcMsg::SendPacket {
                    channel_id: "channel-3".to_string(),
                    data: ibc_message,
                    timeout: env.block.time.plus_seconds(ibc_timeout_seconds).into(),
                }
                .into(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - The kick message is forwarded
    //
    // Expect Error
    //      - When the sender is not the vxASTRO contract
    //
    #[test]
    fn kick_blacklisted() {
        let (mut deps, env, info) = mock_all(OWNER);

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

        // Kick a user as another user, not allowed
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user, &[]),
            astroport_governance::outpost::ExecuteMsg::KickBlacklisted {
                user: Addr::unchecked(user),
            },
        )
        .unwrap_err();

        assert_eq!(err, ContractError::Unauthorized {});

        // Kick a user as the vxASTRO contract
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(VXASTRO_TOKEN, &[]),
            astroport_governance::outpost::ExecuteMsg::KickBlacklisted {
                user: Addr::unchecked(user),
            },
        )
        .unwrap();

        // Build the expected message
        let ibc_message = to_json_binary(&Hub::KickBlacklistedVoter {
            voter: Addr::unchecked(user),
        })
        .unwrap();

        // We should only have 1 message
        assert_eq!(res.messages.len(), 1);

        // Ensure a kick is emitted
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: IbcMsg::SendPacket {
                    channel_id: "channel-3".to_string(),
                    data: ibc_message,
                    timeout: env.block.time.plus_seconds(ibc_timeout_seconds).into(),
                }
                .into(),
            }
        );
    }

    // Test Cases:
    //
    // Expect Success
    //      - The kick message is forwarded
    //
    // Expect Error
    //      - When the sender is not the vxASTRO contract
    //
    #[test]
    fn withdraw_funds() {
        let (mut deps, env, info) = mock_all(OWNER);

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

        // Withdraw stuck funds from the Hub
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user, &[]),
            astroport_governance::outpost::ExecuteMsg::WithdrawHubFunds {},
        )
        .unwrap();

        // Build the expected message
        let ibc_message = to_json_binary(&Hub::WithdrawFunds {
            user: Addr::unchecked(user),
        })
        .unwrap();

        // We should only have 1 message
        assert_eq!(res.messages.len(), 1);

        // Ensure a withdrawal is emitted
        assert_eq!(
            res.messages[0],
            SubMsg {
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
                msg: IbcMsg::SendPacket {
                    channel_id: "channel-3".to_string(),
                    data: ibc_message,
                    timeout: env.block.time.plus_seconds(ibc_timeout_seconds).into(),
                }
                .into(),
            }
        );
    }
}
