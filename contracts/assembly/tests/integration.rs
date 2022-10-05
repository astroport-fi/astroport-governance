use astro_assembly::astroport;
use astroport::{
    token::InstantiateMsg as TokenInstantiateMsg, xastro_token::QueryMsg as XAstroQueryMsg,
};
use std::str::FromStr;

use astroport_governance::assembly::{
    Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, ProposalListResponse, ProposalMessage,
    ProposalResponse, ProposalStatus, ProposalVoteOption, ProposalVotesResponse, QueryMsg,
    UpdateConfig, DEPOSIT_INTERVAL, VOTING_PERIOD_INTERVAL,
};

use astroport_governance::voting_escrow::{
    Cw20HookMsg as VXAstroCw20HookMsg, InstantiateMsg as VXAstroInstantiateMsg,
};

use astroport_governance::builder_unlock::msg::{
    InstantiateMsg as BuilderUnlockInstantiateMsg, ReceiveMsg as BuilderUnlockReceiveMsg,
};
use astroport_governance::builder_unlock::{AllocationParams, Schedule};
use astroport_governance::utils::{EPOCH_START, WEEK};
use astroport_governance::voting_escrow_delegation::{
    ExecuteMsg as DelegatorExecuteMsg, InstantiateMsg as DelegatorInstantiateMsg,
    QueryMsg as DelegatorQueryMsg,
};
use cosmwasm_std::{
    testing::{mock_env, MockApi, MockStorage},
    to_binary, Addr, Binary, CosmosMsg, Decimal, QueryRequest, StdResult, Timestamp, Uint128,
    Uint64, WasmMsg, WasmQuery,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, MinterResponse};
use cw_multi_test::{
    next_block, App, AppBuilder, AppResponse, BankKeeper, ContractWrapper, Executor,
};

const PROPOSAL_VOTING_PERIOD: u64 = *VOTING_PERIOD_INTERVAL.start();
const PROPOSAL_EFFECTIVE_DELAY: u64 = 12_342;
const PROPOSAL_EXPIRATION_PERIOD: u64 = 86_399;
const PROPOSAL_REQUIRED_DEPOSIT: u128 = *DEPOSIT_INTERVAL.start();
const PROPOSAL_REQUIRED_QUORUM: &str = "0.50";
const PROPOSAL_REQUIRED_THRESHOLD: &str = "0.60";

#[test]
fn test_contract_instantiation() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    // Instantiate needed contracts
    let token_addr = instantiate_astro_token(&mut app, &owner);
    let (_, xastro_token_addr) = instantiate_xastro_token(&mut app, &owner, &token_addr);
    let vxastro_token_addr = instantiate_vxastro_token(&mut app, &owner, &xastro_token_addr);
    let builder_unlock_addr = instantiate_builder_unlock_contract(&mut app, &owner, &token_addr);

    let assembly_contract = Box::new(ContractWrapper::new_with_empty(
        astro_assembly::contract::execute,
        astro_assembly::contract::instantiate,
        astro_assembly::contract::query,
    ));

    let assembly_code = app.store_code(assembly_contract);

    let assembly_default_instantiate_msg = InstantiateMsg {
        xastro_token_addr: xastro_token_addr.to_string(),
        vxastro_token_addr: Some(vxastro_token_addr.to_string()),
        voting_escrow_delegator_addr: None,
        builder_unlock_addr: builder_unlock_addr.to_string(),
        proposal_voting_period: PROPOSAL_VOTING_PERIOD,
        proposal_effective_delay: PROPOSAL_EFFECTIVE_DELAY,
        proposal_expiration_period: PROPOSAL_EXPIRATION_PERIOD,
        proposal_required_deposit: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
        proposal_required_quorum: String::from(PROPOSAL_REQUIRED_QUORUM),
        proposal_required_threshold: String::from(PROPOSAL_REQUIRED_THRESHOLD),
        whitelisted_links: vec!["https://some.link/".to_string()],
    };

    // Try to instantiate assembly with wrong threshold
    let err = app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_required_threshold: "0.3".to_string(),
                ..assembly_default_instantiate_msg.clone()
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

    let err = app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_required_threshold: "1.1".to_string(),
                ..assembly_default_instantiate_msg.clone()
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

    let err = app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_required_quorum: "1.1".to_string(),
                ..assembly_default_instantiate_msg.clone()
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

    let err = app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_expiration_period: 500,
                ..assembly_default_instantiate_msg.clone()
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: The expiration period for a proposal cannot be lower than 12342 or higher than 86399"
    );

    let err = app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_effective_delay: 400,
                ..assembly_default_instantiate_msg.clone()
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: The effective delay for a proposal cannot be lower than 6171 or higher than 12342"
    );

    let assembly_instance = app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &assembly_default_instantiate_msg,
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap();

    let res: Config = app
        .wrap()
        .query_wasm_smart(assembly_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(res.xastro_token_addr, xastro_token_addr);
    assert_eq!(res.builder_unlock_addr, builder_unlock_addr);
    assert_eq!(res.proposal_voting_period, PROPOSAL_VOTING_PERIOD);
    assert_eq!(res.proposal_effective_delay, PROPOSAL_EFFECTIVE_DELAY);
    assert_eq!(res.proposal_expiration_period, PROPOSAL_EXPIRATION_PERIOD);
    assert_eq!(
        res.proposal_required_deposit,
        Uint128::from(PROPOSAL_REQUIRED_DEPOSIT)
    );
    assert_eq!(
        res.proposal_required_quorum,
        Decimal::from_str(PROPOSAL_REQUIRED_QUORUM).unwrap()
    );
    assert_eq!(
        res.proposal_required_threshold,
        Decimal::from_str(PROPOSAL_REQUIRED_THRESHOLD).unwrap()
    );
    assert_eq!(
        res.whitelisted_links,
        vec!["https://some.link/".to_string(),]
    );
}

