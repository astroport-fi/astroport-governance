use astroport::asset::{Asset, AssetInfo};
use astroport::incentives::{InputSchedule, RewardType};
use cosmwasm_std::{attr, coin, coins, Decimal, Decimal256, Empty, Event};
use cw_multi_test::Executor;
use cw_utils::PaymentError;

use astroport_emissions_controller_outpost::error::ContractError;
use astroport_governance::assembly::ProposalVoteOption;
use astroport_governance::emissions_controller::consts::{EPOCH_LENGTH, IBC_TIMEOUT};
use astroport_governance::emissions_controller::msg::{ExecuteMsg, VxAstroIbcMsg};
use astroport_governance::emissions_controller::outpost::UserIbcError;
use astroport_governance::voting_escrow::LockInfoResponse;
use astroport_governance::{emissions_controller, voting_escrow};
use astroport_voting_escrow::state::UNLOCK_PERIOD;

use crate::common::helper::{get_epoch_start, ControllerHelper};

mod common;

#[test]
fn set_emissions_test() {
    let mut helper = ControllerHelper::new();
    let astro = helper.astro.clone();

    let pool1 = helper.create_pair("token1", "token2");
    let user = helper.app.api().addr_make("permissionless");

    // Incentivizing with any token other than astro should fail
    let funds = [coin(100_000000, "token1")];
    helper.mint_tokens(&user, &funds).unwrap();
    let schedules = [(
        pool1.as_str(),
        InputSchedule {
            reward: Asset::native("token1", 100_000000u64),
            duration_periods: 1,
        },
    )];
    let err = helper.set_emissions(&user, &schedules, &funds).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PaymentError(PaymentError::MissingDenom("astro".to_string()))
    );

    // Trying to bypass payment error by sending astro.
    // Still should fail due to incentive fee absence
    let funds = [coin(100_000000, &astro)];
    helper.mint_tokens(&user, &funds).unwrap();
    let schedules = [(
        pool1.as_str(),
        InputSchedule {
            reward: Asset::native("token1", 100_000000u64),
            duration_periods: 1,
        },
    )];
    let err = helper.set_emissions(&user, &schedules, &funds).unwrap_err();
    assert_eq!(
        err.downcast::<astroport_incentives::error::ContractError>()
            .unwrap(),
        astroport_incentives::error::ContractError::IncentivizationFeeExpected {
            fee: coin(250_000000, &astro).to_string(),
            lp_token: pool1.to_string(),
            new_reward_token: "token1".to_string(),
        }
    );

    let mut schedules = vec![
        (
            pool1.as_str(),
            InputSchedule {
                reward: Asset::native(&astro, 100_000000u64),
                duration_periods: 1,
            },
        ),
        (
            "random", // <--- invalid pool
            InputSchedule {
                reward: Asset::native(&astro, 100_000000u64),
                duration_periods: 1,
            },
        ),
    ];

    // Try to incentivize with wrong funds
    let funds = coins(100_000000, &astro);
    helper.mint_tokens(&user, &funds).unwrap();
    let err = helper
        .set_emissions(&user, &schedules, &coins(100_000000, &astro))
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidAstroAmount {
            expected: 200_000000u128.into(),
            actual: 100_000000u128.into()
        }
    );

    // Try schedule with <1 uASTRO reward per second
    let invalid_schedules = [(
        pool1.as_str(),
        InputSchedule {
            reward: Asset::native(&astro, 1000u64),
            duration_periods: 1,
        },
    )];
    let err = helper
        .set_emissions(&user, &invalid_schedules, &coins(1000, &astro))
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoValidSchedules {}
    );

    // However, if we mix this invalid schedule with valid ones, it will be filtered out
    schedules.push((
        pool1.as_str(),
        InputSchedule {
            reward: Asset::native(&astro, 1000u64),
            duration_periods: 1,
        },
    ));
    let funds = coins(200_001000, &astro);
    helper.mint_tokens(&user, &funds).unwrap();

    let resp = helper.set_emissions(&user, &schedules, &funds).unwrap();
    // Assert mocked ibc event
    let has_event = resp.has_event(
        &Event::new("transfer").add_attributes([
            attr(
                "packet_timeout_timestamp",
                helper
                    .app
                    .block_info()
                    .time
                    .plus_seconds(IBC_TIMEOUT)
                    .seconds()
                    .to_string(),
            ),
            attr("packet_src_port", "transfer"),
            attr("packet_src_channel", "channel-2"),
            attr("to_address", "emissions_controller"),
            attr("amount", coin(100_001000, &astro).to_string()),
        ]),
    );
    assert!(
        has_event,
        "Expected IBC transfer event. Actual {:?}",
        resp.events
    );

    // Check schedule in the incentives contract
    let expected_rps = Decimal256::from_ratio(100_000000u64, EPOCH_LENGTH);
    let rewards = helper.query_rewards(&pool1).unwrap();
    let epoch_start = get_epoch_start(helper.app.block_info().time.seconds());
    assert_eq!(rewards.len(), 1);
    assert_eq!(rewards[0].rps, expected_rps);
    assert_eq!(
        rewards[0].reward,
        RewardType::Ext {
            info: AssetInfo::native(&astro),
            next_update_ts: epoch_start + EPOCH_LENGTH
        }
    );
}

