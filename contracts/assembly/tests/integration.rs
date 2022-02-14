use astroport::{
    token::InstantiateMsg as TokenInstantiateMsg,
    xastro_token::InstantiateMsg as XAstroInstantiateMsg, xastro_token::QueryMsg as XAstroQueryMsg,
};

use astroport_governance::assembly::{
    Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, Proposal, ProposalListResponse,
    ProposalMessage, ProposalStatus, ProposalVoteOption, ProposalVotesResponse, QueryMsg,
    UpdateConfig,
};
use astroport_governance::builder_unlock::msg::{
    InstantiateMsg as BuilderUnlockInstantiateMsg, ReceiveMsg as BuilderUnlockReceiveMsg,
};
use astroport_governance::builder_unlock::{AllocationParams, Schedule};
use cosmwasm_std::{
    testing::{mock_env, MockApi, MockStorage},
    to_binary, Addr, CosmosMsg, Decimal, StdResult, Uint128, Uint64, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, MinterResponse};
use terra_multi_test::{
    next_block, AppBuilder, AppResponse, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock,
};

const PROPOSAL_VOTING_PERIOD: u64 = 500;
const PROPOSAL_EFFECTIVE_DELAY: u64 = 50;
const PROPOSAL_EXPIRATION_PERIOD: u64 = 400;
const PROPOSAL_REQUIRED_DEPOSIT: u128 = 1000u128;
const PROPOSAL_REQUIRED_QUORUM: u64 = 55;
const PROPOSAL_REQUIRED_THRESHOLD: u64 = 60;

#[test]
fn proper_contract_instantiation() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    // Instantiate needed contracts
    let token_addr = instantiate_astro_token(&mut app, &owner);
    let xastro_token_addr = instantiate_xastro_token(&mut app, &owner);
    let builder_unlock_addr = instantiate_builder_unlock_contract(&mut app, &owner, &token_addr);

    let assembly_contract = Box::new(ContractWrapper::new_with_empty(
        astro_assembly::contract::execute,
        astro_assembly::contract::instantiate,
        astro_assembly::contract::query,
    ));

    let assembly_code = app.store_code(assembly_contract);

    let assembly_default_instantiate_msg = InstantiateMsg {
        xastro_token_addr: xastro_token_addr.to_string(),
        builder_unlock_addr: builder_unlock_addr.to_string(),
        proposal_voting_period: PROPOSAL_VOTING_PERIOD,
        proposal_effective_delay: PROPOSAL_EFFECTIVE_DELAY,
        proposal_expiration_period: PROPOSAL_EXPIRATION_PERIOD,
        proposal_required_deposit: Uint128::from(PROPOSAL_REQUIRED_DEPOSIT),
        proposal_required_quorum: PROPOSAL_REQUIRED_QUORUM,
        proposal_required_threshold: PROPOSAL_REQUIRED_THRESHOLD,
    };

    // Try to instantiate assembly with wrong threshold
    let res = app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_required_threshold: 40,
                ..assembly_default_instantiate_msg.clone()
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        res.to_string(),
        "Generic error: The required threshold for a proposal cannot be lower than 50% or higher than 100%"
    );

    let res = app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_required_threshold: 110,
                ..assembly_default_instantiate_msg.clone()
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        res.to_string(),
        "Generic error: The required threshold for a proposal cannot be lower than 50% or higher than 100%"
    );

    let res = app
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &InstantiateMsg {
                proposal_required_quorum: 110,
                ..assembly_default_instantiate_msg.clone()
            },
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap_err();

    assert_eq!(
        res.to_string(),
        "Generic error: The required quorum for a proposal cannot be higher than 100%"
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
        Decimal::percent(PROPOSAL_REQUIRED_QUORUM)
    );
    assert_eq!(
        res.proposal_required_threshold,
        Decimal::percent(PROPOSAL_REQUIRED_THRESHOLD)
    );
}