#[test]
fn test_proposal_submitting() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user1");

    let (_, staking_instance, xastro_addr, _, _, assembly_addr, _) =
        instantiate_contracts(&mut app, owner, false);

    let proposals: ProposalListResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposals {
                start: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(proposals.proposal_count, Uint64::from(0u32));
    assert_eq!(proposals.proposal_list, vec![]);

    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &user,
        PROPOSAL_REQUIRED_DEPOSIT,
    );

    check_token_balance(&mut app, &xastro_addr, &user, PROPOSAL_REQUIRED_DEPOSIT);

    // Try to create proposal with insufficient token deposit
    let submit_proposal_msg = Cw20ExecuteMsg::Send {
        contract: assembly_addr.to_string(),
        msg: to_binary(&Cw20HookMsg::SubmitProposal {
            title: String::from("Title"),
            description: String::from("Description"),
            link: Some(String::from("https://some.link")),
            messages: None,
        })
        .unwrap(),
        amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT - 1),
    };

    let err = app
        .execute_contract(user.clone(), xastro_addr.clone(), &submit_proposal_msg, &[])
        .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "Insufficient token deposit!");

    // Try to create a proposal with wrong title
    let err = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("X"),
                    description: String::from("Description"),
                    link: Some(String::from("https://some.link/")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Title too short!"
    );

    let err = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from_utf8(vec![b'X'; 65]).unwrap(),
                    description: String::from("Description"),
                    link: Some(String::from("https://some.link/")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Title too long!"
    );

    // Try to create a proposal with wrong description
    let err = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("Title"),
                    description: String::from("X"),
                    link: Some(String::from("https://some.link/")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Description too short!"
    );

    let err = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("Title"),
                    description: String::from_utf8(vec![b'X'; 1025]).unwrap(),
                    link: Some(String::from("https://some.link/")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Description too long!"
    );

    // Try to create a proposal with wrong link
    let err = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("Title"),
                    description: String::from("Description"),
                    link: Some(String::from("X")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Link too short!"
    );

    let err = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("Title"),
                    description: String::from("Description"),
                    link: Some(String::from_utf8(vec![b'X'; 129]).unwrap()),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Link too long!"
    );

    let err = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("Title"),
                    description: String::from("Description"),
                    link: Some(String::from("https://some1.link")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Link is not whitelisted!"
    );

    let err = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("Title"),
                    description: String::from("Description"),
                    link: Some(String::from(
                        "https://some.link/<script>alert('test');</script>",
                    )),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Link is not properly formatted or contains unsafe characters!"
    );

    // Valid proposal submission
    app.execute_contract(
        user.clone(),
        xastro_addr.clone(),
        &Cw20ExecuteMsg::Send {
            contract: assembly_addr.to_string(),
            msg: to_binary(&Cw20HookMsg::SubmitProposal {
                title: String::from("Title"),
                description: String::from("Description"),
                link: Some(String::from("https://some.link/q/")),
                messages: Some(vec![ProposalMessage {
                    order: Uint64::from(0u32),
                    msg: CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: assembly_addr.to_string(),
                        msg: to_binary(&ExecuteMsg::UpdateConfig(UpdateConfig {
                            xastro_token_addr: None,
                            vxastro_token_addr: None,
                            voting_escrow_delegator_addr: None,
                            builder_unlock_addr: None,
                            proposal_voting_period: Some(750),
                            proposal_effective_delay: None,
                            proposal_expiration_period: None,
                            proposal_required_deposit: None,
                            proposal_required_quorum: None,
                            proposal_required_threshold: None,
                            whitelist_add: None,
                            whitelist_remove: None,
                        }))
                        .unwrap(),
                        funds: vec![],
                    }),
                }]),
            })
            .unwrap(),
            amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
        },
        &[],
    )
    .unwrap();

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    assert_eq!(proposal.proposal_id, Uint64::from(1u64));
    assert_eq!(proposal.submitter, user);
    assert_eq!(proposal.status, ProposalStatus::Active);
    assert_eq!(proposal.for_power, Uint128::zero());
    assert_eq!(proposal.against_power, Uint128::zero());
    assert_eq!(proposal.start_block, 12_345);
    assert_eq!(proposal.end_block, 12_345 + PROPOSAL_VOTING_PERIOD);
    assert_eq!(proposal.title, String::from("Title"));
    assert_eq!(proposal.description, String::from("Description"));
    assert_eq!(proposal.link, Some(String::from("https://some.link/q/")));
    assert_eq!(
        proposal.messages,
        Some(vec![ProposalMessage {
            order: Uint64::from(0u32),
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: assembly_addr.to_string(),
                msg: to_binary(&ExecuteMsg::UpdateConfig(UpdateConfig {
                    xastro_token_addr: None,
                    vxastro_token_addr: None,
                    voting_escrow_delegator_addr: None,
                    builder_unlock_addr: None,
                    proposal_voting_period: Some(750),
                    proposal_effective_delay: None,
                    proposal_expiration_period: None,
                    proposal_required_deposit: None,
                    proposal_required_quorum: None,
                    proposal_required_threshold: None,
                    whitelist_add: None,
                    whitelist_remove: None,
                }))
                .unwrap(),
                funds: vec![],
            }),
        }])
    );
    assert_eq!(
        proposal.deposit_amount,
        Uint128::from(PROPOSAL_REQUIRED_DEPOSIT)
    )
}