#[test]
fn permissioned_set_emissions_test() {
    let mut helper = ControllerHelper::new();
    let astro = helper.astro.clone();
    let owner = helper.owner.clone();

    let pool1 = helper.create_pair("token1", "token2");

    // Unauthorized check
    let random = helper.app.api().addr_make("random");
    let err = helper
        .permissioned_set_emissions(&random, &[], &[])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    // Incentivizing with any token other than astro should fail due to fee absence
    let funds = [coin(100_000000, &astro)];
    helper.mint_tokens(&owner, &funds).unwrap();
    let schedules = [(
        pool1.as_str(),
        InputSchedule {
            reward: Asset::native("token1", 100_000000u64),
            duration_periods: 1,
        },
    )];
    let err = helper
        .permissioned_set_emissions(&owner, &schedules, &funds)
        .unwrap_err();
    assert_eq!(
        err.downcast::<astroport_incentives::error::ContractError>()
            .unwrap(),
        astroport_incentives::error::ContractError::IncentivizationFeeExpected {
            fee: coin(250_000000, &astro).to_string(),
            lp_token: pool1.to_string(),
            new_reward_token: "token1".to_string(),
        }
    );

    let schedules = [
        (
            pool1.as_str(),
            InputSchedule {
                reward: Asset::native(&astro, 100_000000u64),
                duration_periods: 1,
            },
        ),
        (
            "random", // <--- invalid pool
            InputSchedule {
                reward: Asset::native(&astro, 100_000000u64),
                duration_periods: 1,
            },
        ),
    ];

    // Try to incentivize with zero funds in balance.
    // Error happens on dispatch from emissions controller to incentives contract
    let err = helper
        .permissioned_set_emissions(&owner, &schedules, &[])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidAstroAmount {
            expected: 200_000000u128.into(),
            actual: 0u128.into()
        }
    );

    // Mint funds to the emissions controller
    let funds = coins(200_000000, &astro);
    helper
        .mint_tokens(&helper.emission_controller.clone(), &funds)
        .unwrap();

    let resp = helper
        .permissioned_set_emissions(&owner, &schedules, &[])
        .unwrap();
    // Assert mocked ibc event
    let has_event = resp.has_event(
        &Event::new("transfer").add_attributes([
            attr(
                "packet_timeout_timestamp",
                helper
                    .app
                    .block_info()
                    .time
                    .plus_seconds(IBC_TIMEOUT)
                    .seconds()
                    .to_string(),
            ),
            attr("packet_src_port", "transfer"),
            attr("packet_src_channel", "channel-2"),
            attr("to_address", "emissions_controller"),
            attr("amount", coin(100_000000, &astro).to_string()),
        ]),
    );
    assert!(
        has_event,
        "Expected IBC transfer event. Actual {:?}",
        resp.events
    );

    // Check schedule in the incentives contract
    let expected_rps = Decimal256::from_ratio(100_000000u64, EPOCH_LENGTH);
    let rewards = helper.query_rewards(&pool1).unwrap();
    let epoch_start = get_epoch_start(helper.app.block_info().time.seconds());
    assert_eq!(rewards.len(), 1);
    assert_eq!(rewards[0].rps, expected_rps);
    assert_eq!(
        rewards[0].reward,
        RewardType::Ext {
            info: AssetInfo::native(&astro),
            next_update_ts: epoch_start + EPOCH_LENGTH
        }
    );
}