#[test]
fn proper_proposal_submitting() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user1");

    let (_, xastro_addr, _, assembly_addr) = instantiate_contracts(&mut app, owner);

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

    mint_tokens(&mut app, &xastro_addr, &user, 2000);

    check_token_balance(&mut app, &xastro_addr, &user, 2000);

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

    let res = app
        .execute_contract(user.clone(), xastro_addr.clone(), &submit_proposal_msg, &[])
        .unwrap_err();

    assert_eq!(res.to_string(), "Insufficient token deposit!");

    // Try to create a proposal with wrong title
    let res = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("X"),
                    description: String::from("Description"),
                    link: Some(String::from("https://some.link")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(1000u128),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Title too short");

    let res = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from_utf8(vec![b'X'; 65]).unwrap(),
                    description: String::from("Description"),
                    link: Some(String::from("https://some.link")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(1000u128),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Title too long");

    // Try to create a proposal with wrong description
    let res = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("Title"),
                    description: String::from("X"),
                    link: Some(String::from("https://some.link")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(1000u128),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Description too short");

    let res = app
        .execute_contract(
            user.clone(),
            xastro_addr.clone(),
            &Cw20ExecuteMsg::Send {
                contract: assembly_addr.to_string(),
                msg: to_binary(&Cw20HookMsg::SubmitProposal {
                    title: String::from("Title"),
                    description: String::from_utf8(vec![b'X'; 1025]).unwrap(),
                    link: Some(String::from("https://some.link")),
                    messages: None,
                })
                .unwrap(),
                amount: Uint128::from(1000u128),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Description too long");

    // Try to create a proposal with wrong link
    let res = app
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
                amount: Uint128::from(1000u128),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Link too short");

    let res = app
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
                amount: Uint128::from(1000u128),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Link too long");

    // Valid proposal submitting
    app.execute_contract(
        user.clone(),
        xastro_addr.clone(),
        &Cw20ExecuteMsg::Send {
            contract: assembly_addr.to_string(),
            msg: to_binary(&Cw20HookMsg::SubmitProposal {
                title: String::from("Title"),
                description: String::from("Description"),
                link: Some(String::from("https://some.link")),
                messages: Some(vec![ProposalMessage {
                    order: Uint64::from(0u32),
                    msg: CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: assembly_addr.to_string(),
                        msg: to_binary(&ExecuteMsg::UpdateConfig(UpdateConfig {
                            xastro_token_addr: None,
                            builder_unlock_addr: None,
                            proposal_voting_period: Some(750),
                            proposal_effective_delay: None,
                            proposal_expiration_period: None,
                            proposal_required_deposit: None,
                            proposal_required_quorum: None,
                            proposal_required_threshold: None,
                        }))
                        .unwrap(),
                        funds: vec![],
                    }),
                }]),
            })
            .unwrap(),
            amount: Uint128::from(1000u128),
        },
        &[],
    )
    .unwrap();

    let proposal: Proposal = app
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
    assert_eq!(proposal.for_voters, Vec::<Addr>::new());
    assert_eq!(proposal.against_voters, Vec::<Addr>::new());
    assert_eq!(proposal.start_block, 12_345);
    assert_eq!(proposal.end_block, 12_345 + 500);
    assert_eq!(proposal.title, String::from("Title"));
    assert_eq!(proposal.description, String::from("Description"));
    assert_eq!(proposal.link, Some(String::from("https://some.link")));
    assert_eq!(
        proposal.messages,
        Some(vec![ProposalMessage {
            order: Uint64::from(0u32),
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: assembly_addr.to_string(),
                msg: to_binary(&ExecuteMsg::UpdateConfig(UpdateConfig {
                    xastro_token_addr: None,
                    builder_unlock_addr: None,
                    proposal_voting_period: Some(750),
                    proposal_effective_delay: None,
                    proposal_expiration_period: None,
                    proposal_required_deposit: None,
                    proposal_required_quorum: None,
                    proposal_required_threshold: None,
                }))
                .unwrap(),
                funds: vec![],
            }),
        }])
    );
    assert_eq!(proposal.deposit_amount, Uint128::from(1000u64))
}