#[test]
fn test_successful_proposal() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    let (
        token_addr,
        staking_instance,
        xastro_addr,
        vxastro_addr,
        builder_unlock_addr,
        assembly_addr,
        _,
    ) = instantiate_contracts(&mut app, owner, false);

    // Init voting power for users
    let balances: Vec<(&str, u128, u128)> = vec![
        ("user0", PROPOSAL_REQUIRED_DEPOSIT, 0), // proposal submitter
        ("user1", 20, 80),
        ("user2", 100, 100),
        ("user3", 300, 100),
        ("user4", 200, 50),
        ("user5", 0, 90),
        ("user6", 100, 200),
        ("user7", 30, 0),
        ("user8", 80, 100),
        ("user9", 50, 0),
        ("user10", 0, 90),
        ("user11", 500, 0),
        ("user12", 10000_000000, 0),
    ];

    let default_allocation_params = AllocationParams {
        amount: Uint128::zero(),
        unlock_schedule: Schedule {
            start_time: 12_345,
            cliff: 5,
            duration: 500,
        },
        proposed_receiver: None,
    };

    let locked_balances = vec![
        (
            "user1".to_string(),
            AllocationParams {
                amount: Uint128::from(80u32),
                ..default_allocation_params.clone()
            },
        ),
        (
            "user4".to_string(),
            AllocationParams {
                amount: Uint128::from(50u32),
                ..default_allocation_params.clone()
            },
        ),
        (
            "user7".to_string(),
            AllocationParams {
                amount: Uint128::from(100u32),
                ..default_allocation_params.clone()
            },
        ),
        (
            "user10".to_string(),
            AllocationParams {
                amount: Uint128::from(30u32),
                ..default_allocation_params.clone()
            },
        ),
    ];

    for (addr, xastro, vxastro) in balances {
        if xastro > 0 {
            mint_tokens(
                &mut app,
                &staking_instance,
                &xastro_addr,
                &Addr::unchecked(addr),
                xastro,
            );
        }

        if vxastro > 0 {
            mint_vxastro(
                &mut app,
                &staking_instance,
                xastro_addr.clone(),
                &vxastro_addr,
                Addr::unchecked(addr),
                vxastro,
            );
        }
    }

    create_allocations(&mut app, token_addr, builder_unlock_addr, locked_balances);

    // Skip period
    app.update_block(|mut block| {
        block.time = block.time.plus_seconds(WEEK);
        block.height += WEEK / 5;
    });

    // Create default proposal
    create_proposal(
        &mut app,
        &xastro_addr,
        &assembly_addr,
        Addr::unchecked("user0"),
        Some(vec![ProposalMessage {
            order: Uint64::from(0u32),
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: assembly_addr.to_string(),
                msg: to_binary(&ExecuteMsg::UpdateConfig(UpdateConfig {
                    xastro_token_addr: None,
                    vxastro_token_addr: None,
                    voting_escrow_delegator_addr: None,
                    builder_unlock_addr: None,
                    proposal_voting_period: Some(PROPOSAL_VOTING_PERIOD + 1000),
                    proposal_effective_delay: None,
                    proposal_expiration_period: None,
                    proposal_required_deposit: None,
                    proposal_required_quorum: None,
                    proposal_required_threshold: None,
                    whitelist_add: Some(vec![
                        "https://some1.link/".to_string(),
                        "https://some2.link/".to_string(),
                    ]),
                    whitelist_remove: Some(vec!["https://some.link/".to_string()]),
                }))
                .unwrap(),
                funds: vec![],
            }),
        }]),
    );

    let votes: Vec<(&str, ProposalVoteOption, u128)> = vec![
        ("user1", ProposalVoteOption::For, 280u128),
        ("user2", ProposalVoteOption::For, 350u128),
        ("user3", ProposalVoteOption::For, 550u128),
        ("user4", ProposalVoteOption::For, 350u128),
        ("user5", ProposalVoteOption::For, 240u128),
        ("user6", ProposalVoteOption::For, 600u128),
        ("user7", ProposalVoteOption::For, 130u128),
        ("user8", ProposalVoteOption::Against, 330u128),
        ("user9", ProposalVoteOption::Against, 50u128),
        ("user10", ProposalVoteOption::Against, 270u128),
        ("user11", ProposalVoteOption::Against, 500u128),
        ("user12", ProposalVoteOption::For, 10000_000000u128),
    ];

    check_total_vp(&mut app, &assembly_addr, 1, 20000003650);

    for (addr, option, expected_vp) in votes {
        let sender = Addr::unchecked(addr);

        check_user_vp(&mut app, &assembly_addr, &sender, 1, expected_vp);

        cast_vote(&mut app, assembly_addr.clone(), 1, sender, option).unwrap();
    }

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    let proposal_votes: ProposalVotesResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::ProposalVotes { proposal_id: 1 },
        )
        .unwrap();

    let proposal_for_voters: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::ProposalVoters {
                proposal_id: 1,
                vote_option: ProposalVoteOption::For,
                start: None,
                limit: None,
            },
        )
        .unwrap();

    let proposal_against_voters: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::ProposalVoters {
                proposal_id: 1,
                vote_option: ProposalVoteOption::Against,
                start: None,
                limit: None,
            },
        )
        .unwrap();

    // Check proposal votes
    assert_eq!(proposal.for_power, Uint128::from(10000002500u128));
    assert_eq!(proposal.against_power, Uint128::from(1150u32));

    assert_eq!(proposal_votes.for_power, Uint128::from(10000002500u128));
    assert_eq!(proposal_votes.against_power, Uint128::from(1150u32));

    assert_eq!(
        proposal_for_voters,
        vec![
            Addr::unchecked("user1"),
            Addr::unchecked("user2"),
            Addr::unchecked("user3"),
            Addr::unchecked("user4"),
            Addr::unchecked("user5"),
            Addr::unchecked("user6"),
            Addr::unchecked("user7"),
            Addr::unchecked("user12"),
        ]
    );
    assert_eq!(
        proposal_against_voters,
        vec![
            Addr::unchecked("user8"),
            Addr::unchecked("user9"),
            Addr::unchecked("user10"),
            Addr::unchecked("user11")
        ]
    );

    // Skip voting period
    app.update_block(|bi| {
        bi.height += PROPOSAL_VOTING_PERIOD + 1;
        bi.time = bi.time.plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + 1));
    });

    // Try to vote after voting period
    let err = cast_vote(
        &mut app,
        assembly_addr.clone(),
        1,
        Addr::unchecked("user11"),
        ProposalVoteOption::Against,
    )
    .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "Voting period ended!");

    // Try to execute the proposal before end_proposal
    let err = app
        .execute_contract(
            Addr::unchecked("user0"),
            assembly_addr.clone(),
            &ExecuteMsg::ExecuteProposal { proposal_id: 1 },
            &[],
        )
        .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "Proposal not passed!");

    // Check the successful completion of the proposal
    check_token_balance(&mut app, &xastro_addr, &Addr::unchecked("user0"), 0);

    app.execute_contract(
        Addr::unchecked("user0"),
        assembly_addr.clone(),
        &ExecuteMsg::EndProposal { proposal_id: 1 },
        &[],
    )
    .unwrap();

    check_token_balance(
        &mut app,
        &xastro_addr,
        &Addr::unchecked("user0"),
        PROPOSAL_REQUIRED_DEPOSIT,
    );

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    assert_eq!(proposal.status, ProposalStatus::Passed);

    // Try to end proposal again
    let err = app
        .execute_contract(
            Addr::unchecked("user0"),
            assembly_addr.clone(),
            &ExecuteMsg::EndProposal { proposal_id: 1 },
            &[],
        )
        .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "Proposal not active!");

    // Try to execute the proposal before the delay
    let err = app
        .execute_contract(
            Addr::unchecked("user0"),
            assembly_addr.clone(),
            &ExecuteMsg::ExecuteProposal { proposal_id: 1 },
            &[],
        )
        .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "Proposal delay not ended!");

    // Skip blocks
    app.update_block(|bi| {
        bi.height += PROPOSAL_EFFECTIVE_DELAY + 1;
        bi.time = bi.time.plus_seconds(5 * (PROPOSAL_EFFECTIVE_DELAY + 1));
    });

    // Try to execute the proposal after the delay
    app.execute_contract(
        Addr::unchecked("user0"),
        assembly_addr.clone(),
        &ExecuteMsg::ExecuteProposal { proposal_id: 1 },
        &[],
    )
    .unwrap();

    let config: Config = app
        .wrap()
        .query_wasm_smart(assembly_addr.to_string(), &QueryMsg::Config {})
        .unwrap();

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.to_string(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    // Check execution result
    assert_eq!(config.proposal_voting_period, PROPOSAL_VOTING_PERIOD + 1000);
    assert_eq!(
        config.whitelisted_links,
        vec![
            "https://some1.link/".to_string(),
            "https://some2.link/".to_string(),
        ]
    );
    assert_eq!(proposal.status, ProposalStatus::Executed);

    // Try to remove proposal before expiration period
    let err = app
        .execute_contract(
            Addr::unchecked("user0"),
            assembly_addr.clone(),
            &ExecuteMsg::RemoveCompletedProposal { proposal_id: 1 },
            &[],
        )
        .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "Proposal not completed!");

    // Remove expired proposal
    app.update_block(|bi| {
        bi.height += PROPOSAL_EXPIRATION_PERIOD + 1;
        bi.time = bi.time.plus_seconds(5 * (PROPOSAL_EXPIRATION_PERIOD + 1));
    });

    app.execute_contract(
        Addr::unchecked("user0"),
        assembly_addr.clone(),
        &ExecuteMsg::RemoveCompletedProposal { proposal_id: 1 },
        &[],
    )
    .unwrap();

    let res: ProposalListResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.to_string(),
            &QueryMsg::Proposals {
                start: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(res.proposal_list, vec![]);
    // proposal_count should not be changed after removing a proposal
    assert_eq!(res.proposal_count, Uint64::from(1u32));
}

