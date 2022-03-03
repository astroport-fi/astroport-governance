use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use astroport_governance::builder_unlock::{AllocationParams, Schedule};

use astroport_governance::builder_unlock::msg::{
    AllocationResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg,
    SimulateWithdrawResponse, StateResponse,
};
use cosmwasm_std::testing::{mock_env, MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, Timestamp, Uint128};
use cw20::BalanceResponse;
use terra_multi_test::{App, BankKeeper, ContractWrapper, Executor, TerraMockQuerier};

const OWNER: &str = "owner";

fn mock_app() -> App {
    let api = MockApi::default();
    let env = mock_env();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let tmq = TerraMockQuerier::new(MockQuerier::new(&[]));

    App::new(api, env.block, bank, storage, tmq)
}

fn init_contracts(app: &mut App) -> (Addr, Addr, InstantiateMsg) {
    // Instantiate ASTRO token contract
    let astro_token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let astro_token_code_id = app.store_code(astro_token_contract);

    let msg = TokenInstantiateMsg {
        name: String::from("Astro token"),
        symbol: String::from("ASTRO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(cw20::MinterResponse {
            minter: OWNER.clone().to_string(),
            cap: None,
        }),
    };

    let astro_token_instance = app
        .instantiate_contract(
            astro_token_code_id,
            Addr::unchecked(OWNER.clone().to_string()),
            &msg,
            &[],
            String::from("ASTRO"),
            None,
        )
        .unwrap();

    // Instantiate the contract
    let unlock_contract = Box::new(ContractWrapper::new(
        builder_unlock::contract::execute,
        builder_unlock::contract::instantiate,
        builder_unlock::contract::query,
    ));

    let unlock_code_id = app.store_code(unlock_contract);

    let unlock_instantiate_msg = InstantiateMsg {
        owner: OWNER.clone().to_string(),
        astro_token: astro_token_instance.to_string(),
    };

    // Init contract
    let unlock_instance = app
        .instantiate_contract(
            unlock_code_id,
            Addr::unchecked(OWNER.clone()),
            &unlock_instantiate_msg,
            &[],
            "unlock",
            None,
        )
        .unwrap();

    (
        unlock_instance,
        astro_token_instance,
        unlock_instantiate_msg,
    )
}

fn mint_some_astro(
    app: &mut App,
    owner: Addr,
    astro_token_instance: Addr,
    amount: Uint128,
    to: String,
) {
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: to.clone(),
        amount: amount,
    };
    let res = app
        .execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[1].attributes[2], attr("to", to));
    assert_eq!(res.events[1].attributes[3], attr("amount", amount));
}

#[test]
fn proper_initialization() {
    let mut app = mock_app();
    let (unlock_instance, _astro_instance, init_msg) = init_contracts(&mut app);

    let resp: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::Config {})
        .unwrap();

    // Check config
    assert_eq!(init_msg.owner, resp.owner);
    assert_eq!(init_msg.astro_token, resp.astro_token);

    // Check state
    let resp: StateResponse = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::State {})
        .unwrap();

    assert_eq!(Uint128::zero(), resp.total_astro_deposited);
    assert_eq!(Uint128::zero(), resp.remaining_astro_tokens);
}

#[test]
fn test_transfer_ownership() {
    let mut app = mock_app();
    let (unlock_instance, _, init_msg) = init_contracts(&mut app);

    // ######    ERROR :: Unauthorized     ######
    let err = app
        .execute_contract(
            Addr::unchecked("not_owner".to_string()),
            unlock_instance.clone(),
            &ExecuteMsg::TransferOwnership {
                new_owner: Some("new_owner".to_string()),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Only the current owner can transfer ownership"
    );

    // ######    SUCCESSFULLY TRANSFERS OWNERSHIP :: UPDATES OWNER    ######
    app.execute_contract(
        Addr::unchecked(OWNER.to_string()),
        unlock_instance.clone(),
        &ExecuteMsg::TransferOwnership {
            new_owner: Some("new_owner".to_string()),
        },
        &[],
    )
    .unwrap();

    let resp: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::Config {})
        .unwrap();

    // Check config
    assert_eq!("new_owner".to_string(), resp.owner);
    assert_eq!(init_msg.astro_token, resp.astro_token);

    // ######    SUCCESSFULLY TRANSFERS OWNERSHIP :: UPDATES REFUND RECIPIENT    ######
    app.execute_contract(
        Addr::unchecked("new_owner".to_string()),
        unlock_instance.clone(),
        &ExecuteMsg::TransferOwnership { new_owner: None },
        &[],
    )
    .unwrap();

    let resp: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::Config {})
        .unwrap();

    // Check config
    assert_eq!("new_owner".to_string(), resp.owner);
    assert_eq!(init_msg.astro_token, resp.astro_token);
}

