use astro_assembly::error::ContractError;
use cosmwasm_std::{coin, coins, Addr, BankMsg, Uint128};
use cw_multi_test::Executor;

use astroport_governance::assembly::{Config, InstantiateMsg, ProposalVoteOption, QueryMsg};

use crate::common::helper::{
    default_init_msg, Helper, PROPOSAL_DELAY, PROPOSAL_REQUIRED_DEPOSIT, PROPOSAL_VOTING_PERIOD,
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
        "Generic error: The expiration period for a proposal cannot be lower than 12342 or higher than 100800"
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
        "Generic error: The effective delay for a proposal cannot be lower than 6171 or higher than 14400"
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

// #[test]
// fn test_successful_proposal() {
//     let owner = Addr::unchecked("owner");
//     let mut helper = Helper::new(&owner).unwrap();
//
//     // Init voting power for users
//     let balances: Vec<(&str, u128, u128)> = vec![
//         ("user0", PROPOSAL_REQUIRED_DEPOSIT.u128(), 0), // proposal submitter
//         ("user1", 20, 80),
//         ("user2", 100, 100),
//         ("user3", 300, 100),
//         ("user4", 200, 50),
//         ("user5", 0, 90),
//         ("user6", 100, 200),
//         ("user7", 30, 0),
//         ("user8", 80, 100),
//         ("user9", 50, 0),
//         ("user10", 0, 90),
//         ("user11", 500, 0),
//         ("user12", 10000_000000, 0),
//     ];
//
//     let default_allocation_params = AllocationParams {
//         amount: Uint128::zero(),
//         unlock_schedule: Schedule {
//             start_time: 12_345,
//             cliff: 5,
//             duration: 500,
//             percent_at_cliff: None,
//         },
//         proposed_receiver: None,
//     };
//
//     let locked_balances = vec![
//         (
//             "user1".to_string(),
//             AllocationParams {
//                 amount: Uint128::from(80u32),
//                 ..default_allocation_params.clone()
//             },
//         ),
//         (
//             "user4".to_string(),
//             AllocationParams {
//                 amount: Uint128::from(50u32),
//                 ..default_allocation_params.clone()
//             },
//         ),
//         (
//             "user7".to_string(),
//             AllocationParams {
//                 amount: Uint128::from(100u32),
//                 ..default_allocation_params.clone()
//             },
//         ),
//         (
//             "user10".to_string(),
//             AllocationParams {
//                 amount: Uint128::from(30u32),
//                 ..default_allocation_params
//             },
//         ),
//     ];
//
//     for (addr, xastro, vxastro) in balances {
//         if xastro > 0 {
//             helper.mint_tokens(&Addr::unchecked(addr), xastro);
//         }
//
//         if vxastro > 0 {
//             helper.mint_vxastro(&Addr::unchecked(addr), vxastro);
//         }
//     }
//
//     helper.create_allocations(locked_balances);
//
//     // Skip period
//     helper.app.update_block(|mut block| {
//         block.time = block.time.plus_seconds(WEEK);
//         block.height += WEEK / 5;
//     });
//
//     // Create default proposal
//     helper.create_proposal(&Addr::unchecked("user0"), vec![]);
//
//     let votes: Vec<(&str, ProposalVoteOption, u128)> = vec![
//         ("user1", ProposalVoteOption::For, 180u128),
//         ("user2", ProposalVoteOption::For, 200u128),
//         ("user3", ProposalVoteOption::For, 400u128),
//         ("user4", ProposalVoteOption::For, 300u128),
//         ("user5", ProposalVoteOption::For, 90u128),
//         ("user6", ProposalVoteOption::For, 300u128),
//         ("user7", ProposalVoteOption::For, 130u128),
//         ("user8", ProposalVoteOption::Against, 180u128),
//         ("user9", ProposalVoteOption::Against, 50u128),
//         ("user10", ProposalVoteOption::Against, 120u128),
//         ("user11", ProposalVoteOption::Against, 500u128),
//         ("user12", ProposalVoteOption::For, 10000_000000u128),
//     ];
//
//     let prop_vp = helper.proposal_total_vp(1).unwrap();
//     assert_eq!(prop_vp, 20000002450u128.into());
//
//     for (addr, option, expected_vp) in votes {
//         let sender = Addr::unchecked(addr);
//
//         let vp = helper.user_vp(&sender, 1);
//         assert_eq!(vp, expected_vp.into());
//
//         helper.cast_vote(1, sender, option).unwrap();
//     }
//
//     let proposal = helper.proposal(1);
//
//     let proposal_votes = helper.proposal_votes(1);
//
//     let proposal_for_voters = helper
//         .proposal_voters(1)
//         .into_iter()
//         .filter(|v| v.vote_option == ProposalVoteOption::For)
//         .collect::<Vec<_>>();
//
//     let proposal_against_voters = helper
//         .proposal_voters(1)
//         .into_iter()
//         .filter(|v| v.vote_option == ProposalVoteOption::Against)
//         .collect::<Vec<_>>();
//
//     // Check proposal votes
//     assert_eq!(proposal.for_power, Uint128::from(10000001600u128));
//     assert_eq!(proposal.against_power, Uint128::from(850u32));
//
//     assert_eq!(proposal_votes.for_power, Uint128::from(10000001600u128));
//     assert_eq!(proposal_votes.against_power, Uint128::from(850u32));
//
//     assert_eq!(
//         proposal_for_voters,
//         vec![
//             Addr::unchecked("user1"),
//             Addr::unchecked("user2"),
//             Addr::unchecked("user3"),
//             Addr::unchecked("user4"),
//             Addr::unchecked("user5"),
//             Addr::unchecked("user6"),
//             Addr::unchecked("user7"),
//             Addr::unchecked("user12"),
//         ]
//     );
//     assert_eq!(
//         proposal_against_voters,
//         vec![
//             Addr::unchecked("user8"),
//             Addr::unchecked("user9"),
//             Addr::unchecked("user10"),
//             Addr::unchecked("user11")
//         ]
//     );
//
//     // Skip voting period
//     helper.app.update_block(|bi| {
//         bi.height += PROPOSAL_VOTING_PERIOD + 1;
//         bi.time = bi.time.plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + 1));
//     });
//
//     // Try to vote after voting period
//     let err = helper
//         .cast_vote(1, Addr::unchecked("user11"), ProposalVoteOption::Against)
//         .unwrap_err();
//
//     assert_eq!(
//         err.downcast::<ContractError>().unwrap(),
//         ContractError::VotingPeriodEnded {}
//     );
//
//     // Try to execute the proposal before end_proposal
//     let err = helper
//         .app
//         .execute_contract(
//             Addr::unchecked("user0"),
//             helper.assembly.clone(),
//             &ExecuteMsg::ExecuteProposal { proposal_id: 1 },
//             &[],
//         )
//         .unwrap_err();
//
//     assert_eq!(
//         err.downcast::<ContractError>().unwrap(),
//         ContractError::ProposalNotPassed {}
//     );
//
//     // Check the successful completion of the proposal
//     check_token_balance(&mut app, &xastro_addr, &Addr::unchecked("user0"), 0);
//
//     app.execute_contract(
//         Addr::unchecked("user0"),
//         assembly_addr.clone(),
//         &ExecuteMsg::EndProposal { proposal_id: 1 },
//         &[],
//     )
//     .unwrap();
//
//     check_token_balance(
//         &mut app,
//         &xastro_addr,
//         &Addr::unchecked("user0"),
//         PROPOSAL_REQUIRED_DEPOSIT,
//     );
//
//     let proposal: Proposal = app
//         .wrap()
//         .query_wasm_smart(
//             assembly_addr.clone(),
//             &QueryMsg::Proposal { proposal_id: 1 },
//         )
//         .unwrap();
//
//     assert_eq!(proposal.status, ProposalStatus::Passed);
//
//     // Try to end proposal again
//     let err = app
//         .execute_contract(
//             Addr::unchecked("user0"),
//             assembly_addr.clone(),
//             &ExecuteMsg::EndProposal { proposal_id: 1 },
//             &[],
//         )
//         .unwrap_err();
//
//     assert_eq!(err.root_cause().to_string(), "Proposal not active!");
//
//     // Try to execute the proposal before the delay
//     let err = app
//         .execute_contract(
//             Addr::unchecked("user0"),
//             assembly_addr.clone(),
//             &ExecuteMsg::ExecuteProposal { proposal_id: 1 },
//             &[],
//         )
//         .unwrap_err();
//
//     assert_eq!(err.root_cause().to_string(), "Proposal delay not ended!");
//
//     // Skip blocks
//     app.update_block(|bi| {
//         bi.height += PROPOSAL_EFFECTIVE_DELAY + 1;
//         bi.time = bi.time.plus_seconds(5 * (PROPOSAL_EFFECTIVE_DELAY + 1));
//     });
//
//     // Try to execute the proposal after the delay
//     app.execute_contract(
//         Addr::unchecked("user0"),
//         assembly_addr.clone(),
//         &ExecuteMsg::ExecuteProposal { proposal_id: 1 },
//         &[],
//     )
//     .unwrap();
//
//     let config: Config = app
//         .wrap()
//         .query_wasm_smart(assembly_addr.to_string(), &QueryMsg::Config {})
//         .unwrap();
//
//     let proposal: Proposal = app
//         .wrap()
//         .query_wasm_smart(
//             assembly_addr.to_string(),
//             &QueryMsg::Proposal { proposal_id: 1 },
//         )
//         .unwrap();
//
//     // Check execution result
//     assert_eq!(config.proposal_voting_period, PROPOSAL_VOTING_PERIOD + 1000);
//     assert_eq!(
//         config.whitelisted_links,
//         vec![
//             "https://some1.link/".to_string(),
//             "https://some2.link/".to_string(),
//         ]
//     );
//     assert_eq!(proposal.status, ProposalStatus::Executed);
//
//     // Try to remove proposal before expiration period
//     let err = app
//         .execute_contract(
//             Addr::unchecked("user0"),
//             assembly_addr.clone(),
//             &ExecuteMsg::RemoveCompletedProposal { proposal_id: 1 },
//             &[],
//         )
//         .unwrap_err();
//
//     assert_eq!(err.root_cause().to_string(), "Proposal not completed!");
//
//     // Remove expired proposal
//     app.update_block(|bi| {
//         bi.height += PROPOSAL_EXPIRATION_PERIOD + 1;
//         bi.time = bi.time.plus_seconds(5 * (PROPOSAL_EXPIRATION_PERIOD + 1));
//     });
//
//     app.execute_contract(
//         Addr::unchecked("user0"),
//         assembly_addr.clone(),
//         &ExecuteMsg::RemoveCompletedProposal { proposal_id: 1 },
//         &[],
//     )
//     .unwrap();
//
//     let res: ProposalListResponse = app
//         .wrap()
//         .query_wasm_smart(
//             assembly_addr.to_string(),
//             &QueryMsg::Proposals {
//                 start: None,
//                 limit: None,
//             },
//         )
//         .unwrap();
//
//     assert_eq!(res.proposal_list, vec![]);
//     // proposal_count should not be changed after removing a proposal
//     assert_eq!(res.proposal_count, Uint64::from(1u32));
// }
//
// #[test]
// fn test_successful_emissions_proposal() {
//     use cosmwasm_std::{coins, BankMsg};
//
//     let mut app = mock_app();
//     let owner = Addr::unchecked("generator_controller");
//
//     let (_, _, _, _, _, assembly_addr, _, _) = instantiate_contracts(&mut app, owner, true, false);
//
//     // Provide some funds to the Assembly contract to use in the proposal messages
//     app.init_modules(|router, _, storage| {
//         router.bank.init_balance(
//             storage,
//             &Addr::unchecked(assembly_addr.clone()),
//             coins(1000, "uluna"),
//         )
//     })
//     .unwrap();
//
//     let emissions_proposal_msg = ExecuteMsg::ExecuteEmissionsProposal {
//         title: "Emissions Test title".to_string(),
//         description: "Emissions Test description".to_string(),
//         // Sample message to use as we don't have IBC or the Generator to set emissions on
//         messages: vec![CosmosMsg::Bank(BankMsg::Send {
//             to_address: "generator_controller".into(),
//             amount: coins(1, "uluna"),
//         })],
//         ibc_channel: None,
//     };
//
//     app.execute_contract(
//         Addr::unchecked("generator_controller"),
//         assembly_addr.clone(),
//         &emissions_proposal_msg,
//         &[],
//     )
//     .unwrap();
//
//     let proposal: Proposal = app
//         .wrap()
//         .query_wasm_smart(assembly_addr, &QueryMsg::Proposal { proposal_id: 1 })
//         .unwrap();
//
//     assert_eq!(proposal.status, ProposalStatus::Executed);
// }
//
// #[test]
// fn test_no_generator_controller_emissions_proposal() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("generator_controller");
//
//     let (_, _, _, _, _, assembly_addr, _, _) = instantiate_contracts(&mut app, owner, false, false);
//     let emissions_proposal_msg = ExecuteMsg::ExecuteEmissionsProposal {
//         title: "Emissions Test title!".to_string(),
//         description: "Emissions Test description!".to_string(),
//         messages: vec![],
//         ibc_channel: None,
//     };
//
//     let err = app
//         .execute_contract(
//             Addr::unchecked("generator_controller"),
//             assembly_addr,
//             &emissions_proposal_msg,
//             &[],
//         )
//         .unwrap_err();
//
//     assert_eq!(
//         err.root_cause().to_string(),
//         "Sender is not the Generator controller installed in the assembly"
//     );
// }
//
// #[test]
// fn test_empty_messages_emissions_proposal() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("generator_controller");
//
//     let (_, _, _, _, _, assembly_addr, _, _) = instantiate_contracts(&mut app, owner, true, false);
//     let emissions_proposal_msg = ExecuteMsg::ExecuteEmissionsProposal {
//         title: "Emissions Test title!".to_string(),
//         description: "Emissions Test description!".to_string(),
//         messages: vec![],
//         ibc_channel: None,
//     };
//
//     let err = app
//         .execute_contract(
//             Addr::unchecked("generator_controller"),
//             assembly_addr,
//             &emissions_proposal_msg,
//             &[],
//         )
//         .unwrap_err();
//
//     assert_eq!(
//         err.root_cause().to_string(),
//         "The proposal has no messages to execute"
//     );
// }
//
// #[test]
// fn test_unauthorised_emissions_proposal() {
//     use cosmwasm_std::BankMsg;
//
//     let mut app = mock_app();
//     let owner = Addr::unchecked("generator_controller");
//
//     let (_, _, _, _, _, assembly_addr, _, _) = instantiate_contracts(&mut app, owner, true, false);
//     let emissions_proposal_msg = ExecuteMsg::ExecuteEmissionsProposal {
//         title: "Emissions Test title!".to_string(),
//         description: "Emissions Test description!".to_string(),
//         // Sample message to use as we don't have IBC or the Generator to set emissions on
//         messages: vec![CosmosMsg::Bank(BankMsg::Send {
//             to_address: "generator_controller".into(),
//             amount: coins(1, "uluna"),
//         })],
//         ibc_channel: None,
//     };
//
//     let err = app
//         .execute_contract(
//             Addr::unchecked("not_generator_controller"),
//             assembly_addr,
//             &emissions_proposal_msg,
//             &[],
//         )
//         .unwrap_err();
//
//     assert_eq!(err.root_cause().to_string(), "Unauthorized");
// }
//
// #[test]
// fn test_voting_power_changes() {
//     let mut app = mock_app();
//
//     let owner = Addr::unchecked("owner");
//
//     let (_, staking_instance, xastro_addr, _, _, assembly_addr, _, _) =
//         instantiate_contracts(&mut app, owner, false, false);
//
//     // Mint tokens for submitting proposal
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user0"),
//         PROPOSAL_REQUIRED_DEPOSIT,
//     );
//
//     // Mint tokens for casting votes at start block
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user1"),
//         40000_000000,
//     );
//
//     app.update_block(|mut block| {
//         block.time = block.time.plus_seconds(WEEK);
//         block.height += WEEK / 5;
//     });
//
//     // Create proposal
//     create_proposal(
//         &mut app,
//         &xastro_addr,
//         &assembly_addr,
//         Addr::unchecked("user0"),
//         Some(vec![CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: assembly_addr.to_string(),
//             msg: to_json_binary(&ExecuteMsg::UpdateConfig(Box::new(UpdateConfig {
//                 xastro_token_addr: None,
//                 vxastro_token_addr: None,
//                 voting_escrow_delegator_addr: None,
//                 ibc_controller: None,
//                 generator_controller: None,
//                 hub: None,
//                 builder_unlock_addr: None,
//                 proposal_voting_period: Some(750),
//                 proposal_effective_delay: None,
//                 proposal_expiration_period: None,
//                 proposal_required_deposit: None,
//                 proposal_required_quorum: None,
//                 proposal_required_threshold: None,
//                 whitelist_add: None,
//                 whitelist_remove: None,
//                 guardian_addr: None,
//             })))
//             .unwrap(),
//             funds: vec![],
//         })]),
//     );
//     // Mint user2's tokens at the same block to increase total supply and add voting power to try to cast vote.
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user2"),
//         5000_000000,
//     );
//
//     app.update_block(next_block);
//
//     // user1 can vote as he had voting power before the proposal submitting.
//     cast_vote(
//         &mut app,
//         assembly_addr.clone(),
//         1,
//         Addr::unchecked("user1"),
//         ProposalVoteOption::For,
//     )
//     .unwrap();
//     // Should panic, because user2 doesn't have any voting power.
//     let err = cast_vote(
//         &mut app,
//         assembly_addr.clone(),
//         1,
//         Addr::unchecked("user2"),
//         ProposalVoteOption::Against,
//     )
//     .unwrap_err();
//
//     // user2 doesn't have voting power and doesn't affect on total voting power(total supply at)
//     // total supply = 5000
//     assert_eq!(
//         err.root_cause().to_string(),
//         "You don't have any voting power!"
//     );
//
//     app.update_block(next_block);
//
//     // Skip voting period and delay
//     app.update_block(|bi| {
//         bi.height += PROPOSAL_VOTING_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1;
//         bi.time = bi
//             .time
//             .plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1));
//     });
//
//     // End proposal
//     app.execute_contract(
//         Addr::unchecked("user0"),
//         assembly_addr.clone(),
//         &ExecuteMsg::EndProposal { proposal_id: 1 },
//         &[],
//     )
//     .unwrap();
//
//     let proposal: Proposal = app
//         .wrap()
//         .query_wasm_smart(
//             assembly_addr.clone(),
//             &QueryMsg::Proposal { proposal_id: 1 },
//         )
//         .unwrap();
//
//     // Check proposal votes
//     assert_eq!(proposal.for_power, Uint128::from(40000_000000u128));
//     assert_eq!(proposal.against_power, Uint128::zero());
//     // Should be passed, as total_voting_power=5000, for_votes=40000.
//     // So user2 didn't affect the result. Because he had to have xASTRO before the vote was submitted.
//     assert_eq!(proposal.status, ProposalStatus::Passed);
// }
//
// #[test]
// fn test_fail_outpost_vote_without_hub() {
//     let mut app = mock_app();
//
//     let owner = Addr::unchecked("owner");
//
//     let (_, staking_instance, xastro_addr, _, _, assembly_addr, _, _) =
//         instantiate_contracts(&mut app, owner, false, false);
//
//     // Mint tokens for submitting proposal
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user0"),
//         PROPOSAL_REQUIRED_DEPOSIT,
//     );
//
//     // Mint tokens for casting votes at start block
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user1"),
//         40000_000000,
//     );
//
//     app.update_block(|mut block| {
//         block.time = block.time.plus_seconds(WEEK);
//         block.height += WEEK / 5;
//     });
//
//     // Create proposal
//     create_proposal(
//         &mut app,
//         &xastro_addr,
//         &assembly_addr,
//         Addr::unchecked("user0"),
//         Some(vec![CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: assembly_addr.to_string(),
//             msg: to_json_binary(&ExecuteMsg::UpdateConfig(Box::new(UpdateConfig {
//                 xastro_token_addr: None,
//                 vxastro_token_addr: None,
//                 voting_escrow_delegator_addr: None,
//                 ibc_controller: None,
//                 generator_controller: None,
//                 hub: None,
//                 builder_unlock_addr: None,
//                 proposal_voting_period: Some(750),
//                 proposal_effective_delay: None,
//                 proposal_expiration_period: None,
//                 proposal_required_deposit: None,
//                 proposal_required_quorum: None,
//                 proposal_required_threshold: None,
//                 whitelist_add: None,
//                 whitelist_remove: None,
//                 guardian_addr: None,
//             })))
//             .unwrap(),
//             funds: vec![],
//         })]),
//     );
//     // Mint user2's tokens at the same block to increase total supply and add voting power to try to cast vote.
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user2"),
//         5000_000000,
//     );
//
//     app.update_block(next_block);
//
//     // user1 can not vote from an Outpost due to no Hub contract set
//     let err = cast_outpost_vote(
//         &mut app,
//         assembly_addr.clone(),
//         1,
//         Addr::unchecked("invalid_contract"),
//         Addr::unchecked("user1"),
//         ProposalVoteOption::For,
//         Uint128::from(100u64),
//     )
//     .unwrap_err();
//
//     assert_eq!(
//         err.root_cause().to_string(),
//         "Sender is not the Hub installed in the assembly"
//     );
// }
//
// #[test]
// fn test_outpost_vote() {
//     let mut app = mock_app();
//
//     let owner = Addr::unchecked("owner");
//
//     let (astro_token, staking_instance, xastro_addr, _, _, assembly_addr, _, hub_addr) =
//         instantiate_contracts(&mut app, owner.clone(), false, true);
//
//     let user1_voting_power = 10_000_000_000;
//     let user2_voting_power = 5_000_000_000;
//     let remote_user1_voting_power = 80_000_000_000u128;
//     // let remote_user2_voting_power = 3_000_000_000u128;
//
//     let hub_addr = hub_addr.unwrap();
//
//     // Mint tokens for submitting proposal
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user0"),
//         PROPOSAL_REQUIRED_DEPOSIT,
//     );
//
//     // Mint tokens for casting votes at start block
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user1"),
//         user1_voting_power,
//     );
//
//     // Mint tokens for casting votes against vote at start block
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user2"),
//         user2_voting_power,
//     );
//
//     // Mint ASTRO to stake
//     mint_tokens(
//         &mut app,
//         &owner,
//         &astro_token,
//         &Addr::unchecked("cw20ics20"),
//         1_000_000_000_000u128,
//     );
//
//     app.update_block(|mut block| {
//         block.time = block.time.plus_seconds(WEEK);
//         block.height += WEEK / 5;
//     });
//
//     // Create proposal
//     create_proposal(
//         &mut app,
//         &xastro_addr,
//         &assembly_addr,
//         Addr::unchecked("user0"),
//         Some(vec![CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: assembly_addr.to_string(),
//             msg: to_json_binary(&ExecuteMsg::UpdateConfig(Box::new(UpdateConfig {
//                 xastro_token_addr: None,
//                 vxastro_token_addr: None,
//                 voting_escrow_delegator_addr: None,
//                 ibc_controller: None,
//                 generator_controller: None,
//                 hub: None,
//                 builder_unlock_addr: None,
//                 proposal_voting_period: Some(750),
//                 proposal_effective_delay: None,
//                 proposal_expiration_period: None,
//                 proposal_required_deposit: None,
//                 proposal_required_quorum: None,
//                 proposal_required_threshold: None,
//                 whitelist_add: None,
//                 whitelist_remove: None,
//                 guardian_addr: None,
//             })))
//             .unwrap(),
//             funds: vec![],
//         })]),
//     );
//
//     app.update_block(next_block);
//
//     // Outpost votes won't be accepted from other addresses
//     let err = cast_outpost_vote(
//         &mut app,
//         assembly_addr.clone(),
//         1,
//         Addr::unchecked("other_contract"),
//         Addr::unchecked("remote1"),
//         ProposalVoteOption::For,
//         Uint128::from(remote_user1_voting_power),
//     )
//     .unwrap_err();
//     assert_eq!(err.root_cause().to_string(), "Unauthorized");
//
//     // Attempts to vote with no xASTRO minted on Outposts
//     let err = cast_outpost_vote(
//         &mut app,
//         assembly_addr.clone(),
//         1,
//         hub_addr,
//         Addr::unchecked("remote1"),
//         ProposalVoteOption::For,
//         Uint128::from(remote_user1_voting_power),
//     )
//     .unwrap_err();
//     assert_eq!(
//         err.root_cause().to_string(),
//         "Voting power exceeds maximum Outpost power"
//     );
//
//     // Note: Due to cw-multitest not supporting IBC messages we can no longer
//     // test voting with Outpost voting power
//
//     // app.execute_contract(
//     //     owner,
//     //     hub_addr.clone(),
//     //     &astroport_governance::hub::ExecuteMsg::AddOutpost {
//     //         outpost_addr: "outpost1".to_string(),
//     //         outpost_channel: "channel-3".to_string(),
//     //         cw20_ics20_channel: "channel-1".to_string(),
//     //     },
//     //     &[],
//     // )
//     // .unwrap_err();
//
//     // Stake some ASTRO from an Outpost
//     // stake_remote_astro(
//     //     &mut app,
//     //     Addr::unchecked("cw20ics20".to_string()),
//     //     hub_addr.clone(),
//     //     astro_token,
//     //     Uint128::from(remote_user1_voting_power),
//     // )
//     // .unwrap_err();
//
//     // Continue normally
//     // cast_outpost_vote(
//     //     &mut app,
//     //     assembly_addr.clone(),
//     //     1,
//     //     hub_addr.clone(),
//     //     Addr::unchecked("remote1"),
//     //     ProposalVoteOption::For,
//     //     Uint128::from(remote_user1_voting_power),
//     // )
//     // .unwrap();
// }
//
// #[test]
// fn test_block_height_selection() {
//     // Block height is 12345 after app initialization
//     let mut app = mock_app();
//
//     let owner = Addr::unchecked("owner");
//     let user1 = Addr::unchecked("user1");
//     let user2 = Addr::unchecked("user2");
//     let user3 = Addr::unchecked("user3");
//
//     let (_, staking_instance, xastro_addr, _, _, assembly_addr, _, _) =
//         instantiate_contracts(&mut app, owner, false, false);
//
//     // Mint tokens for submitting proposal
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &Addr::unchecked("user0"),
//         PROPOSAL_REQUIRED_DEPOSIT,
//     );
//
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &user1,
//         6000_000001,
//     );
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &user2,
//         4000_000000,
//     );
//
//     // Skip to the next period
//     app.update_block(|mut block| {
//         block.time = block.time.plus_seconds(WEEK);
//         block.height += WEEK / 5;
//     });
//
//     // Create proposal
//     create_proposal(
//         &mut app,
//         &xastro_addr,
//         &assembly_addr,
//         Addr::unchecked("user0"),
//         None,
//     );
//
//     cast_vote(
//         &mut app,
//         assembly_addr.clone(),
//         1,
//         user1,
//         ProposalVoteOption::For,
//     )
//     .unwrap();
//
//     // Mint huge amount of xASTRO. These tokens cannot affect on total supply in proposal 1 because
//     // they were minted after proposal.start_block - 1
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &user3,
//         100000_000000,
//     );
//     // Mint more xASTRO to user2, who will vote against the proposal, what is enough to make proposal unsuccessful.
//     mint_tokens(
//         &mut app,
//         &staking_instance,
//         &xastro_addr,
//         &user2,
//         3000_000000,
//     );
//     // Total voting power should be 20k xASTRO (proposal minimum deposit 10k + 4k + 6k users VP)
//     check_total_vp(&mut app, &assembly_addr, 1, 20000_000001);
//
//     cast_vote(
//         &mut app,
//         assembly_addr.clone(),
//         1,
//         user2,
//         ProposalVoteOption::Against,
//     )
//     .unwrap();
//
//     // Skip voting period
//     app.update_block(|bi| {
//         bi.height += PROPOSAL_VOTING_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1;
//         bi.time = bi
//             .time
//             .plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1));
//     });
//
//     // End proposal
//     app.execute_contract(
//         Addr::unchecked("user0"),
//         assembly_addr.clone(),
//         &ExecuteMsg::EndProposal { proposal_id: 1 },
//         &[],
//     )
//     .unwrap();
//
//     let proposal: Proposal = app
//         .wrap()
//         .query_wasm_smart(
//             assembly_addr.clone(),
//             &QueryMsg::Proposal { proposal_id: 1 },
//         )
//         .unwrap();
//
//     assert_eq!(proposal.for_power, Uint128::new(6000_000001));
//     // Against power is 4000, as user2's balance was increased after proposal.start_block - 1
//     // at which everyone's voting power are considered.
//     assert_eq!(proposal.against_power, Uint128::new(4000_000000));
//     // Proposal is passed, as the total supply was increased after proposal.start_block - 1.
//     assert_eq!(proposal.status, ProposalStatus::Passed);
// }
//
// #[test]
// fn test_unsuccessful_proposal() {
//     let mut app = mock_app();
//
//     let owner = Addr::unchecked("owner");
//
//     let (_, staking_instance, xastro_addr, _, _, assembly_addr, _, _) =
//         instantiate_contracts(&mut app, owner, false, false);
//
//     // Init voting power for users
//     let xastro_balances: Vec<(&str, u128)> = vec![
//         ("user0", PROPOSAL_REQUIRED_DEPOSIT), // proposal submitter
//         ("user1", 100),
//         ("user2", 200),
//         ("user3", 400),
//         ("user4", 250),
//         ("user5", 90),
//         ("user6", 300),
//         ("user7", 30),
//         ("user8", 180),
//         ("user9", 50),
//         ("user10", 90),
//         ("user11", 500),
//     ];
//
//     for (addr, xastro) in xastro_balances {
//         mint_tokens(
//             &mut app,
//             &staking_instance,
//             &xastro_addr,
//             &Addr::unchecked(addr),
//             xastro,
//         );
//     }
//
//     // Skip period
//     app.update_block(|mut block| {
//         block.time = block.time.plus_seconds(WEEK);
//         block.height += WEEK / 5;
//     });
//
//     // Create proposal
//     create_proposal(
//         &mut app,
//         &xastro_addr,
//         &assembly_addr,
//         Addr::unchecked("user0"),
//         None,
//     );
//
//     let expected_voting_power: Vec<(&str, ProposalVoteOption)> = vec![
//         ("user1", ProposalVoteOption::For),
//         ("user2", ProposalVoteOption::For),
//         ("user3", ProposalVoteOption::For),
//         ("user4", ProposalVoteOption::Against),
//         ("user5", ProposalVoteOption::Against),
//         ("user6", ProposalVoteOption::Against),
//         ("user7", ProposalVoteOption::Against),
//         ("user8", ProposalVoteOption::Against),
//         ("user9", ProposalVoteOption::Against),
//         ("user10", ProposalVoteOption::Against),
//     ];
//
//     for (addr, option) in expected_voting_power {
//         cast_vote(
//             &mut app,
//             assembly_addr.clone(),
//             1,
//             Addr::unchecked(addr),
//             option,
//         )
//         .unwrap();
//     }
//
//     // Skip voting period
//     app.update_block(|bi| {
//         bi.height += PROPOSAL_VOTING_PERIOD + 1;
//         bi.time = bi.time.plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + 1));
//     });
//
//     // Check balance of submitter before and after proposal completion
//     check_token_balance(&mut app, &xastro_addr, &Addr::unchecked("user0"), 0);
//
//     app.execute_contract(
//         Addr::unchecked("user0"),
//         assembly_addr.clone(),
//         &ExecuteMsg::EndProposal { proposal_id: 1 },
//         &[],
//     )
//     .unwrap();
//
//     check_token_balance(
//         &mut app,
//         &xastro_addr,
//         &Addr::unchecked("user0"),
//         10000_000000,
//     );
//
//     // Check proposal status
//     let proposal: Proposal = app
//         .wrap()
//         .query_wasm_smart(
//             assembly_addr.clone(),
//             &QueryMsg::Proposal { proposal_id: 1 },
//         )
//         .unwrap();
//
//     assert_eq!(proposal.status, ProposalStatus::Rejected);
//
//     // Remove expired proposal
//     app.update_block(|bi| {
//         bi.height += PROPOSAL_EXPIRATION_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1;
//         bi.time = bi
//             .time
//             .plus_seconds(5 * (PROPOSAL_EXPIRATION_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1));
//     });
//
//     app.execute_contract(
//         Addr::unchecked("user0"),
//         assembly_addr.clone(),
//         &ExecuteMsg::RemoveCompletedProposal { proposal_id: 1 },
//         &[],
//     )
//     .unwrap();
//
//     let res: ProposalListResponse = app
//         .wrap()
//         .query_wasm_smart(
//             assembly_addr.to_string(),
//             &QueryMsg::Proposals {
//                 start: None,
//                 limit: None,
//             },
//         )
//         .unwrap();
//
//     assert_eq!(res.proposal_list, vec![]);
//     // proposal_count should not be changed after removing
//     assert_eq!(res.proposal_count, Uint64::from(1u32));
// }
//
// #[test]
// fn test_check_messages() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("owner");
//     let (_, _, _, vxastro_addr, _, assembly_addr, _, _) =
//         instantiate_contracts(&mut app, owner, false, false);
//
//     change_owner(&mut app, &vxastro_addr, &assembly_addr);
//     let user = Addr::unchecked("user");
//     let into_check_msg = |msgs: Vec<(String, Binary)>| {
//         let messages = msgs
//             .into_iter()
//             .map(|(contract_addr, msg)| {
//                 CosmosMsg::Wasm(WasmMsg::Execute {
//                     contract_addr,
//                     msg,
//                     funds: vec![],
//                 })
//             })
//             .collect();
//         ExecuteMsg::CheckMessages { messages }
//     };
//
//     let config_before: astroport_governance::voting_escrow_lite::Config = app
//         .wrap()
//         .query_wasm_smart(
//             &vxastro_addr,
//             &astroport_governance::voting_escrow_lite::QueryMsg::Config {},
//         )
//         .unwrap();
//
//     let vxastro_blacklist_msg = vec![(
//         vxastro_addr.to_string(),
//         to_json_binary(
//             &astroport_governance::voting_escrow_lite::ExecuteMsg::UpdateConfig {
//                 new_guardian: None,
//                 generator_controller: None,
//                 outpost: None,
//             },
//         )
//         .unwrap(),
//     )];
//     let err = app
//         .execute_contract(
//             user,
//             assembly_addr.clone(),
//             &into_check_msg(vxastro_blacklist_msg),
//             &[],
//         )
//         .unwrap_err();
//     assert_eq!(
//         &err.root_cause().to_string(),
//         "Messages check passed. Nothing was committed to the blockchain"
//     );
//
//     let config_after: astroport_governance::voting_escrow_lite::Config = app
//         .wrap()
//         .query_wasm_smart(
//             &vxastro_addr,
//             &astroport_governance::voting_escrow_lite::QueryMsg::Config {},
//         )
//         .unwrap();
//     assert_eq!(config_before, config_after);
// }
//
// fn mock_app() -> App {
//     let mut env = mock_env();
//     env.block.time = Timestamp::from_seconds(EPOCH_START);
//     let api = MockApi::default();
//     let bank = BankKeeper::new();
//     let storage = MockStorage::new();
//
//     AppBuilder::new()
//         .with_api(api)
//         .with_block(env.block)
//         .with_bank(bank)
//         .with_storage(storage)
//         .build(|_, _, _| {})
// }
//
// fn instantiate_contracts(
//     router: &mut App,
//     owner: Addr,
//     with_generator_controller: bool,
//     with_hub: bool,
// ) -> (
//     Addr,
//     Addr,
//     Addr,
//     Addr,
//     Addr,
//     Addr,
//     Option<Addr>,
//     Option<Addr>,
// ) {
//     let token_addr = instantiate_astro_token(router, &owner);
//     let (staking_addr, xastro_token_addr) = instantiate_xastro_token(router, &owner, &token_addr);
//     let vxastro_token_addr = instantiate_vxastro_token(router, &owner, &xastro_token_addr);
//     let builder_unlock_addr = instantiate_builder_unlock_contract(router, &owner, &token_addr);
//
//     // If we want to test immediate proposals we need to set the address
//     // for the generator controller. Deploying the generator controller in this
//     // test would require deploying factory, tokens and pools. That test is
//     // better suited in the generator controller itself. Thus, we use the owner
//     // address as the generator controller address to test immediate proposals.
//     let mut generator_controller_addr = None;
//
//     if with_generator_controller {
//         generator_controller_addr = Some(owner.to_string());
//     }
//
//     let mut hub_addr = None;
//
//     if with_hub {
//         hub_addr = Some(instantiate_hub(
//             router,
//             &owner,
//             &Addr::unchecked("contract6".to_string()),
//             &staking_addr,
//         ));
//     }
//
//     let assembly_addr = instantiate_assembly_contract(
//         router,
//         &owner,
//         &xastro_token_addr,
//         &vxastro_token_addr,
//         &builder_unlock_addr,
//         None,
//         generator_controller_addr,
//         hub_addr.clone(),
//     );
//
//     (
//         token_addr,
//         staking_addr,
//         xastro_token_addr,
//         vxastro_token_addr,
//         builder_unlock_addr,
//         assembly_addr,
//         None,
//         hub_addr,
//     )
// }
//
// fn instantiate_astro_token(router: &mut App, owner: &Addr) -> Addr {
//     let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
//         astroport_token::contract::execute,
//         astroport_token::contract::instantiate,
//         astroport_token::contract::query,
//     ));
//
//     let astro_token_code_id = router.store_code(astro_token_contract);
//
//     let msg = TokenInstantiateMsg {
//         name: String::from("Astro token"),
//         symbol: String::from("ASTRO"),
//         decimals: 6,
//         initial_balances: vec![],
//         mint: Some(MinterResponse {
//             minter: owner.to_string(),
//             cap: None,
//         }),
//         marketing: None,
//     };
//
//     router
//         .instantiate_contract(
//             astro_token_code_id,
//             owner.clone(),
//             &msg,
//             &[],
//             String::from("ASTRO"),
//             None,
//         )
//         .unwrap()
// }
//
// fn instantiate_xastro_token(router: &mut App, owner: &Addr, astro_token: &Addr) -> (Addr, Addr) {
//     let xastro_contract = Box::new(ContractWrapper::new_with_empty(
//         astroport_xastro_token::contract::execute,
//         astroport_xastro_token::contract::instantiate,
//         astroport_xastro_token::contract::query,
//     ));
//
//     let xastro_code_id = router.store_code(xastro_contract);
//
//     let staking_contract = Box::new(
//         ContractWrapper::new_with_empty(
//             astroport_staking::contract::execute,
//             astroport_staking::contract::instantiate,
//             astroport_staking::contract::query,
//         )
//         .with_reply_empty(astroport_staking::contract::reply),
//     );
//
//     let staking_code_id = router.store_code(staking_contract);
//
//     let msg = astroport::staking::InstantiateMsg {
//         owner: owner.to_string(),
//         token_code_id: xastro_code_id,
//         deposit_token_addr: astro_token.to_string(),
//         marketing: None,
//     };
//     let staking_instance = router
//         .instantiate_contract(
//             staking_code_id,
//             owner.clone(),
//             &msg,
//             &[],
//             String::from("xASTRO"),
//             None,
//         )
//         .unwrap();
//
//     let res = router
//         .wrap()
//         .query::<astroport::staking::ConfigResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: staking_instance.to_string(),
//             msg: to_json_binary(&astroport::staking::QueryMsg::Config {}).unwrap(),
//         }))
//         .unwrap();
//
//     (staking_instance, res.share_token_addr)
// }
//
// fn instantiate_vxastro_token(router: &mut App, owner: &Addr, xastro: &Addr) -> Addr {
//     let vxastro_token_contract = Box::new(ContractWrapper::new_with_empty(
//         voting_escrow_lite::execute::execute,
//         voting_escrow_lite::contract::instantiate,
//         voting_escrow_lite::query::query,
//     ));
//
//     let vxastro_token_code_id = router.store_code(vxastro_token_contract);
//
//     let msg = VXAstroInstantiateMsg {
//         owner: owner.to_string(),
//         guardian_addr: Some(owner.to_string()),
//         deposit_token_addr: xastro.to_string(),
//         generator_controller_addr: None,
//         outpost_addr: None,
//         marketing: None,
//         logo_urls_whitelist: vec![],
//     };
//
//     router
//         .instantiate_contract(
//             vxastro_token_code_id,
//             owner.clone(),
//             &msg,
//             &[],
//             String::from("vxASTRO"),
//             None,
//         )
//         .unwrap()
// }
//
// fn instantiate_hub(
//     router: &mut App,
//     owner: &Addr,
//     assembly_addr: &Addr,
//     staking_addr: &Addr,
// ) -> Addr {
//     let hub_contract = Box::new(
//         ContractWrapper::new_with_empty(
//             astroport_hub::execute::execute,
//             astroport_hub::contract::instantiate,
//             astroport_hub::query::query,
//         )
//         .with_reply(astroport_hub::reply::reply),
//     );
//
//     let hub_code_id = router.store_code(hub_contract);
//
//     let msg = HubInstantiateMsg {
//         owner: owner.to_string(),
//         assembly_addr: assembly_addr.to_string(),
//         cw20_ics20_addr: "cw20ics20".to_string(),
//         generator_controller_addr: "unknown".to_string(),
//         ibc_timeout_seconds: 60,
//         staking_addr: staking_addr.to_string(),
//     };
//
//     router
//         .instantiate_contract(
//             hub_code_id,
//             owner.clone(),
//             &msg,
//             &[],
//             String::from("Hub"),
//             None,
//         )
//         .unwrap()
// }
//
// fn instantiate_builder_unlock_contract(router: &mut App, owner: &Addr, astro_token: &Addr) -> Addr {
//     let builder_unlock_contract = Box::new(ContractWrapper::new_with_empty(
//         builder_unlock::contract::execute,
//         builder_unlock::contract::instantiate,
//         builder_unlock::contract::query,
//     ));
//
//     let builder_unlock_code_id = router.store_code(builder_unlock_contract);
//
//     let msg = BuilderUnlockInstantiateMsg {
//         owner: owner.to_string(),
//         astro_token: astro_token.to_string(),
//         max_allocations_amount: Uint128::new(300_000_000_000_000u128),
//     };
//
//     router
//         .instantiate_contract(
//             builder_unlock_code_id,
//             owner.clone(),
//             &msg,
//             &[],
//             "Builder Unlock contract".to_string(),
//             Some(owner.to_string()),
//         )
//         .unwrap()
// }
//
// #[allow(clippy::too_many_arguments)]
// fn instantiate_assembly_contract(
//     router: &mut App,
//     owner: &Addr,
//     xastro: &Addr,
//     vxastro: &Addr,
//     builder: &Addr,
//     delegator: Option<String>,
//     generator_controller_addr: Option<String>,
//     hub_addr: Option<Addr>,
// ) -> Addr {
//     let assembly_contract = Box::new(ContractWrapper::new_with_empty(
//         astro_assembly::contract::execute,
//         astro_assembly::contract::instantiate,
//         astro_assembly::contract::query,
//     ));
//
//     let assembly_code = router.store_code(assembly_contract);
//
//     let hub: Option<String> = hub_addr.as_ref().map(|s| s.to_string());
//
//     let msg = InstantiateMsg {
//         xastro_token_addr: xastro.to_string(),
//         vxastro_token_addr: Some(vxastro.to_string()),
//         voting_escrow_delegator_addr: delegator,
//         ibc_controller: None,
//         generator_controller_addr,
//         hub_addr: hub,
//         builder_unlock_addr: builder.to_string(),
//         proposal_voting_period: PROPOSAL_VOTING_PERIOD,
//         proposal_effective_delay: PROPOSAL_EFFECTIVE_DELAY,
//         proposal_expiration_period: PROPOSAL_EXPIRATION_PERIOD,
//         proposal_required_deposit: Uint128::new(PROPOSAL_REQUIRED_DEPOSIT),
//         proposal_required_quorum: String::from(PROPOSAL_REQUIRED_QUORUM),
//         proposal_required_threshold: String::from(PROPOSAL_REQUIRED_THRESHOLD),
//         whitelisted_links: vec!["https://some.link/".to_string()],
//     };
//
//     router
//         .instantiate_contract(
//             assembly_code,
//             owner.clone(),
//             &msg,
//             &[],
//             "Assembly".to_string(),
//             Some(owner.to_string()),
//         )
//         .unwrap()
// }
//
//
// fn mint_vxastro(
//     app: &mut App,
//     staking_instance: &Addr,
//     xastro: Addr,
//     vxastro: &Addr,
//     recipient: Addr,
//     amount: u128,
// ) {
//     mint_tokens(app, staking_instance, &xastro, &recipient, amount);
//
//     let msg = Cw20ExecuteMsg::Send {
//         contract: vxastro.to_string(),
//         amount: Uint128::from(amount),
//         msg: to_json_binary(&VXAstroCw20HookMsg::CreateLock { time: WEEK * 50 }).unwrap(),
//     };
//
//     app.execute_contract(recipient, xastro, &msg, &[]).unwrap();
// }
//
//
// fn create_proposal(
//     app: &mut App,
//     token: &Addr,
//     assembly: &Addr,
//     submitter: Addr,
//     msgs: Option<Vec<CosmosMsg>>,
// ) {
//     let submit_proposal_msg = Cw20HookMsg::SubmitProposal {
//         title: "Test title!".to_string(),
//         description: "Test description!".to_string(),
//         link: None,
//         messages: msgs,
//         ibc_channel: None,
//     };
//
//     app.execute_contract(
//         submitter,
//         token.clone(),
//         &Cw20ExecuteMsg::Send {
//             contract: assembly.to_string(),
//             amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
//             msg: to_json_binary(&submit_proposal_msg).unwrap(),
//         },
//         &[],
//     )
//     .unwrap();
// }
//
// fn check_token_balance(app: &mut App, token: &Addr, address: &Addr, expected: u128) {
//     let msg = XAstroQueryMsg::Balance {
//         address: address.to_string(),
//     };
//     let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
//     assert_eq!(res.unwrap().balance, Uint128::from(expected));
// }
//
// fn check_user_vp(app: &mut App, assembly: &Addr, address: &Addr, proposal_id: u64, expected: u128) {
//     let res: Uint128 = app
//         .wrap()
//         .query_wasm_smart(
//             assembly.to_string(),
//             &QueryMsg::UserVotingPower {
//                 user: address.to_string(),
//                 proposal_id,
//             },
//         )
//         .unwrap();
//
//     assert_eq!(res.u128(), expected);
// }
//
// fn check_total_vp(app: &mut App, assembly: &Addr, proposal_id: u64, expected: u128) {
//     let res: Uint128 = app
//         .wrap()
//         .query_wasm_smart(
//             assembly.to_string(),
//             &QueryMsg::TotalVotingPower { proposal_id },
//         )
//         .unwrap();
//
//     assert_eq!(res.u128(), expected);
// }
//
// fn cast_vote(
//     app: &mut App,
//     assembly: Addr,
//     proposal_id: u64,
//     sender: Addr,
//     option: ProposalVoteOption,
// ) -> anyhow::Result<AppResponse> {
//     app.execute_contract(
//         sender,
//         assembly,
//         &ExecuteMsg::CastVote {
//             proposal_id,
//             vote: option,
//         },
//         &[],
//     )
// }
//
// fn cast_outpost_vote(
//     app: &mut App,
//     assembly: Addr,
//     proposal_id: u64,
//     sender: Addr,
//     voter: Addr,
//     option: ProposalVoteOption,
//     voting_power: Uint128,
// ) -> anyhow::Result<AppResponse> {
//     app.execute_contract(
//         sender,
//         assembly,
//         &ExecuteMsg::CastOutpostVote {
//             proposal_id,
//             voter: voter.to_string(),
//             vote: option,
//             voting_power,
//         },
//         &[],
//     )
// }
//
// // Add back once cw-multitest supports IBC
// // fn stake_remote_astro(
// //     app: &mut App,
// //     sender: Addr,
// //     hub: Addr,
// //     astro_token: Addr,
// //     amount: Uint128,
// // ) -> anyhow::Result<AppResponse> {
// //     let cw20_msg = to_json_binary(&astroport_governance::hub::Cw20HookMsg::OutpostMemo {
// //         channel: "channel-1".to_string(),
// //         sender: "remoteuser1".to_string(),
// //         receiver: hub.to_string(),
// //         memo: "{\"stake\":{}}".to_string(),
// //     })
// //     .unwrap();
//
// //     let msg = Cw20ExecuteMsg::Send {
// //         contract: hub.to_string(),
// //         amount,
// //         msg: cw20_msg,
// //     };
//
// //     app.execute_contract(sender, astro_token, &msg, &[])
// // }
//
// fn change_owner(app: &mut App, contract: &Addr, assembly: &Addr) {
//     let msg = astroport_governance::voting_escrow_lite::ExecuteMsg::ProposeNewOwner {
//         new_owner: assembly.to_string(),
//         expires_in: 100,
//     };
//     app.execute_contract(Addr::unchecked("owner"), contract.clone(), &msg, &[])
//         .unwrap();
//
//     app.execute_contract(
//         assembly.clone(),
//         contract.clone(),
//         &astroport_governance::voting_escrow_lite::ExecuteMsg::ClaimOwnership {},
//         &[],
//     )
//     .unwrap();
// }