#[test]
fn test_voting_power_changes() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    let (_, staking_instance, xastro_addr, _, _, assembly_addr, _) =
        instantiate_contracts(&mut app, owner, false);

    // Mint tokens for submitting proposal
    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &Addr::unchecked("user0"),
        PROPOSAL_REQUIRED_DEPOSIT,
    );

    // Mint tokens for casting votes at start block
    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &Addr::unchecked("user1"),
        40000_000000,
    );

    app.update_block(|mut block| {
        block.time = block.time.plus_seconds(WEEK);
        block.height += WEEK / 5;
    });

    // Create proposal
    create_proposal(
        &mut app,
        &xastro_addr,
        &assembly_addr,
        Addr::unchecked("user0"),
        Some(vec![ProposalMessage {
            order: Uint64::from(0u32),
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: assembly_addr.to_string(),
                msg: to_binary(&ExecuteMsg::UpdateConfig(UpdateConfig {
                    xastro_token_addr: None,
                    vxastro_token_addr: None,
                    voting_escrow_delegator_addr: None,
                    builder_unlock_addr: None,
                    proposal_voting_period: Some(750),
                    proposal_effective_delay: None,
                    proposal_expiration_period: None,
                    proposal_required_deposit: None,
                    proposal_required_quorum: None,
                    proposal_required_threshold: None,
                    whitelist_add: None,
                    whitelist_remove: None,
                }))
                .unwrap(),
                funds: vec![],
            }),
        }]),
    );
    // Mint user2's tokens at the same block to increase total supply and add voting power to try to cast vote.
    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &Addr::unchecked("user2"),
        5000_000000,
    );

    app.update_block(next_block);

    // user1 can vote as he had voting power before the proposal submitting.
    cast_vote(
        &mut app,
        assembly_addr.clone(),
        1,
        Addr::unchecked("user1"),
        ProposalVoteOption::For,
    )
    .unwrap();
    // Should panic, because user2 doesn't have any voting power.
    let err = cast_vote(
        &mut app,
        assembly_addr.clone(),
        1,
        Addr::unchecked("user2"),
        ProposalVoteOption::Against,
    )
    .unwrap_err();

    // user2 doesn't have voting power and doesn't affect on total voting power(total supply at)
    // total supply = 5000
    assert_eq!(
        err.root_cause().to_string(),
        "You don't have any voting power!"
    );

    app.update_block(next_block);

    // Skip voting period and delay
    app.update_block(|bi| {
        bi.height += PROPOSAL_VOTING_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1;
        bi.time = bi
            .time
            .plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1));
    });

    // End proposal
    app.execute_contract(
        Addr::unchecked("user0"),
        assembly_addr.clone(),
        &ExecuteMsg::EndProposal { proposal_id: 1 },
        &[],
    )
    .unwrap();

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    // Check proposal votes
    assert_eq!(proposal.for_power, Uint128::from(40000_000000u128));
    assert_eq!(proposal.against_power, Uint128::zero());
    // Should be passed, as total_voting_power=5000, for_votes=40000.
    // So user2 didn't affect the result. Because he had to have xASTRO before the vote was submitted.
    assert_eq!(proposal.status, ProposalStatus::Passed);
}

