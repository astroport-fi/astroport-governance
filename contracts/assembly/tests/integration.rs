use std::collections::HashMap;
use std::str::FromStr;

use cosmwasm_std::{coin, coins, Addr, BankMsg, Decimal, Uint128};
use cw_multi_test::Executor;

use astro_assembly::error::ContractError;
use astroport_governance::assembly::{
    Config, ExecuteMsg, InstantiateMsg, ProposalListResponse, ProposalStatus, ProposalVoteOption,
    ProposalVoterResponse, QueryMsg, UpdateConfig, DELAY_INTERVAL, DEPOSIT_INTERVAL,
    EXPIRATION_PERIOD_INTERVAL, VOTING_PERIOD_INTERVAL,
};

use crate::common::helper::{
    default_init_msg, Helper, PROPOSAL_DELAY, PROPOSAL_EXPIRATION, PROPOSAL_REQUIRED_DEPOSIT,
    PROPOSAL_VOTING_PERIOD,
};

mod common;

#[test]
fn test_contract_instantiation() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

    let assembly_code = helper.assembly_code_id;
    let staking = helper.staking.clone();
    let builder_unlock = helper.builder_unlock.clone();

    // Try to instantiate assembly with wrong threshold
    let err = helper
        .app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_required_threshold: "0.3".to_string(),
                ..default_init_msg(&staking, &builder_unlock)
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: The required threshold for a proposal cannot be lower than 33% or higher than 100%"
    );

    let err = helper
        .app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_required_threshold: "1.1".to_string(),
                ..default_init_msg(&staking, &builder_unlock)
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: The required threshold for a proposal cannot be lower than 33% or higher than 100%"
    );

    let err = helper
        .app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_required_quorum: "1.1".to_string(),
                ..default_init_msg(&staking, &builder_unlock)
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: The required quorum for a proposal cannot be lower than 1% or higher than 100%"
    );

    let err = helper
        .app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_expiration_period: 500,
                ..default_init_msg(&staking, &builder_unlock)
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        format!("Generic error: The expiration period for a proposal cannot be lower than {} or higher than {}", EXPIRATION_PERIOD_INTERVAL.start(), EXPIRATION_PERIOD_INTERVAL.end())
    );

    let err = helper
        .app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_effective_delay: 400,
                ..default_init_msg(&staking, &builder_unlock)
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        format!("Generic error: The effective delay for a proposal cannot be lower than {} or higher than {}", DELAY_INTERVAL.start(), DELAY_INTERVAL.end())
    );

    let err = helper
        .app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                whitelisted_links: vec![],
                ..default_init_msg(&staking, &builder_unlock)
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::WhitelistEmpty {}
    );

    let assembly_instance = helper
        .app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &default_init_msg(&staking, &builder_unlock),
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap();

    let res: Config = helper
        .app
        .wrap()
        .query_wasm_smart(assembly_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(res.xastro_denom, helper.xastro_denom);
    assert_eq!(res.builder_unlock_addr, helper.builder_unlock);
    assert_eq!(
        res.whitelisted_links,
        vec!["https://some.link/".to_string(),]
    );
}