#[test]
fn test_voting() {
    let mut helper = ControllerHelper::new();

    let user = helper.app.api().addr_make("user");

    let err = helper
        .vote(
            &user,
            &[
                ("pool1".to_string(), Decimal::percent(1)),
                ("pool1".to_string(), Decimal::percent(1)),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::DuplicatedVotes {}
    );

    let err = helper
        .vote(
            &user,
            &[
                ("pool1".to_string(), Decimal::percent(1)),
                ("pool2".to_string(), Decimal::percent(1)),
                ("pool3".to_string(), Decimal::percent(1)),
                ("pool4".to_string(), Decimal::percent(1)),
                ("pool5".to_string(), Decimal::percent(1)),
                ("pool6".to_string(), Decimal::percent(1)),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ExceededMaxPoolsToVote {}
    );

    let err = helper
        .vote(&user, &[("pool1".to_string(), Decimal::percent(1))])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ZeroVotingPower {}
    );

    let err = helper
        .vote(
            &user,
            &[
                ("pool1".to_string(), Decimal::one()),
                ("pool2".to_string(), Decimal::one()),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidTotalWeight {}
    );

    // Until voting channel set by the owner, any vxASTRO interactions should fail
    let err = helper.lock(&user, 1000u64.into()).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: The contract does not have channel "
    );

    helper.set_voting_channel();
    helper.lock(&user, 1000u64.into()).unwrap();

    // Can't lock more until the hub acknowledges a previous message
    let err = helper.lock(&user, 1000u64.into()).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PendingUser(user.to_string())
    );

    // Mock ibc ack
    helper
        .mock_ibc_ack(
            VxAstroIbcMsg::UpdateUserVotes {
                voter: user.to_string(),
                voting_power: Default::default(),
                is_unlock: false,
            },
            None,
        )
        .unwrap();

    helper
        .vote(&user, &[("pool1".to_string(), Decimal::one())])
        .unwrap();

    // Cant do anything until the hub acknowledges the vote
    let err = helper
        .vote(&user, &[("pool1".to_string(), Decimal::one())])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PendingUser(user.to_string())
    );
    let err = helper.unlock(&user).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PendingUser(user.to_string())
    );
    let err = helper.refresh_user(&user).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PendingUser(user.to_string())
    );

    // Time out IBC packet
    let mock_packet = VxAstroIbcMsg::EmissionsVote {
        voter: user.to_string(),
        voting_power: Default::default(),
        votes: Default::default(),
    };
    helper.mock_ibc_timeout(mock_packet.clone()).unwrap();

    let ibc_status = helper.query_ibc_status(&user).unwrap();
    assert_eq!(ibc_status.pending_msg, None);
    assert_eq!(
        ibc_status.error,
        Some(UserIbcError {
            msg: mock_packet,
            err: "IBC packet timeout".to_string()
        })
    );

    helper
        .vote(&user, &[("pool1".to_string(), Decimal::one())])
        .unwrap();

    // Refreshing user with 0 voting power should fail
    let random = helper.app.api().addr_make("random");
    let err = helper.refresh_user(&random).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ZeroVotingPower {}
    );

    helper
        .mock_ibc_ack(
            VxAstroIbcMsg::EmissionsVote {
                voter: user.to_string(),
                voting_power: Default::default(),
                votes: Default::default(),
            },
            None,
        )
        .unwrap();

    // Check failed unlock

    helper.unlock(&user).unwrap();
    // Check user VP became 0
    let user_vp = helper.user_vp(&user, None).unwrap();
    assert_eq!(user_vp.u128(), 0);

    let mock_packet = VxAstroIbcMsg::UpdateUserVotes {
        voter: user.to_string(),
        voting_power: Default::default(),
        is_unlock: true,
    };
    helper
        .mock_ibc_ack(mock_packet.clone(), Some("error"))
        .unwrap();
    let ibc_status = helper.query_ibc_status(&user).unwrap();
    assert_eq!(ibc_status.pending_msg, None);
    assert_eq!(
        ibc_status.error,
        Some(UserIbcError {
            msg: mock_packet,
            err: "error".to_string()
        })
    );
    let lock_info = helper.lock_info(&user).unwrap();
    assert_eq!(
        lock_info,
        LockInfoResponse {
            amount: 1000u128.into(),
            unlock_status: None,
        }
    );
    // Ensure user VP was recovered
    let user_vp = helper.user_vp(&user, None).unwrap();
    assert_eq!(user_vp.u128(), 1000);

    // Ensure nobody but vxASTRO can call UpdateUserVotes
    let err = helper
        .app
        .execute_contract(
            user.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::UpdateUserVotes {
                user: user.to_string(),
                is_unlock: true,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );
}