#[test]
fn test_block_height_selection() {
    // Block height is 12345 after app initialization
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let user1 = Addr::unchecked("user1");
    let user2 = Addr::unchecked("user2");
    let user3 = Addr::unchecked("user3");

    let (_, staking_instance, xastro_addr, _, _, assembly_addr, _) =
        instantiate_contracts(&mut app, owner, false);

    // Mint tokens for submitting proposal
    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &Addr::unchecked("user0"),
        PROPOSAL_REQUIRED_DEPOSIT,
    );

    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &user1,
        6000_000001,
    );
    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &user2,
        4000_000000,
    );

    // Skip to the next period
    app.update_block(|mut block| {
        block.time = block.time.plus_seconds(WEEK);
        block.height += WEEK / 5;
    });

    // Create proposal
    create_proposal(
        &mut app,
        &xastro_addr,
        &assembly_addr,
        Addr::unchecked("user0"),
        None,
    );

    cast_vote(
        &mut app,
        assembly_addr.clone(),
        1,
        user1,
        ProposalVoteOption::For,
    )
    .unwrap();

    // Mint huge amount of xASTRO. These tokens cannot affect on total supply in proposal 1 because
    // they were minted after proposal.start_block - 1
    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &user3,
        100000_000000,
    );
    // Mint more xASTRO to user2, who will vote against the proposal, what is enough to make proposal unsuccessful.
    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &user2,
        3000_000000,
    );
    // Total voting power should be 20k xASTRO (proposal minimum deposit 10k + 4k + 6k users VP)
    check_total_vp(&mut app, &assembly_addr, 1, 20000_000001);

    cast_vote(
        &mut app,
        assembly_addr.clone(),
        1,
        user2,
        ProposalVoteOption::Against,
    )
    .unwrap();

    // Skip voting period
    app.update_block(|bi| {
        bi.height += PROPOSAL_VOTING_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1;
        bi.time = bi
            .time
            .plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1));
    });

    // End proposal
    app.execute_contract(
        Addr::unchecked("user0"),
        assembly_addr.clone(),
        &ExecuteMsg::EndProposal { proposal_id: 1 },
        &[],
    )
    .unwrap();

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    assert_eq!(proposal.for_power, Uint128::new(6000_000001));
    // Against power is 4000, as user2's balance was increased after proposal.start_block - 1
    // at which everyone's voting power are considered.
    assert_eq!(proposal.against_power, Uint128::new(4000_000000));
    // Proposal is passed, as the total supply was increased after proposal.start_block - 1.
    assert_eq!(proposal.status, ProposalStatus::Passed);
}

#[test]
fn test_unsuccessful_proposal() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    let (_, staking_instance, xastro_addr, _, _, assembly_addr, _) =
        instantiate_contracts(&mut app, owner, false);

    // Init voting power for users
    let xastro_balances: Vec<(&str, u128)> = vec![
        ("user0", PROPOSAL_REQUIRED_DEPOSIT), // proposal submitter
        ("user1", 100),
        ("user2", 200),
        ("user3", 400),
        ("user4", 250),
        ("user5", 90),
        ("user6", 300),
        ("user7", 30),
        ("user8", 180),
        ("user9", 50),
        ("user10", 90),
        ("user11", 500),
    ];

    for (addr, xastro) in xastro_balances {
        mint_tokens(
            &mut app,
            &staking_instance,
            &xastro_addr,
            &Addr::unchecked(addr),
            xastro,
        );
    }

    // Skip period
    app.update_block(|mut block| {
        block.time = block.time.plus_seconds(WEEK);
        block.height += WEEK / 5;
    });

    // Create proposal
    create_proposal(
        &mut app,
        &xastro_addr,
        &assembly_addr,
        Addr::unchecked("user0"),
        None,
    );

    let expected_voting_power: Vec<(&str, ProposalVoteOption)> = vec![
        ("user1", ProposalVoteOption::For),
        ("user2", ProposalVoteOption::For),
        ("user3", ProposalVoteOption::For),
        ("user4", ProposalVoteOption::Against),
        ("user5", ProposalVoteOption::Against),
        ("user6", ProposalVoteOption::Against),
        ("user7", ProposalVoteOption::Against),
        ("user8", ProposalVoteOption::Against),
        ("user9", ProposalVoteOption::Against),
        ("user10", ProposalVoteOption::Against),
    ];

    for (addr, option) in expected_voting_power {
        cast_vote(
            &mut app,
            assembly_addr.clone(),
            1,
            Addr::unchecked(addr),
            option,
        )
        .unwrap();
    }

    // Skip voting period
    app.update_block(|bi| {
        bi.height += PROPOSAL_VOTING_PERIOD + 1;
        bi.time = bi.time.plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + 1));
    });

    // Check balance of submitter before and after proposal completion
    check_token_balance(&mut app, &xastro_addr, &Addr::unchecked("user0"), 0);

    app.execute_contract(
        Addr::unchecked("user0"),
        assembly_addr.clone(),
        &ExecuteMsg::EndProposal { proposal_id: 1 },
        &[],
    )
    .unwrap();

    check_token_balance(
        &mut app,
        &xastro_addr,
        &Addr::unchecked("user0"),
        10000_000000,
    );

    // Check proposal status
    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    assert_eq!(proposal.status, ProposalStatus::Rejected);

    // Remove expired proposal
    app.update_block(|bi| {
        bi.height += PROPOSAL_EXPIRATION_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1;
        bi.time = bi
            .time
            .plus_seconds(5 * (PROPOSAL_EXPIRATION_PERIOD + PROPOSAL_EFFECTIVE_DELAY + 1));
    });

    app.execute_contract(
        Addr::unchecked("user0"),
        assembly_addr.clone(),
        &ExecuteMsg::RemoveCompletedProposal { proposal_id: 1 },
        &[],
    )
    .unwrap();

    let res: ProposalListResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.to_string(),
            &QueryMsg::Proposals {
                start: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(res.proposal_list, vec![]);
    // proposal_count should not be changed after removing
    assert_eq!(res.proposal_count, Uint64::from(1u32));
}