#[test]
fn test_proposal_lifecycle() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

    let user = Addr::unchecked("user");
    helper.get_xastro(&user, 2 * PROPOSAL_REQUIRED_DEPOSIT.u128() + 1000); // initial stake consumes 1000 xASTRO
    let late_voter = Addr::unchecked("late_voter");
    helper.get_xastro(&late_voter, 2 * PROPOSAL_REQUIRED_DEPOSIT.u128());

    helper.next_block(10);

    helper.submit_sample_proposal(&user);

    // Check voting power
    assert_eq!(
        helper.user_vp(&user, 1).u128(),
        2 * PROPOSAL_REQUIRED_DEPOSIT.u128()
    );
    assert_eq!(
        helper.user_vp(&late_voter, 1).u128(),
        2 * PROPOSAL_REQUIRED_DEPOSIT.u128()
    );
    assert_eq!(
        helper.proposal_total_vp(1).unwrap().u128(),
        4 * PROPOSAL_REQUIRED_DEPOSIT.u128() + 1000 // 1000 locked forever in the staking contract
    );

    // Unstake after proposal submission
    helper
        .unstake(&user, PROPOSAL_REQUIRED_DEPOSIT.u128())
        .unwrap();
    // Current voting power is 0
    assert_eq!(helper.query_xastro_bal_at(&user, None), Uint128::zero());

    // However voting power for the 1st proposal is still == 2 * PROPOSAL_REQUIRED_DEPOSIT
    assert_eq!(
        helper.user_vp(&user, 1).u128(),
        2 * PROPOSAL_REQUIRED_DEPOSIT.u128()
    );

    helper.cast_vote(1, &user, ProposalVoteOption::For).unwrap();

    // One more voter got voting power in the middle of voting period.
    // His voting power as well as total xASTRO supply increase are not accounted at the proposal start block.
    let behind_voter = Addr::unchecked("behind_voter");
    helper.get_xastro(&behind_voter, 20 * PROPOSAL_REQUIRED_DEPOSIT.u128());
    let err = helper
        .cast_vote(1, &behind_voter, ProposalVoteOption::For)
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoVotingPower {}
    );

    helper.next_block(10);

    // Try to vote again
    let err = helper
        .cast_vote(1, &user, ProposalVoteOption::For)
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::UserAlreadyVoted {}
    );

    // Try to vote without voting power
    let err = helper
        .cast_vote(1, &Addr::unchecked("stranger"), ProposalVoteOption::Against)
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoVotingPower {}
    );

    // Try to end proposal
    let err = helper.end_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::VotingPeriodNotEnded {}
    );

    // Try to execute proposal
    let err = helper.execute_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotPassed {}
    );

    helper.next_block_height(PROPOSAL_VOTING_PERIOD);

    // Late voter tries to vote after voting period
    let err = helper
        .cast_vote(1, &late_voter, ProposalVoteOption::Against)
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::VotingPeriodEnded {}
    );

    // Try to execute proposal before it is ended
    let err = helper.execute_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotPassed {}
    );

    helper.end_proposal(1).unwrap();

    // Try to end proposal again
    let err = helper.end_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotActive {}
    );

    // Submitter received his deposit back
    assert_eq!(
        helper.query_balance(&user, &helper.xastro_denom).unwrap(),
        PROPOSAL_REQUIRED_DEPOSIT
    );

    // Try to execute proposal before the delay is ended
    let err = helper.execute_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalDelayNotEnded {}
    );

    // Late voter has no chance to vote
    let err = helper
        .cast_vote(1, &late_voter, ProposalVoteOption::Against)
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotActive {}
    );

    helper.next_block_height(PROPOSAL_DELAY);

    // Finally execute proposal
    helper.execute_proposal(1).unwrap();

    // Try to execute proposal again
    let err = helper.execute_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotPassed {}
    );
    // Try to end proposal
    let err = helper.end_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotActive {}
    );

    // Ensure proposal message was executed
    assert_eq!(
        helper.query_balance("receiver", "some_coin").unwrap(),
        Uint128::one()
    );
}

#[test]
fn test_rejected_proposal() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

    let user = Addr::unchecked("user");
    helper.get_xastro(&user, PROPOSAL_REQUIRED_DEPOSIT.u128() + 1000); // initial stake consumes 1000 xASTRO

    helper.next_block(10);

    // Proposal messages contain one simple transfer
    let assembly = helper.assembly.clone();
    helper.mint_coin(&assembly, coin(1, "some_coin"));
    helper.submit_proposal(
        &user,
        vec![BankMsg::Send {
            to_address: "receiver".to_string(),
            amount: coins(1, "some_coin"),
        }
        .into()],
    );

    helper
        .cast_vote(1, &user, ProposalVoteOption::Against)
        .unwrap();

    helper.next_block(10);

    // Try to vote again
    let err = helper
        .cast_vote(1, &user, ProposalVoteOption::For)
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::UserAlreadyVoted {}
    );

    helper.next_block_height(PROPOSAL_VOTING_PERIOD);

    helper.end_proposal(1).unwrap();

    // Try to end proposal again
    let err = helper.end_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotActive {}
    );

    // Submitter received his deposit back
    assert_eq!(
        helper.query_balance(&user, &helper.xastro_denom).unwrap(),
        PROPOSAL_REQUIRED_DEPOSIT
    );

    // Try to execute proposal. It should be rejected.
    let err = helper.execute_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotPassed {}
    );

    helper.next_block_height(PROPOSAL_DELAY);

    // Try to execute proposal after delay (which doesn't make sense in reality)
    let err = helper.execute_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotPassed {}
    );

    // Try to end proposal
    let err = helper.end_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposalNotActive {}
    );

    // Ensure proposal message was not executed
    assert_eq!(
        helper.query_balance("receiver", "some_coin").unwrap(),
        Uint128::zero()
    );
}