#[test]
fn test_privileged_list_disabled() {
    let mut helper = ControllerHelper::new();
    let owner = helper.owner.clone();
    let user = helper.app.api().addr_make("user");

    // Must fail to deserialize outpost controller Config into Hub's controller Config
    helper
        .app
        .execute_contract(
            owner.clone(),
            helper.vxastro.clone(),
            &voting_escrow::ExecuteMsg::SetPrivilegedList {
                list: vec![user.to_string()],
            },
            &[],
        )
        .unwrap_err();
}

#[test]
fn test_unlock_and_withdraw() {
    let mut helper = ControllerHelper::new();
    let user = helper.app.api().addr_make("user");

    helper.set_voting_channel();
    helper.lock(&user, 1000u64.into()).unwrap();

    // Mock ibc ack
    helper
        .mock_ibc_ack(
            VxAstroIbcMsg::UpdateUserVotes {
                voter: user.to_string(),
                voting_power: Default::default(),
                is_unlock: false,
            },
            None,
        )
        .unwrap();

    helper.unlock(&user).unwrap();
    helper.timetravel(UNLOCK_PERIOD);

    let err = helper.withdraw(&user).unwrap_err();
    assert_eq!(
        err.downcast::<astroport_voting_escrow::error::ContractError>()
            .unwrap(),
        astroport_voting_escrow::error::ContractError::HubNotConfirmed {}
    );

    // Mock hub confirmation
    helper
        .mock_ibc_ack(
            VxAstroIbcMsg::UpdateUserVotes {
                voter: user.to_string(),
                voting_power: Default::default(),
                is_unlock: true,
            },
            None,
        )
        .unwrap();

    helper.withdraw(&user).unwrap();
    let user_bal = helper
        .app
        .wrap()
        .query_balance(&user, &helper.xastro)
        .unwrap()
        .amount
        .u128();
    assert_eq!(user_bal, 1000);
}