#[test]
fn proper_successful_proposal() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    let (token_addr, xastro_addr, builder_unlock_addr, assembly_addr) =
        instantiate_contracts(&mut app, owner);

    // Init voting power for users
    let xastro_balances: Vec<(&str, u128)> = vec![
        ("user0", PROPOSAL_REQUIRED_DEPOSIT), // Proposal submitter
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

    for (addr, xastro) in xastro_balances {
        mint_tokens(&mut app, &xastro_addr, &Addr::unchecked(addr), xastro);
    }

    create_allocations(&mut app, token_addr, builder_unlock_addr, locked_balances);

    // Skip block
    app.update_block(next_block);

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
                    builder_unlock_addr: None,
                    proposal_voting_period: Some(750),
                    proposal_effective_delay: None,
                    proposal_expiration_period: None,
                    proposal_required_deposit: None,
                    proposal_required_quorum: None,
                    proposal_required_threshold: None,
                }))
                .unwrap(),
                funds: vec![],
            }),
        }]),
    );

    let expected_voting_power: Vec<(&str, ProposalVoteOption)> = vec![
        ("user1", ProposalVoteOption::For),
        ("user2", ProposalVoteOption::For),
        ("user3", ProposalVoteOption::For),
        ("user4", ProposalVoteOption::For),
        ("user5", ProposalVoteOption::For),
        ("user6", ProposalVoteOption::For),
        ("user7", ProposalVoteOption::For),
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

    let proposal: Proposal = app
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

    // Check proposal votes
    assert_eq!(proposal.for_power, Uint128::from(1600u32));
    assert_eq!(proposal.against_power, Uint128::from(350u32));

    assert_eq!(proposal_votes.for_power, Uint128::from(1600u32));
    assert_eq!(proposal_votes.against_power, Uint128::from(350u32));

    assert_eq!(
        proposal.for_voters,
        vec![
            Addr::unchecked("user1"),
            Addr::unchecked("user2"),
            Addr::unchecked("user3"),
            Addr::unchecked("user4"),
            Addr::unchecked("user5"),
            Addr::unchecked("user6"),
            Addr::unchecked("user7")
        ]
    );
    assert_eq!(
        proposal.against_voters,
        vec![
            Addr::unchecked("user8"),
            Addr::unchecked("user9"),
            Addr::unchecked("user10")
        ]
    );

    // Skip voting period
    app.update_block(|bi| {
        bi.height += PROPOSAL_VOTING_PERIOD + 1;
        bi.time = bi.time.plus_seconds(5 * (PROPOSAL_VOTING_PERIOD + 1));
    });

    // Try to vote after voting period
    let res = cast_vote(
        &mut app,
        assembly_addr.clone(),
        1,
        Addr::unchecked("user11"),
        ProposalVoteOption::Against,
    )
    .unwrap_err();

    assert_eq!(res.to_string(), "Voting period ended!");

    // Try to execute proposal before end_proposal
    let res = app
        .execute_contract(
            Addr::unchecked("user0"),
            assembly_addr.clone(),
            &ExecuteMsg::ExecuteProposal { proposal_id: 1 },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Proposal not passed!");

    // Check the successful completion of the proposal
    check_token_balance(&mut app, &xastro_addr, &Addr::unchecked("user0"), 0);

    app.execute_contract(
        Addr::unchecked("user0"),
        assembly_addr.clone(),
        &ExecuteMsg::EndProposal { proposal_id: 1 },
        &[],
    )
    .unwrap();

    check_token_balance(&mut app, &xastro_addr, &Addr::unchecked("user0"), 1000);

    let proposal: Proposal = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    assert_eq!(proposal.status, ProposalStatus::Passed);

    // Try to end proposal again.
    let res = app
        .execute_contract(
            Addr::unchecked("user0"),
            assembly_addr.clone(),
            &ExecuteMsg::EndProposal { proposal_id: 1 },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Proposal not active!");

    // Try to execute proposal before delay
    let res = app
        .execute_contract(
            Addr::unchecked("user0"),
            assembly_addr.clone(),
            &ExecuteMsg::ExecuteProposal { proposal_id: 1 },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Proposal delay not ended!");

    // Skip blocks
    app.update_block(|bi| {
        bi.height += PROPOSAL_EFFECTIVE_DELAY + 1;
        bi.time = bi.time.plus_seconds(5 * (PROPOSAL_EFFECTIVE_DELAY + 1));
    });

    // Try to execute proposal after delay
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

    let proposal: Proposal = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.to_string(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    // Check execution result
    assert_eq!(config.proposal_voting_period, 750);
    assert_eq!(proposal.status, ProposalStatus::Executed);

    // Try to remove proposal before expiration period
    let res = app
        .execute_contract(
            Addr::unchecked("user0"),
            assembly_addr.clone(),
            &ExecuteMsg::RemoveCompletedProposal { proposal_id: 1 },
            &[],
        )
        .unwrap_err();

    assert_eq!(res.to_string(), "Proposal not completed!");

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
    // proposal_count should not be changed after removing
    assert_eq!(res.proposal_count, Uint64::from(1u32));
}

#[test]
fn proper_unsuccessful_proposal() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    let (_, xastro_addr, _, assembly_addr) = instantiate_contracts(&mut app, owner);

    // Init voting power for users
    let xastro_balances: Vec<(&str, u128)> = vec![
        ("user0", PROPOSAL_REQUIRED_DEPOSIT), // Proposal submitter
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
        mint_tokens(&mut app, &xastro_addr, &Addr::unchecked(addr), xastro);
    }

    // Skip block
    app.update_block(next_block);

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

    check_token_balance(&mut app, &xastro_addr, &Addr::unchecked("user0"), 1000);

    // Check proposal status
    let proposal: Proposal = app
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

fn mock_app() -> TerraApp {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let custom = TerraMock::luna_ust_case();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .with_custom(custom)
        .build()
}

fn instantiate_contracts(router: &mut TerraApp, owner: Addr) -> (Addr, Addr, Addr, Addr) {
    let token_addr = instantiate_astro_token(router, &owner);
    let xastro_token_addr = instantiate_xastro_token(router, &owner);
    let builder_unlock_addr = instantiate_builder_unlock_contract(router, &owner, &token_addr);
    let assembly_addr =
        instantiate_assembly_contract(router, &owner, &xastro_token_addr, &builder_unlock_addr);

    assert_eq!("contract #0", token_addr);
    assert_eq!("contract #1", xastro_token_addr);
    assert_eq!("contract #2", builder_unlock_addr);
    assert_eq!("contract #3", assembly_addr);

    (
        token_addr,
        xastro_token_addr,
        builder_unlock_addr,
        assembly_addr,
    )
}

fn instantiate_astro_token(router: &mut TerraApp, owner: &Addr) -> Addr {
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

fn instantiate_xastro_token(router: &mut TerraApp, owner: &Addr) -> Addr {
    let xastro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_xastro_token::contract::execute,
        astroport_xastro_token::contract::instantiate,
        astroport_xastro_token::contract::query,
    ));

    let xastro_token_code_id = router.store_code(xastro_token_contract);

    let msg = XAstroInstantiateMsg {
        name: String::from("xAstro token"),
        symbol: String::from("xASTRO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
    };

    router
        .instantiate_contract(
            xastro_token_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("xASTRO"),
            None,
        )
        .unwrap()
}

fn instantiate_builder_unlock_contract(
    router: &mut TerraApp,
    owner: &Addr,
    astro_token: &Addr,
) -> Addr {
    let builder_unlock_contract = Box::new(ContractWrapper::new_with_empty(
        builder_unlock::contract::execute,
        builder_unlock::contract::instantiate,
        builder_unlock::contract::query,
    ));

    let builder_unlock_code_id = router.store_code(builder_unlock_contract);

    let msg = BuilderUnlockInstantiateMsg {
        owner: owner.to_string(),
        astro_token: astro_token.to_string(),
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
    router: &mut TerraApp,
    owner: &Addr,
    xastro: &Addr,
    builder: &Addr,
) -> Addr {
    let assembly_contract = Box::new(ContractWrapper::new_with_empty(
        astro_assembly::contract::execute,
        astro_assembly::contract::instantiate,
        astro_assembly::contract::query,
    ));

    let assembly_code = router.store_code(assembly_contract);

    let msg = InstantiateMsg {
        xastro_token_addr: xastro.to_string(),
        builder_unlock_addr: builder.to_string(),
        proposal_voting_period: PROPOSAL_VOTING_PERIOD,
        proposal_effective_delay: PROPOSAL_EFFECTIVE_DELAY,
        proposal_expiration_period: PROPOSAL_EXPIRATION_PERIOD,
        proposal_required_deposit: Uint128::new(PROPOSAL_REQUIRED_DEPOSIT),
        proposal_required_quorum: PROPOSAL_REQUIRED_QUORUM,
        proposal_required_threshold: PROPOSAL_REQUIRED_THRESHOLD,
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

fn mint_tokens(app: &mut TerraApp, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(Addr::unchecked("owner"), token.to_owned(), &msg, &[])
        .unwrap();
}

fn create_allocations(
    app: &mut TerraApp,
    token: Addr,
    builder_unlock_contract_addr: Addr,
    allocations: Vec<(String, AllocationParams)>,
) {
    let amount = allocations
        .iter()
        .map(|params| params.1.amount.u128())
        .sum();

    mint_tokens(app, &token, &Addr::unchecked("owner"), amount);

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
    app: &mut TerraApp,
    token: &Addr,
    assembly: &Addr,
    submitter: Addr,
    msgs: Option<Vec<ProposalMessage>>,
) {
    let submit_proposal_msg = Cw20HookMsg::SubmitProposal {
        title: "Title".to_string(),
        description: "Description".to_string(),
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

fn check_token_balance(app: &mut TerraApp, token: &Addr, address: &Addr, expected: u128) {
    let msg = XAstroQueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}

fn cast_vote(
    app: &mut TerraApp,
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