#[test]
fn test_create_allocations() {
    let mut app = mock_app();
    let (unlock_instance, astro_instance, _) = init_contracts(&mut app);

    mint_some_astro(
        &mut app,
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        Uint128::new(1_000_000_000_000000),
        OWNER.to_string(),
    );

    let mut allocations: Vec<(String, AllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "team_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));

    // ######    ERROR :: Only owner can create allocations     ######
    mint_some_astro(
        &mut app,
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        Uint128::new(1_000),
        "not_owner".to_string(),
    );

    let mut err = app
        .execute_contract(
            Addr::unchecked("not_owner".to_string()),
            astro_instance.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: unlock_instance.clone().to_string(),
                amount: Uint128::from(1_000u64),
                msg: to_binary(&ReceiveMsg::CreateAllocations {
                    allocations: allocations.clone(),
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Only the contract owner can create allocations"
    );

    // ######    ERROR :: Only ASTRO can be can be deposited     ######
    // Instantiate the ASTRO token contract
    let not_astro_token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let not_astro_token_code_id = app.store_code(not_astro_token_contract);

    let msg = TokenInstantiateMsg {
        name: String::from("Astro Token"),
        symbol: String::from("ASTRO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(cw20::MinterResponse {
            minter: OWNER.clone().to_string(),
            cap: None,
        }),
    };

    let not_astro_token_instance = app
        .instantiate_contract(
            not_astro_token_code_id,
            Addr::unchecked(OWNER.clone().to_string()),
            &msg,
            &[],
            String::from("FAKE_ASTRO"),
            None,
        )
        .unwrap();

    app.execute_contract(
        Addr::unchecked(OWNER.clone()),
        not_astro_token_instance.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: OWNER.clone().to_string(),
            amount: Uint128::from(15_000_000_000000u64),
        },
        &[],
    )
    .unwrap();

    err = app
        .execute_contract(
            Addr::unchecked(OWNER.clone()),
            not_astro_token_instance.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: unlock_instance.clone().to_string(),
                amount: Uint128::from(15_000_000_000000u64),
                msg: to_binary(&ReceiveMsg::CreateAllocations {
                    allocations: allocations.clone(),
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Only ASTRO can be deposited"
    );

    // ######    ERROR :: ASTRO deposit amount mismatch     ######
    err = app
        .execute_contract(
            Addr::unchecked(OWNER.clone()),
            astro_instance.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: unlock_instance.clone().to_string(),
                amount: Uint128::from(15_000_000_000001u64),
                msg: to_binary(&ReceiveMsg::CreateAllocations {
                    allocations: allocations.clone(),
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: ASTRO deposit amount mismatch"
    );

    // ######    SUCCESSFULLY CREATES ALLOCATIONS    ######
    app.execute_contract(
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        &cw20::Cw20ExecuteMsg::Send {
            contract: unlock_instance.clone().to_string(),
            amount: Uint128::from(15_000_000_000000u64),
            msg: to_binary(&ReceiveMsg::CreateAllocations {
                allocations: allocations.clone(),
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    // Check state
    let resp: StateResponse = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::State {})
        .unwrap();
    assert_eq!(
        resp.total_astro_deposited,
        Uint128::from(15_000_000_000000u64)
    );
    assert_eq!(
        resp.remaining_astro_tokens,
        Uint128::from(15_000_000_000000u64)
    );

    // Check allocation #1
    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(
        resp.params.unlock_schedule,
        Schedule {
            start_time: 1642402274u64,
            cliff: 0u64,
            duration: 31536000u64
        }
    );

    // Check allocation #2
    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "advisor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(
        resp.params.unlock_schedule,
        Schedule {
            start_time: 1642402274u64,
            cliff: 7776000u64,
            duration: 31536000u64
        }
    );

    // Check allocation #3
    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "team_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(
        resp.params.unlock_schedule,
        Schedule {
            start_time: 1642402274u64,
            cliff: 7776000u64,
            duration: 31536000u64
        }
    );

    // ######    ERROR :: Allocation already exists for user {}     ######
    err = app
        .execute_contract(
            Addr::unchecked(OWNER.clone()),
            astro_instance.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: unlock_instance.clone().to_string(),
                amount: Uint128::from(5_000_000_000000u64),
                msg: to_binary(&ReceiveMsg::CreateAllocations {
                    allocations: vec![allocations[0].clone()],
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Allocation (params) already exists for investor_1"
    );
}

#[test]
fn test_withdraw() {
    let mut app = mock_app();
    let (unlock_instance, astro_instance, _) = init_contracts(&mut app);

    mint_some_astro(
        &mut app,
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        Uint128::new(1_000_000_000_000000),
        OWNER.to_string(),
    );

    let mut allocations: Vec<(String, AllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "team_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));

    // SUCCESSFULLY CREATES ALLOCATIONS
    app.execute_contract(
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        &cw20::Cw20ExecuteMsg::Send {
            contract: unlock_instance.clone().to_string(),
            amount: Uint128::from(15_000_000_000000u64),
            msg: to_binary(&ReceiveMsg::CreateAllocations {
                allocations: allocations.clone(),
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    // ######    ERROR :: Allocation doesn't exist    ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER.clone()),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "astroport_governance::builder_unlock::AllocationParams not found"
    );

    // ######   SUCCESSFULLY WITHDRAWS ASTRO #1   ######
    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1642402275)
    });

    let astro_bal_before: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_instance,
            &cw20::Cw20QueryMsg::Balance {
                address: "investor_1".to_string(),
            },
        )
        .unwrap();

    app.execute_contract(
        Addr::unchecked("investor_1".clone()),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    // Check state
    let state_resp: StateResponse = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::State {})
        .unwrap();
    assert_eq!(
        state_resp.total_astro_deposited,
        Uint128::from(15_000_000_000000u64)
    );
    assert_eq!(
        state_resp.remaining_astro_tokens,
        Uint128::from(14_999_999_841452u64)
    );

    // Check allocation #1
    let alloc_resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(alloc_resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(alloc_resp.status.astro_withdrawn, Uint128::from(158548u64));

    let astro_bal_after: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_instance,
            &cw20::Cw20QueryMsg::Balance {
                address: "investor_1".to_string(),
            },
        )
        .unwrap();

    assert_eq!(
        astro_bal_after.balance - astro_bal_before.balance,
        alloc_resp.status.astro_withdrawn
    );

    // Check the number of unlocked tokens
    let mut unlock_resp: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::UnlockedTokens {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(unlock_resp, Uint128::from(158548u64));

    // ######    ERROR :: No unlocked ASTRO to be withdrawn   ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1".clone()),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: No unlocked ASTRO to be withdrawn"
    );

    // ######   SUCCESSFULLY WITHDRAWS ASTRO #2   ######
    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1642402285)
    });

    // Check the number of unlocked tokens
    unlock_resp = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::UnlockedTokens {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(unlock_resp, Uint128::from(1744038u64));

    // Check the number of tokens that can be withdrawn from the contract right now
    let mut sim_withdraw_resp: SimulateWithdrawResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::SimulateWithdraw {
                account: "investor_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();

    assert_eq!(
        sim_withdraw_resp.astro_to_withdraw,
        unlock_resp - alloc_resp.status.astro_withdrawn
    );

    app.execute_contract(
        Addr::unchecked("investor_1".clone()),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, unlock_resp);

    // ######    ERROR :: No unlocked ASTRO to be withdrawn   ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1".clone()),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: No unlocked ASTRO to be withdrawn"
    );

    // ######   SUCCESSFULLY WITHDRAWS ASTRO #3   ######
    // ***** Check that tokens that can be withdrawn before cliff is 0 *****
    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1650178273)
    });

    // Check the number of unlocked tokens
    unlock_resp = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::UnlockedTokens {
                account: "team_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(unlock_resp, Uint128::from(1232876553779u64));

    // Check Number of tokens that can be withdrawn
    sim_withdraw_resp = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::SimulateWithdraw {
                account: "team_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();

    assert_eq!(sim_withdraw_resp.astro_to_withdraw, Uint128::zero());

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1650178279)
    });

    // Check the number of unlocked tokens
    unlock_resp = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::UnlockedTokens {
                account: "team_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(unlock_resp, Uint128::from(1232877505073u64));

    // Check Number of tokens that can be withdrawn
    sim_withdraw_resp = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::SimulateWithdraw {
                account: "team_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();

    assert_eq!(
        sim_withdraw_resp.astro_to_withdraw,
        Uint128::from(1232877505073u64)
    );

    app.execute_contract(
        Addr::unchecked("team_1".clone()),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "team_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        resp.status.astro_withdrawn,
        sim_withdraw_resp.astro_to_withdraw
    );

    // Check Number of tokens that can be withdrawn
    sim_withdraw_resp = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::SimulateWithdraw {
                account: "team_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();

    assert_eq!(sim_withdraw_resp.astro_to_withdraw, Uint128::zero());
}

#[test]
fn test_propose_new_receiver() {
    let mut app = mock_app();
    let (unlock_instance, astro_instance, _) = init_contracts(&mut app);

    mint_some_astro(
        &mut app,
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        Uint128::new(1_000_000_000_000000),
        OWNER.to_string(),
    );

    let mut allocations: Vec<(String, AllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "team_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));

    // SUCCESSFULLY CREATES ALLOCATIONS
    app.execute_contract(
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        &cw20::Cw20ExecuteMsg::Send {
            contract: unlock_instance.clone().to_string(),
            amount: Uint128::from(15_000_000_000000u64),
            msg: to_binary(&ReceiveMsg::CreateAllocations {
                allocations: allocations.clone(),
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    // ######    ERROR :: Allocation doesn't exist    ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER.clone()),
            unlock_instance.clone(),
            &ExecuteMsg::ProposeNewReceiver {
                new_receiver: "investor_1_new".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "astroport_governance::builder_unlock::AllocationParams not found"
    );

    // ######    ERROR :: Invalid new_receiver    ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1".clone()),
            unlock_instance.clone(),
            &ExecuteMsg::ProposeNewReceiver {
                new_receiver: "team_1".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Invalid new_receiver. Proposed receiver already has an ASTRO allocation of 5000000000000 ASTRO"
    );

    // ######   SUCCESSFULLY PROPOSES NEW RECEIVER   ######
    app.execute_contract(
        Addr::unchecked("investor_1".clone()),
        unlock_instance.clone(),
        &ExecuteMsg::ProposeNewReceiver {
            new_receiver: "investor_1_new".to_string(),
        },
        &[],
    )
    .unwrap();

    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        resp.params.proposed_receiver,
        Some(Addr::unchecked("investor_1_new".to_string()))
    );

    // ######    ERROR ::"Proposed receiver already set"   ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1".clone()),
            unlock_instance.clone(),
            &ExecuteMsg::ProposeNewReceiver {
                new_receiver: "investor_1_new_".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Proposed receiver already set to investor_1_new"
    );
}

#[test]
fn test_drop_new_receiver() {
    let mut app = mock_app();
    let (unlock_instance, astro_instance, _) = init_contracts(&mut app);

    mint_some_astro(
        &mut app,
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        Uint128::new(1_000_000_000_000000),
        OWNER.to_string(),
    );

    let mut allocations: Vec<(String, AllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "team_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));

    // SUCCESSFULLY CREATES ALLOCATIONS
    app.execute_contract(
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        &cw20::Cw20ExecuteMsg::Send {
            contract: unlock_instance.clone().to_string(),
            amount: Uint128::from(15_000_000_000000u64),
            msg: to_binary(&ReceiveMsg::CreateAllocations {
                allocations: allocations.clone(),
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    // ######    ERROR :: Allocation doesn't exist    ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER.clone()),
            unlock_instance.clone(),
            &ExecuteMsg::DropNewReceiver {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "astroport_governance::builder_unlock::AllocationParams not found"
    );

    // ######    ERROR ::"Proposed receiver not set"   ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1".clone()),
            unlock_instance.clone(),
            &ExecuteMsg::DropNewReceiver {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Proposed receiver not set");

    // ######   SUCCESSFULLY DROP NEW RECEIVER   ######
    // SUCCESSFULLY PROPOSES NEW RECEIVER
    app.execute_contract(
        Addr::unchecked("investor_1".clone()),
        unlock_instance.clone(),
        &ExecuteMsg::ProposeNewReceiver {
            new_receiver: "investor_1_new".to_string(),
        },
        &[],
    )
    .unwrap();

    let mut resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        resp.params.proposed_receiver,
        Some(Addr::unchecked("investor_1_new".to_string()))
    );

    app.execute_contract(
        Addr::unchecked("investor_1".clone()),
        unlock_instance.clone(),
        &ExecuteMsg::DropNewReceiver {},
        &[],
    )
    .unwrap();

    resp = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.proposed_receiver, None);
}

#[test]
fn test_claim_receiver() {
    let mut app = mock_app();
    let (unlock_instance, astro_instance, _) = init_contracts(&mut app);

    mint_some_astro(
        &mut app,
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        Uint128::new(1_000_000_000_000000),
        OWNER.to_string(),
    );

    let mut allocations: Vec<(String, AllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));
    allocations.push((
        "team_1".to_string(),
        AllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
            },
            proposed_receiver: None,
        },
    ));

    // SUCCESSFULLY CREATES ALLOCATIONS
    app.execute_contract(
        Addr::unchecked(OWNER.clone()),
        astro_instance.clone(),
        &cw20::Cw20ExecuteMsg::Send {
            contract: unlock_instance.clone().to_string(),
            amount: Uint128::from(15_000_000_000000u64),
            msg: to_binary(&ReceiveMsg::CreateAllocations {
                allocations: allocations.clone(),
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    // ######    ERROR :: Allocation doesn't exist    ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER.clone()),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "astroport_governance::builder_unlock::AllocationParams not found"
    );

    // ######    ERROR ::"Proposed receiver not set"   ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1_new".clone()),
            unlock_instance.clone(),
            &ExecuteMsg::ClaimReceiver {
                prev_receiver: "investor_1".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Proposed receiver not set");

    // ######   SUCCESSFULLY CLAIMED BY NEW RECEIVER   ######
    // SUCCESSFULLY PROPOSES NEW RECEIVER
    app.execute_contract(
        Addr::unchecked("investor_1".clone()),
        unlock_instance.clone(),
        &ExecuteMsg::ProposeNewReceiver {
            new_receiver: "investor_1_new".to_string(),
        },
        &[],
    )
    .unwrap();

    let alloc_resp_before: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();

    // Check Number of tokens that can be withdrawn
    let sim_withdraw_resp_before: SimulateWithdrawResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::SimulateWithdraw {
                account: "investor_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();

    // Claimed by new receiver
    app.execute_contract(
        Addr::unchecked("investor_1_new".clone()),
        unlock_instance.clone(),
        &ExecuteMsg::ClaimReceiver {
            prev_receiver: "investor_1".to_string(),
        },
        &[],
    )
    .unwrap();

    // Check allocation state of previous beneficiary
    let alloc_resp_after: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        AllocationParams {
            amount: Uint128::zero(),
            unlock_schedule: Schedule {
                start_time: 0u64,
                cliff: 0u64,
                duration: 0u64,
            },
            proposed_receiver: None,
        },
        alloc_resp_after.params
    );
    assert_eq!(alloc_resp_before.status, alloc_resp_after.status);

    // Check allocation state of new beneficiary
    let alloc_resp_after: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1_new".to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        AllocationParams {
            amount: alloc_resp_before.params.amount,
            unlock_schedule: Schedule {
                start_time: alloc_resp_before.params.unlock_schedule.start_time,
                cliff: alloc_resp_before.params.unlock_schedule.cliff,
                duration: alloc_resp_before.params.unlock_schedule.duration,
            },
            proposed_receiver: None,
        },
        alloc_resp_after.params
    );
    assert_eq!(alloc_resp_before.status, alloc_resp_after.status);

    // Check Number of tokens that can be withdrawn
    let sim_withdraw_resp_after_prev_inv: SimulateWithdrawResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::SimulateWithdraw {
                account: "investor_1_new".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(
        sim_withdraw_resp_after_prev_inv.astro_to_withdraw,
        Uint128::zero()
    );

    // Check Number of tokens that can be withdrawn
    let sim_withdraw_resp_after_new_inv: SimulateWithdrawResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::SimulateWithdraw {
                account: "investor_1_new".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(
        sim_withdraw_resp_after_new_inv.astro_to_withdraw,
        sim_withdraw_resp_before.astro_to_withdraw,
    );
}