#[test]
fn test_interchain_governance() {
    let mut helper = ControllerHelper::new();
    helper.set_voting_channel();

    let user = helper.app.api().addr_make("user");

    // Proposal is not registered
    helper.cast_vote(&user, 1).unwrap_err();

    let now = helper.app.block_info().time.seconds();

    let err = helper
        .mock_packet_receive(
            VxAstroIbcMsg::RegisterProposal {
                proposal_id: 1,
                start_time: now - 10,
            },
            "channel-100",
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Invalid channel"
    );

    helper
        .mock_packet_receive(
            VxAstroIbcMsg::RegisterProposal {
                proposal_id: 1,
                start_time: now,
            },
            "channel-1",
        )
        .unwrap();

    assert!(
        helper.is_prop_registered(1),
        "Proposal should be registered"
    );

    let err = helper
        .mock_packet_receive(
            VxAstroIbcMsg::RegisterProposal {
                proposal_id: 1,
                start_time: now,
            },
            "channel-1",
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Proposal already registered"
    );

    let err = helper.cast_vote(&user, 1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ZeroVotingPower {}
    );

    helper.lock(&user, 1000u64.into()).unwrap();

    // User locked after proposal registration. Still zero voting power
    let err = helper.cast_vote(&user, 1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ZeroVotingPower {}
    );

    helper.timetravel(100);

    let now = helper.app.block_info().time.seconds();

    helper
        .mock_packet_receive(
            VxAstroIbcMsg::RegisterProposal {
                proposal_id: 2,
                start_time: now,
            },
            "channel-1",
        )
        .unwrap();

    let err = helper.cast_vote(&user, 2).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PendingUser(user.to_string())
    );

    // Mock ibc ack
    helper
        .mock_ibc_ack(
            VxAstroIbcMsg::UpdateUserVotes {
                voter: user.to_string(),
                voting_power: Default::default(),
                is_unlock: false,
            },
            None,
        )
        .unwrap();

    helper.cast_vote(&user, 2).unwrap();

    // Timeout voting packet
    helper
        .mock_ibc_timeout(VxAstroIbcMsg::GovernanceVote {
            voter: user.to_string(),
            voting_power: Default::default(),
            proposal_id: 2,
            vote: ProposalVoteOption::For,
        })
        .unwrap();

    helper.cast_vote(&user, 2).unwrap();

    // Mock ack
    helper
        .mock_ibc_ack(
            VxAstroIbcMsg::GovernanceVote {
                voter: user.to_string(),
                voting_power: Default::default(),
                proposal_id: 2,
                vote: ProposalVoteOption::For,
            },
            None,
        )
        .unwrap();

    // Can't vote again
    let err = helper.cast_vote(&user, 2).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::AlreadyVoted {}
    );

    let voters = helper
        .app
        .wrap()
        .query_wasm_smart::<Vec<String>>(
            &helper.emission_controller,
            &emissions_controller::outpost::QueryMsg::QueryProposalVoters {
                proposal_id: 2,
                limit: Some(100),
                start_after: None,
            },
        )
        .unwrap();
    assert_eq!(voters, vec![user.to_string()]);
}

#[test]
fn test_update_config() {
    let mut helper = ControllerHelper::new();
    let owner = helper.owner.clone();

    // Unauthorized check
    let random = helper.app.api().addr_make("random");
    let err = helper.update_config(&random, None, None, None).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    let err = helper
        .update_config(&owner, Some("channel-100".to_string()), None, None)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: The contract does not have channel channel-100"
    );

    helper
        .update_config(
            &owner,
            Some("channel-1".to_string()),
            Some("hub_emissions_controller".to_string()),
            Some("channel-10".to_string()),
        )
        .unwrap();
    let config = helper.query_config().unwrap();
    assert_eq!(config.voting_ibc_channel, "channel-1");
    assert_eq!(config.hub_emissions_controller, "hub_emissions_controller");
    assert_eq!(config.ics20_channel, "channel-10");
}

#[test]
fn test_change_ownership() {
    let mut helper = ControllerHelper::new();

    let new_owner = helper.app.api().addr_make("new_owner");

    // New owner
    let msg = ExecuteMsg::<Empty>::ProposeNewOwner {
        new_owner: new_owner.to_string(),
        expires_in: 100, // seconds
    };

    // Unauthorized check
    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("not_owner"),
            helper.emission_controller.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose a new owner
    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.emission_controller.clone(),
            &msg,
            &[],
        )
        .unwrap();

    // Claim from invalid addr
    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("invalid_addr"),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop the ownership proposal
    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::DropOwnershipProposal {},
            &[],
        )
        .unwrap();

    // Claim ownership
    let err = helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose a new owner again
    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.emission_controller.clone(),
            &msg,
            &[],
        )
        .unwrap();
    helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::ClaimOwnership {},
            &[],
        )
        .unwrap();

    assert_eq!(helper.query_config().unwrap().owner.to_string(), new_owner)
}