#[test]
fn test_expired_proposal() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

    let user = Addr::unchecked("user");
    helper.get_xastro(&user, PROPOSAL_REQUIRED_DEPOSIT.u128() + 1000); // initial stake consumes 1000 xASTRO

    helper.next_block(10);

    // Proposal messages coins one simple transfer
    let assembly = helper.assembly.clone();
    helper.mint_coin(&assembly, coin(1, "some_coin"));
    helper.submit_proposal(
        &user,
        vec![BankMsg::Send {
            to_address: "receiver".to_string(),
            amount: coins(1, "some_coin"),
        }
        .into()],
    );

    helper.cast_vote(1, &user, ProposalVoteOption::For).unwrap();

    helper.next_block_height(PROPOSAL_VOTING_PERIOD + PROPOSAL_DELAY + PROPOSAL_EXPIRATION + 1);

    helper.end_proposal(1).unwrap();

    // Submitter received his deposit back
    assert_eq!(
        helper.query_balance(&user, &helper.xastro_denom).unwrap(),
        PROPOSAL_REQUIRED_DEPOSIT
    );

    // Try to execute proposal. It should be rejected.
    let err = helper.execute_proposal(1).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ExecuteProposalExpired {}
    );

    // Ensure proposal message was not executed
    assert_eq!(
        helper.query_balance("receiver", "some_coin").unwrap(),
        Uint128::zero()
    );
}

#[test]
fn test_check_messages() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

    // Prepare for check messages
    let assembly = helper.assembly.clone();
    helper.mint_coin(&assembly, coin(1, "some_coin"));

    // Valid message
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("permissionless"),
            assembly.clone(),
            &ExecuteMsg::CheckMessages(vec![BankMsg::Send {
                to_address: "receiver".to_string(),
                amount: coins(1, "some_coin"),
            }
            .into()]),
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::MessagesCheckPassed {}
    );

    // Invalid message
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("permissionless"),
            assembly.clone(),
            &ExecuteMsg::CheckMessages(vec![BankMsg::Send {
                to_address: "receiver".to_string(),
                amount: coins(1000, "uusdc"),
            }
            .into()]),
            &[],
        )
        .unwrap_err();
    // The error must be different
    assert_ne!(
        err.root_cause().to_string(),
        ContractError::MessagesCheckPassed {}.to_string()
    );
}