#[test]
fn test_check_messages() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let (_, _, _, vxastro_addr, _, assembly_addr, _) =
        instantiate_contracts(&mut app, owner, false);

    change_owner(&mut app, &vxastro_addr, &assembly_addr);
    let user = Addr::unchecked("user");
    let into_check_msg = |msgs: Vec<(String, Binary)>| {
        let messages = msgs
            .into_iter()
            .enumerate()
            .map(|(i, (contract_addr, msg))| ProposalMessage {
                order: Uint64::from(i as u64),
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    funds: vec![],
                }),
            })
            .collect();
        ExecuteMsg::CheckMessages { messages }
    };

    let config_before: astroport_governance::voting_escrow::ConfigResponse = app
        .wrap()
        .query_wasm_smart(
            &vxastro_addr,
            &astroport_governance::voting_escrow::QueryMsg::Config {},
        )
        .unwrap();

    let vxastro_blacklist_msg = vec![(
        vxastro_addr.to_string(),
        to_binary(
            &astroport_governance::voting_escrow::ExecuteMsg::UpdateConfig { new_guardian: None },
        )
        .unwrap(),
    )];
    let err = app
        .execute_contract(
            user.clone(),
            assembly_addr.clone(),
            &into_check_msg(vxastro_blacklist_msg),
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.root_cause().to_string(),
        "Messages check passed. Nothing was committed to the blockchain"
    );

    let config_after: astroport_governance::voting_escrow::ConfigResponse = app
        .wrap()
        .query_wasm_smart(
            &vxastro_addr,
            &astroport_governance::voting_escrow::QueryMsg::Config {},
        )
        .unwrap();
    assert_eq!(config_before, config_after);
}

#[test]
fn test_delegated_vp() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    let (_, staking_instance, xastro_addr, vxastro_addr, _, assembly_addr, delegator) =
        instantiate_contracts(&mut app, owner, true);
    let delegator = delegator.unwrap();

    let users = vec![
        (
            "user1",
            103_000_000_000u128,
            1000u16,
            "user4",
            177_278_846_150u128,
        ),
        (
            "user2",
            612_000_000_000u128,
            2000u16,
            "user5",
            1_053_346_153_800u128,
        ),
        (
            "user3",
            205_000_000_000u128,
            3000u16,
            "user6",
            352_836_538_450u128,
        ),
    ];

    // Mint tokens for submitting proposal
    mint_tokens(
        &mut app,
        &staking_instance,
        &xastro_addr,
        &Addr::unchecked("user0"),
        PROPOSAL_REQUIRED_DEPOSIT,
    );

    // Mint vxASTRO and delegate it to the other users
    for (from, amount, bps, to, exp_vp) in users {
        mint_vxastro(
            &mut app,
            &staking_instance,
            xastro_addr.clone(),
            &vxastro_addr,
            Addr::unchecked(from),
            amount,
        );
        delegate_vxastro(
            &mut app,
            delegator.clone(),
            Addr::unchecked(from),
            Addr::unchecked(to),
            bps,
        );

        let from_amount: Uint128 = app
            .wrap()
            .query_wasm_smart(
                &delegator,
                &DelegatorQueryMsg::AdjustedBalance {
                    account: from.to_string(),
                    timestamp: None,
                },
            )
            .unwrap();

        let to_amount: Uint128 = app
            .wrap()
            .query_wasm_smart(
                &delegator,
                &DelegatorQueryMsg::AdjustedBalance {
                    account: to.to_string(),
                    timestamp: None,
                },
            )
            .unwrap();

        assert_eq!(from_amount + to_amount, Uint128::from(exp_vp));
    }

    app.update_block(|mut block| {
        block.time = block.time.plus_seconds(WEEK);
        block.height += WEEK / 5;
    });

    // Create proposal
    create_proposal(
        &mut app,
        &xastro_addr,
        &assembly_addr,
        Addr::unchecked("user0"),
        Some(vec![ProposalMessage {
            order: Uint64::from(0u32),
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: assembly_addr.to_string(),
                msg: to_binary(&ExecuteMsg::UpdateConfig(UpdateConfig {
                    xastro_token_addr: None,
                    vxastro_token_addr: None,
                    voting_escrow_delegator_addr: None,
                    builder_unlock_addr: None,
                    proposal_voting_period: Some(750),
                    proposal_effective_delay: None,
                    proposal_expiration_period: None,
                    proposal_required_deposit: None,
                    proposal_required_quorum: None,
                    proposal_required_threshold: None,
                    whitelist_add: None,
                    whitelist_remove: None,
                }))
                .unwrap(),
                funds: vec![],
            }),
        }]),
    );

    let votes: Vec<(&str, ProposalVoteOption)> = vec![
        ("user1", ProposalVoteOption::Against),
        ("user2", ProposalVoteOption::For),
        ("user3", ProposalVoteOption::Against),
        ("user4", ProposalVoteOption::For),
        ("user5", ProposalVoteOption::Against),
        ("user6", ProposalVoteOption::For),
    ];

    for (user, vote) in votes {
        cast_vote(
            &mut app,
            assembly_addr.clone(),
            1u64,
            Addr::unchecked(user),
            vote,
        )
        .unwrap();
    }

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    assert_eq!(proposal.for_power, Uint128::from(1_578_255_769_188u128));
    assert_eq!(proposal.against_power, Uint128::from(925_205_769_212u128));

    // Skip voting period
    app.update_block(|bi| {
        bi.height += PROPOSAL_VOTING_PERIOD + 1;
        bi.time = bi.time.plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + 1));
    });

    app.execute_contract(
        Addr::unchecked("user0"),
        assembly_addr.clone(),
        &ExecuteMsg::EndProposal { proposal_id: 1 },
        &[],
    )
    .unwrap();

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    assert_eq!(proposal.status, ProposalStatus::Passed);
}

fn mock_app() -> App {
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(EPOCH_START);
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .build(|_, _, _| {})
}

fn instantiate_contracts(
    router: &mut App,
    owner: Addr,
    with_delegator: bool,
) -> (Addr, Addr, Addr, Addr, Addr, Addr, Option<Addr>) {
    let token_addr = instantiate_astro_token(router, &owner);
    let (staking_addr, xastro_token_addr) = instantiate_xastro_token(router, &owner, &token_addr);
    let vxastro_token_addr = instantiate_vxastro_token(router, &owner, &xastro_token_addr);
    let builder_unlock_addr = instantiate_builder_unlock_contract(router, &owner, &token_addr);

    let mut delegator_addr = None;

    if with_delegator {
        delegator_addr = Some(instantiate_delegator_contract(
            router,
            &owner,
            &vxastro_token_addr,
        ));
    }

    let assembly_addr = instantiate_assembly_contract(
        router,
        &owner,
        &xastro_token_addr,
        &vxastro_token_addr,
        &builder_unlock_addr,
        delegator_addr.clone().map(String::from),
    );

    assert_eq!(Addr::unchecked("contract0"), token_addr);
    assert_eq!(Addr::unchecked("contract1"), staking_addr);
    assert_eq!(Addr::unchecked("contract2"), xastro_token_addr);
    assert_eq!(Addr::unchecked("contract3"), vxastro_token_addr);
    assert_eq!(Addr::unchecked("contract4"), builder_unlock_addr);

    (
        token_addr,
        staking_addr,
        xastro_token_addr,
        vxastro_token_addr,
        builder_unlock_addr,
        assembly_addr,
        delegator_addr,
    )
}

fn instantiate_astro_token(router: &mut App, owner: &Addr) -> Addr {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let astro_token_code_id = router.store_code(astro_token_contract);

    let msg = TokenInstantiateMsg {
        name: String::from("Astro token"),
        symbol: String::from("ASTRO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        marketing: None,
    };

    router
        .instantiate_contract(
            astro_token_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ASTRO"),
            None,
        )
        .unwrap()
}

fn instantiate_xastro_token(router: &mut App, owner: &Addr, astro_token: &Addr) -> (Addr, Addr) {
    let xastro_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_xastro_token::contract::execute,
        astroport_xastro_token::contract::instantiate,
        astroport_xastro_token::contract::query,
    ));

    let xastro_code_id = router.store_code(xastro_contract);

    let staking_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_staking::contract::execute,
            astroport_staking::contract::instantiate,
            astroport_staking::contract::query,
        )
        .with_reply_empty(astroport_staking::contract::reply),
    );

    let staking_code_id = router.store_code(staking_contract);

    let msg = astroport::staking::InstantiateMsg {
        owner: owner.to_string(),
        token_code_id: xastro_code_id,
        deposit_token_addr: astro_token.to_string(),
        marketing: None,
    };
    let staking_instance = router
        .instantiate_contract(
            staking_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("xASTRO"),
            None,
        )
        .unwrap();

    let res = router
        .wrap()
        .query::<astroport::staking::ConfigResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: staking_instance.to_string(),
            msg: to_binary(&astroport::staking::QueryMsg::Config {}).unwrap(),
        }))
        .unwrap();

    (staking_instance, res.share_token_addr)
}

fn instantiate_vxastro_token(router: &mut App, owner: &Addr, xastro: &Addr) -> Addr {
    let vxastro_token_contract = Box::new(ContractWrapper::new_with_empty(
        voting_escrow::contract::execute,
        voting_escrow::contract::instantiate,
        voting_escrow::contract::query,
    ));

    let vxastro_token_code_id = router.store_code(vxastro_token_contract);

    let msg = VXAstroInstantiateMsg {
        owner: owner.to_string(),
        guardian_addr: Some(owner.to_string()),
        deposit_token_addr: xastro.to_string(),
        marketing: None,
        logo_urls_whitelist: vec![],
    };

    router
        .instantiate_contract(
            vxastro_token_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("vxASTRO"),
            None,
        )
        .unwrap()
}

fn instantiate_builder_unlock_contract(router: &mut App, owner: &Addr, astro_token: &Addr) -> Addr {
    let builder_unlock_contract = Box::new(ContractWrapper::new_with_empty(
        builder_unlock::contract::execute,
        builder_unlock::contract::instantiate,
        builder_unlock::contract::query,
    ));

    let builder_unlock_code_id = router.store_code(builder_unlock_contract);

    let msg = BuilderUnlockInstantiateMsg {
        owner: owner.to_string(),
        astro_token: astro_token.to_string(),
        max_allocations_amount: Uint128::new(300_000_000_000_000u128),
    };

    router
        .instantiate_contract(
            builder_unlock_code_id,
            owner.clone(),
            &msg,
            &[],
            "Builder Unlock contract".to_string(),
            Some(owner.to_string()),
        )
        .unwrap()
}

fn instantiate_assembly_contract(
    router: &mut App,
    owner: &Addr,
    xastro: &Addr,
    vxastro: &Addr,
    builder: &Addr,
    delegator: Option<String>,
) -> Addr {
    let assembly_contract = Box::new(ContractWrapper::new_with_empty(
        astro_assembly::contract::execute,
        astro_assembly::contract::instantiate,
        astro_assembly::contract::query,
    ));

    let assembly_code = router.store_code(assembly_contract);

    let msg = InstantiateMsg {
        xastro_token_addr: xastro.to_string(),
        vxastro_token_addr: Some(vxastro.to_string()),
        voting_escrow_delegator_addr: delegator,
        builder_unlock_addr: builder.to_string(),
        proposal_voting_period: PROPOSAL_VOTING_PERIOD,
        proposal_effective_delay: PROPOSAL_EFFECTIVE_DELAY,
        proposal_expiration_period: PROPOSAL_EXPIRATION_PERIOD,
        proposal_required_deposit: Uint128::new(PROPOSAL_REQUIRED_DEPOSIT),
        proposal_required_quorum: String::from(PROPOSAL_REQUIRED_QUORUM),
        proposal_required_threshold: String::from(PROPOSAL_REQUIRED_THRESHOLD),
        whitelisted_links: vec!["https://some.link/".to_string()],
    };

    router
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &msg,
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap()
}