#[test]
fn test_update_config() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();
    let assembly = helper.assembly.clone();

    let err = helper
        .app
        .execute_contract(
            owner.clone(),
            assembly.clone(),
            &ExecuteMsg::UpdateConfig(Box::new(UpdateConfig {
                xastro_denom: None,
                ibc_controller: None,
                builder_unlock_addr: None,
                proposal_voting_period: None,
                proposal_effective_delay: None,
                proposal_expiration_period: None,
                proposal_required_deposit: None,
                proposal_required_quorum: None,
                proposal_required_threshold: None,
                whitelist_remove: None,
                whitelist_add: None,
            })),
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    let updated_config = UpdateConfig {
        xastro_denom: Some("test".to_string()),
        ibc_controller: Some("ibc_controller".to_string()),
        builder_unlock_addr: Some("builder_unlock".to_string()),
        proposal_voting_period: Some(*VOTING_PERIOD_INTERVAL.end()),
        proposal_effective_delay: Some(*DELAY_INTERVAL.end()),
        proposal_expiration_period: Some(*EXPIRATION_PERIOD_INTERVAL.end()),
        proposal_required_deposit: Some(*DEPOSIT_INTERVAL.end()),
        proposal_required_quorum: Some("0.5".to_string()),
        proposal_required_threshold: Some("0.5".to_string()),
        whitelist_remove: Some(vec!["https://some.link/".to_string()]),
        whitelist_add: Some(vec!["https://another.link/".to_string()]),
    };

    helper
        .app
        .execute_contract(
            assembly.clone(), // only assembly itself can update config
            assembly.clone(),
            &ExecuteMsg::UpdateConfig(Box::new(updated_config)),
            &[],
        )
        .unwrap();

    let config: Config = helper
        .app
        .wrap()
        .query_wasm_smart(assembly, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(config.xastro_denom, "test");
    assert_eq!(
        config.ibc_controller,
        Some(Addr::unchecked("ibc_controller"))
    );
    assert_eq!(
        config.builder_unlock_addr,
        Addr::unchecked("builder_unlock")
    );
    assert_eq!(config.proposal_voting_period, *VOTING_PERIOD_INTERVAL.end());
    assert_eq!(config.proposal_effective_delay, *DELAY_INTERVAL.end());
    assert_eq!(
        config.proposal_expiration_period,
        *EXPIRATION_PERIOD_INTERVAL.end()
    );
    assert_eq!(
        config.proposal_required_deposit,
        Uint128::new(*DEPOSIT_INTERVAL.end())
    );
    assert_eq!(
        config.proposal_required_quorum,
        Decimal::from_str("0.5").unwrap()
    );
    assert_eq!(
        config.proposal_required_threshold,
        Decimal::from_str("0.5").unwrap()
    );
    assert_eq!(
        config.whitelisted_links,
        vec!["https://another.link/".to_string()]
    );
}

#[test]
fn test_voting_power() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();

    helper.get_xastro(&owner, 1001u64);

    struct TestBalance {
        xastro: u128,
        builder_allocation: u128,
    }

    let mut total_xastro = 0u128;
    let mut total_builder_allocation = 0u128;

    let users_num = 100;
    let balances: HashMap<Addr, TestBalance> = (1..=users_num)
        .map(|i| {
            let user = Addr::unchecked(format!("user{i}"));
            let balances = TestBalance {
                xastro: i * 1_000000,
                builder_allocation: if i % 2 == 0 { i * 1_000000 } else { 0 },
            };
            helper.get_xastro(&user, balances.xastro);
            if balances.builder_allocation > 0 {
                helper.create_builder_allocation(&user, balances.builder_allocation);
            }

            total_xastro += balances.xastro;
            total_builder_allocation += balances.builder_allocation;

            (user, balances)
        })
        .collect();

    let submitter = balances.iter().last().unwrap().0;
    helper.get_xastro(submitter, PROPOSAL_REQUIRED_DEPOSIT.u128());
    total_xastro += PROPOSAL_REQUIRED_DEPOSIT.u128();

    helper.next_block(10);

    helper.submit_sample_proposal(submitter);

    let proposal = helper.proposal(1);
    assert_eq!(
        proposal.total_voting_power.u128(),
        total_xastro + total_builder_allocation + 1001
    );

    // First 40 users vote against the proposal
    let mut against_power = 0u128;
    balances.iter().take(40).for_each(|(addr, balances)| {
        helper.next_block(100);
        against_power += balances.xastro + balances.builder_allocation;
        helper
            .cast_vote(1, addr, ProposalVoteOption::Against)
            .unwrap();
    });

    let proposal = helper.proposal(1);
    assert_eq!(proposal.against_power.u128(), against_power);

    // Next 40 vote for the proposal
    let mut for_power = 0u128;
    balances
        .iter()
        .skip(40)
        .take(40)
        .for_each(|(addr, balances)| {
            helper.next_block(100);
            for_power += balances.xastro + balances.builder_allocation;
            helper.cast_vote(1, addr, ProposalVoteOption::For).unwrap();
        });

    let proposal = helper.proposal(1);
    assert_eq!(proposal.for_power.u128(), for_power);

    // Total voting power stays the same
    let proposal = helper.proposal(1);
    assert_eq!(
        proposal.total_voting_power.u128(),
        total_xastro + total_builder_allocation + 1001
    );

    helper.next_block_height(PROPOSAL_VOTING_PERIOD);

    helper.end_proposal(1).unwrap();

    let proposal = helper.proposal(1);

    assert_eq!(
        proposal.total_voting_power.u128(),
        total_xastro + total_builder_allocation + 1001
    );
    assert_eq!(proposal.submitter, submitter.clone());
    assert_eq!(proposal.status, ProposalStatus::Passed);
    assert_eq!(proposal.for_power.u128(), for_power);
    assert_eq!(proposal.against_power.u128(), against_power);

    let proposal_votes = helper.proposal_votes(1);
    assert_eq!(proposal_votes.for_power.u128(), for_power);
    assert_eq!(proposal_votes.against_power.u128(), against_power);
}

#[test]
fn test_queries() {
    let owner = Addr::unchecked("owner");
    let mut helper = Helper::new(&owner).unwrap();
    let assembly = helper.assembly.clone();

    helper.get_xastro(&owner, 10 * PROPOSAL_REQUIRED_DEPOSIT.u128() + 1000);

    for i in 1..=10 {
        helper.next_block(100);
        helper.submit_sample_proposal(&owner);
        helper
            .cast_vote(i, &owner, ProposalVoteOption::For)
            .unwrap();
    }

    let proposal_voters = helper.proposal_voters(5);
    assert_eq!(
        proposal_voters,
        [ProposalVoterResponse {
            address: owner.to_string(),
            vote_option: ProposalVoteOption::For
        }]
    );

    let proposals = helper
        .app
        .wrap()
        .query_wasm_smart::<ProposalListResponse>(
            &assembly,
            &QueryMsg::Proposals {
                start: None,
                limit: None,
            },
        )
        .unwrap()
        .proposal_list;

    assert_eq!(proposals.len(), 10);
}