fn instantiate_delegator_contract(router: &mut App, owner: &Addr, vxastro: &Addr) -> Addr {
    let nft_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_nft::contract::execute,
        astroport_nft::contract::instantiate,
        astroport_nft::contract::query,
    ));

    let nft_code_id = router.store_code(nft_contract);

    let delegator_contract = Box::new(
        ContractWrapper::new_with_empty(
            voting_escrow_delegation::contract::execute,
            voting_escrow_delegation::contract::instantiate,
            voting_escrow_delegation::contract::query,
        )
        .with_reply_empty(voting_escrow_delegation::contract::reply),
    );

    let delegator_code_id = router.store_code(delegator_contract);

    let msg = DelegatorInstantiateMsg {
        owner: owner.to_string(),
        nft_code_id,
        voting_escrow_addr: vxastro.to_string(),
    };

    router
        .instantiate_contract(
            delegator_code_id,
            owner.clone(),
            &msg,
            &[],
            "Voting Escrow Delegator",
            Some(owner.to_string()),
        )
        .unwrap()
}

fn mint_tokens(app: &mut App, minter: &Addr, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(minter.clone(), token.to_owned(), &msg, &[])
        .unwrap();
}

fn mint_vxastro(
    app: &mut App,
    staking_instance: &Addr,
    xastro: Addr,
    vxastro: &Addr,
    recipient: Addr,
    amount: u128,
) {
    mint_tokens(
        app,
        staking_instance,
        &xastro.clone(),
        &recipient.clone(),
        amount,
    );

    let msg = Cw20ExecuteMsg::Send {
        contract: vxastro.to_string(),
        amount: Uint128::from(amount),
        msg: to_binary(&VXAstroCw20HookMsg::CreateLock { time: WEEK * 50 }).unwrap(),
    };

    app.execute_contract(recipient, xastro, &msg, &[]).unwrap();
}

fn delegate_vxastro(app: &mut App, delegator_addr: Addr, from: Addr, to: Addr, bps: u16) {
    let msg = DelegatorExecuteMsg::CreateDelegation {
        bps,
        expire_time: 2 * 7 * 86400,
        token_id: format!("{}-{}-{}", from, to, bps),
        recipient: to.to_string(),
    };

    app.execute_contract(from.clone(), delegator_addr, &msg, &[])
        .unwrap();
}

fn create_allocations(
    app: &mut App,
    token: Addr,
    builder_unlock_contract_addr: Addr,
    allocations: Vec<(String, AllocationParams)>,
) {
    let amount = allocations
        .iter()
        .map(|params| params.1.amount.u128())
        .sum();

    mint_tokens(
        app,
        &Addr::unchecked("owner"),
        &token,
        &Addr::unchecked("owner"),
        amount,
    );

    app.execute_contract(
        Addr::unchecked("owner"),
        Addr::unchecked(token.to_string()),
        &Cw20ExecuteMsg::Send {
            contract: builder_unlock_contract_addr.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&BuilderUnlockReceiveMsg::CreateAllocations { allocations }).unwrap(),
        },
        &[],
    )
    .unwrap();
}

fn create_proposal(
    app: &mut App,
    token: &Addr,
    assembly: &Addr,
    submitter: Addr,
    msgs: Option<Vec<ProposalMessage>>,
) {
    let submit_proposal_msg = Cw20HookMsg::SubmitProposal {
        title: "Test title!".to_string(),
        description: "Test description!".to_string(),
        link: None,
        messages: msgs,
    };

    app.execute_contract(
        submitter,
        token.clone(),
        &Cw20ExecuteMsg::Send {
            contract: assembly.to_string(),
            amount: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
            msg: to_binary(&submit_proposal_msg).unwrap(),
        },
        &[],
    )
    .unwrap();
}

fn check_token_balance(app: &mut App, token: &Addr, address: &Addr, expected: u128) {
    let msg = XAstroQueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}

fn check_user_vp(app: &mut App, assembly: &Addr, address: &Addr, proposal_id: u64, expected: u128) {
    let res: Uint128 = app
        .wrap()
        .query_wasm_smart(
            assembly.to_string(),
            &QueryMsg::UserVotingPower {
                user: address.to_string(),
                proposal_id,
            },
        )
        .unwrap();

    assert_eq!(res.u128(), expected);
}

fn check_total_vp(app: &mut App, assembly: &Addr, proposal_id: u64, expected: u128) {
    let res: Uint128 = app
        .wrap()
        .query_wasm_smart(
            assembly.to_string(),
            &QueryMsg::TotalVotingPower { proposal_id },
        )
        .unwrap();

    assert_eq!(res.u128(), expected);
}

fn cast_vote(
    app: &mut App,
    assembly: Addr,
    proposal_id: u64,
    sender: Addr,
    option: ProposalVoteOption,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        sender,
        assembly,
        &ExecuteMsg::CastVote {
            proposal_id,
            vote: option,
        },
        &[],
    )
}

fn change_owner(app: &mut App, contract: &Addr, assembly: &Addr) {
    let msg = astroport_governance::voting_escrow::ExecuteMsg::ProposeNewOwner {
        new_owner: assembly.to_string(),
        expires_in: 100,
    };
    app.execute_contract(Addr::unchecked("owner"), contract.clone(), &msg, &[])
        .unwrap();

    app.execute_contract(
        assembly.clone(),
        contract.clone(),
        &astroport_governance::voting_escrow::ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();
}
