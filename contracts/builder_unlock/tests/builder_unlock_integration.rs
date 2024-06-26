use std::time::SystemTime;

use cosmwasm_std::{coin, coins, Addr, Decimal, StdResult, Timestamp, Uint128};
use cw_multi_test::{App, BasicApp, ContractWrapper, Executor};
use cw_utils::PaymentError;

use astroport_governance::builder_unlock::{
    AllocationParams, AllocationResponse, Config, ExecuteMsg, InstantiateMsg, QueryMsg,
    SimulateWithdrawResponse,
};
use astroport_governance::builder_unlock::{CreateAllocationParams, Schedule, State};
use builder_unlock::error::ContractError;

pub const ASTRO_DENOM: &str = "factory/assembly/ASTRO";

const OWNER: &str = "owner";

fn mock_app() -> App {
    let mut app = BasicApp::default();
    app.init_modules(|router, _, storage| {
        router
            .bank
            .init_balance(
                storage,
                &Addr::unchecked(OWNER),
                vec![coin(u128::MAX, ASTRO_DENOM), coin(u128::MAX, "random")],
            )
            .unwrap()
    });

    app
}

fn init_contracts(app: &mut App) -> (Addr, InstantiateMsg) {
    // Instantiate the contract
    let unlock_contract = Box::new(ContractWrapper::new(
        builder_unlock::contract::execute,
        builder_unlock::contract::instantiate,
        builder_unlock::query::query,
    ));

    let unlock_code_id = app.store_code(unlock_contract);

    let unlock_instantiate_msg = InstantiateMsg {
        owner: OWNER.to_string(),
        astro_denom: ASTRO_DENOM.to_string(),
        max_allocations_amount: Uint128::new(300_000_000_000_000u128),
    };

    // Init contract
    let unlock_instance = app
        .instantiate_contract(
            unlock_code_id,
            Addr::unchecked(OWNER),
            &unlock_instantiate_msg,
            &[],
            "unlock",
            None,
        )
        .unwrap();

    (unlock_instance, unlock_instantiate_msg)
}

fn mint_some_astro(app: &mut App, amount: Uint128, to: String) {
    app.send_tokens(
        Addr::unchecked(OWNER),
        Addr::unchecked(to),
        &coins(amount.u128(), ASTRO_DENOM),
    )
    .unwrap();
}

fn check_alloc_amount(app: &mut App, contract_addr: &Addr, account: &Addr, amount: Uint128) {
    let res: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::Allocation {
                account: account.to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(res.status.amount, amount);
}

fn check_unlock_amount(app: &mut App, contract_addr: &Addr, account: &Addr, amount: Uint128) {
    let resp: Uint128 = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::UnlockedTokens {
                account: account.to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp, amount);
}

#[test]
fn proper_initialization() {
    let mut app = mock_app();
    let (unlock_instance, init_msg) = init_contracts(&mut app);

    let resp: Config = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::Config {})
        .unwrap();

    // Check config
    assert_eq!(init_msg.owner, resp.owner);
    assert_eq!(init_msg.astro_denom, resp.astro_denom);

    // Check state
    let resp: State = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::State { timestamp: None })
        .unwrap();

    assert_eq!(Uint128::zero(), resp.total_astro_deposited);
    assert_eq!(Uint128::zero(), resp.remaining_astro_tokens);
}

#[test]
fn test_transfer_ownership() {
    let mut app = mock_app();
    let (unlock_instance, init_msg) = init_contracts(&mut app);

    // ######    ERROR :: Unauthorized     ######
    let err = app
        .execute_contract(
            Addr::unchecked("not_owner".to_string()),
            unlock_instance.clone(),
            &ExecuteMsg::ProposeNewOwner {
                new_owner: "new_owner".to_string(),
                expires_in: 600,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    app.execute_contract(
        Addr::unchecked(OWNER.to_string()),
        unlock_instance.clone(),
        &ExecuteMsg::ProposeNewOwner {
            new_owner: "new_owner".to_string(),
            expires_in: 100,
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked("new_owner".to_string()),
        unlock_instance.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    let resp: Config = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::Config {})
        .unwrap();

    // Check config
    assert_eq!("new_owner".to_string(), resp.owner);
    assert_eq!(init_msg.astro_denom, resp.astro_denom);
}

#[test]
fn test_create_allocations() {
    let mut app = mock_app();
    let (unlock_instance, _) = init_contracts(&mut app);

    let mut allocations: Vec<(String, CreateAllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "team_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));

    // ######    ERROR :: Only owner can create allocations     ######
    mint_some_astro(&mut app, Uint128::new(1_000), "not_owner".to_string());

    let err = app
        .execute_contract(
            Addr::unchecked("not_owner".to_string()),
            unlock_instance.clone(),
            &ExecuteMsg::CreateAllocations {
                allocations: allocations.clone(),
            },
            &coins(1_000, ASTRO_DENOM),
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Only the contract owner can create allocations"
    );

    // ######    ERROR :: Only ASTRO can be can be deposited     ######

    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::CreateAllocations {
                allocations: allocations.clone(),
            },
            &coins(15_000_000_000000, "random"),
        )
        .unwrap_err();

    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PaymentError(PaymentError::MissingDenom(ASTRO_DENOM.to_string()))
    );

    // ######    ERROR :: ASTRO deposit amount mismatch     ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::CreateAllocations {
                allocations: allocations.clone(),
            },
            &coins(15_000_000_000001, ASTRO_DENOM),
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::DepositAmountMismatch {
            expected: 15000000000000u128.into(),
            got: 15000000000001u128.into()
        }
    );

    // ######    SUCCESSFULLY CREATES ALLOCATIONS    ######
    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::CreateAllocations {
            allocations: allocations.clone(),
        },
        &coins(15_000_000_000000, ASTRO_DENOM),
    )
    .unwrap();

    // Check state
    let resp: State = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::State { timestamp: None })
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
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(resp.status.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(
        resp.params.unlock_schedule,
        Schedule {
            start_time: 1642402274u64,
            cliff: 0u64,
            duration: 31536000u64,
            percent_at_cliff: None,
        }
    );

    // Check allocation #2
    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "advisor_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(resp.status.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(
        resp.params.unlock_schedule,
        Schedule {
            start_time: 1642402274u64,
            cliff: 7776000u64,
            duration: 31536000u64,
            percent_at_cliff: None,
        }
    );

    // Check allocation #3
    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "team_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(resp.status.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(
        resp.params.unlock_schedule,
        Schedule {
            start_time: 1642402274u64,
            cliff: 7776000u64,
            duration: 31536000u64,
            percent_at_cliff: None,
        }
    );

    // ######    ERROR :: Allocation already exists for user {}     ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::CreateAllocations {
                allocations: vec![allocations[0].clone()],
            },
            &coins(5_000_000_000000, ASTRO_DENOM),
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::AllocationExists {
            user: "investor_1".to_string()
        }
    );
}

#[test]
fn test_withdraw() {
    let mut app = mock_app();
    let (unlock_instance, _) = init_contracts(&mut app);

    let mut allocations: Vec<(String, CreateAllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "team_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1642402274)
    });

    // SUCCESSFULLY CREATES ALLOCATIONS
    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::CreateAllocations {
            allocations: allocations.clone(),
        },
        &coins(15_000_000_000000, ASTRO_DENOM),
    )
    .unwrap();

    // ######    ERROR :: Allocation doesn't exist    ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoAllocation {
            address: OWNER.to_string()
        }
    );

    app.next_block(1);

    // ######   SUCCESSFULLY WITHDRAWS ASTRO #1   ######
    let astro_bal_before = app.wrap().query_balance("investor_1", ASTRO_DENOM).unwrap();

    app.execute_contract(
        Addr::unchecked("investor_1"),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    // ######    ERROR :: No unlocked ASTRO to be withdrawn   ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1"),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoUnlockedAstro {}
    );

    // Check state
    let state_resp: State = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::State { timestamp: None })
        .unwrap();
    assert_eq!(
        state_resp.total_astro_deposited,
        Uint128::from(15_000_000_000000u64)
    );
    assert_eq!(
        state_resp.remaining_astro_tokens,
        Uint128::from(14_999_999_841452u64)
    );

    app.next_block(1);

    // Check allocation #1
    let alloc_resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(alloc_resp.status.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(alloc_resp.status.astro_withdrawn, Uint128::from(158548u64));

    let astro_bal_after = app.wrap().query_balance("investor_1", ASTRO_DENOM).unwrap();

    assert_eq!(
        astro_bal_after.amount - astro_bal_before.amount,
        alloc_resp.status.astro_withdrawn
    );

    // Check the number of unlocked tokens
    let unlock_resp: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::UnlockedTokens {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(unlock_resp.u128(), 317097);

    // ######   SUCCESSFULLY WITHDRAWS ASTRO #2   ######
    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1642402285)
    });

    // Check the number of unlocked tokens
    let unlock_resp: Uint128 = app
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
        Addr::unchecked("investor_1"),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    let unlock_resp: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::UnlockedTokens {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();

    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(resp.status.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, unlock_resp);

    // ######    ERROR :: No unlocked ASTRO to be withdrawn   ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1"),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoUnlockedAstro {}
    );

    // ######   SUCCESSFULLY WITHDRAWS ASTRO #3   ######
    // ***** Check that tokens that can be withdrawn before cliff is 0 *****
    app.update_block(|b| {
        b.height += 1572480;
        b.time = Timestamp::from_seconds(1657954273)
    });

    // Check the number of unlocked tokens
    let unlock_resp: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::UnlockedTokens {
                account: "team_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(unlock_resp, Uint128::from(2465753266108u64));

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
        Uint128::from(2465753266108u64)
    );

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1657954279)
    });

    // Check the number of unlocked tokens
    let unlock_resp: Uint128 = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::UnlockedTokens {
                account: "team_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(unlock_resp, Uint128::from(2465754217402u64));

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
        Uint128::from(2465754217402u64)
    );

    app.execute_contract(
        Addr::unchecked("team_1"),
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
                timestamp: None,
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
    let (unlock_instance, _) = init_contracts(&mut app);

    let mut allocations: Vec<(String, CreateAllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "team_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));

    // SUCCESSFULLY CREATES ALLOCATIONS
    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::CreateAllocations {
            allocations: allocations.clone(),
        },
        &coins(15_000_000_000000, ASTRO_DENOM),
    )
    .unwrap();

    // ######    ERROR :: Allocation doesn't exist    ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::ProposeNewReceiver {
                new_receiver: "investor_1_new".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoAllocation {
            address: OWNER.to_string()
        }
    );

    // ######    ERROR :: Invalid new_receiver    ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1"),
            unlock_instance.clone(),
            &ExecuteMsg::ProposeNewReceiver {
                new_receiver: "team_1".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposedReceiverAlreadyHasAllocation {}
    );

    // ######   SUCCESSFULLY PROPOSES NEW RECEIVER   ######
    app.execute_contract(
        Addr::unchecked("investor_1"),
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
                timestamp: None,
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
            Addr::unchecked("investor_1"),
            unlock_instance.clone(),
            &ExecuteMsg::ProposeNewReceiver {
                new_receiver: "investor_1_new_".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposedReceiverAlreadySet {
            proposed_receiver: Addr::unchecked("investor_1_new")
        }
    );
}

#[test]
fn test_drop_new_receiver() {
    let mut app = mock_app();
    let (unlock_instance, _) = init_contracts(&mut app);

    let mut allocations: Vec<(String, CreateAllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "team_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));

    // SUCCESSFULLY CREATES ALLOCATIONS
    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::CreateAllocations {
            allocations: allocations.clone(),
        },
        &coins(15_000_000_000000, ASTRO_DENOM),
    )
    .unwrap();

    // ######    ERROR :: Allocation doesn't exist    ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::DropNewReceiver {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoAllocation {
            address: OWNER.to_string()
        }
    );

    // ######    ERROR ::"Proposed receiver not set"   ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1"),
            unlock_instance.clone(),
            &ExecuteMsg::DropNewReceiver {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposedReceiverNotSet {}
    );

    // ######   SUCCESSFULLY DROP NEW RECEIVER   ######
    // SUCCESSFULLY PROPOSES NEW RECEIVER
    app.execute_contract(
        Addr::unchecked("investor_1"),
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
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(
        resp.params.proposed_receiver,
        Some(Addr::unchecked("investor_1_new".to_string()))
    );

    app.execute_contract(
        Addr::unchecked("investor_1"),
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
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(resp.params.proposed_receiver, None);
}

#[test]
fn test_claim_receiver() {
    let mut app = mock_app();
    let (unlock_instance, _) = init_contracts(&mut app);

    let mut allocations: Vec<(String, CreateAllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "team_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));

    // SUCCESSFULLY CREATES ALLOCATIONS
    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::CreateAllocations {
            allocations: allocations.clone(),
        },
        &coins(15_000_000_000000, ASTRO_DENOM),
    )
    .unwrap();

    // ######    ERROR :: Allocation doesn't exist    ######
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoAllocation {
            address: OWNER.to_string()
        }
    );

    // ######    ERROR ::"Proposed receiver not set"   ######
    let err = app
        .execute_contract(
            Addr::unchecked("investor_1_new"),
            unlock_instance.clone(),
            &ExecuteMsg::ClaimReceiver {
                prev_receiver: "investor_1".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ProposedReceiverMismatch {}
    );

    // ######   SUCCESSFULLY CLAIMED BY NEW RECEIVER   ######
    // SUCCESSFULLY PROPOSES NEW RECEIVER
    app.execute_contract(
        Addr::unchecked("investor_1"),
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
                timestamp: None,
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
        Addr::unchecked("investor_1_new"),
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
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(
        AllocationParams {
            unlock_schedule: Schedule {
                start_time: 0u64,
                cliff: 0u64,
                duration: 0u64,
                percent_at_cliff: None,
            },
            proposed_receiver: None,
        },
        alloc_resp_after.params
    );

    // Check allocation state of new beneficiary
    let alloc_resp_after: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocation {
                account: "investor_1_new".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(
        AllocationParams {
            unlock_schedule: Schedule {
                start_time: alloc_resp_before.params.unlock_schedule.start_time,
                cliff: alloc_resp_before.params.unlock_schedule.cliff,
                duration: alloc_resp_before.params.unlock_schedule.duration,
                percent_at_cliff: None,
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

#[test]
fn test_increase_and_decrease_allocation() {
    let mut app = mock_app();
    let (unlock_instance, _) = init_contracts(&mut app);

    // Create allocations
    let allocations: Vec<(String, CreateAllocationParams)> = vec![(
        "investor".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1_571_797_419u64,
                cliff: 300u64,
                duration: 1_534_700u64,
                percent_at_cliff: None,
            },
        },
    )];

    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::CreateAllocations {
            allocations: allocations.clone(),
        },
        &coins(5_000_000_000000, ASTRO_DENOM),
    )
    .unwrap();

    // Check allocations before changes
    check_alloc_amount(
        &mut app,
        &unlock_instance,
        &Addr::unchecked("investor"),
        Uint128::new(5_000_000_000_000u128),
    );

    // Skip blocks
    app.update_block(|bi| {
        bi.height += 1000;
        bi.time = bi.time.plus_seconds(5_000);
    });

    // Withdraw ASTRO
    app.execute_contract(
        Addr::unchecked("investor".to_string()),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    // Skip blocks
    app.update_block(|bi| {
        bi.height += 4000;
        bi.time = bi.time.plus_seconds(20_000);
    });

    check_unlock_amount(
        &mut app,
        &unlock_instance,
        &Addr::unchecked("investor"),
        Uint128::new(81_449_143_155u128),
    );

    // Try to decrease 4918550856846 ASTRO
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::DecreaseAllocation {
                receiver: "investor".to_string(),
                amount: Uint128::from(4_918_550_856_846u128),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InsufficientLockedAmount {
            locked_amount: 4918550856845u128.into()
        }
    );

    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::DecreaseAllocation {
            receiver: "investor".to_string(),
            amount: Uint128::from(1_000_000_000_000u128),
        },
        &[],
    )
    .unwrap();

    // Unlock amount didn't change after decreasing
    check_unlock_amount(
        &mut app,
        &unlock_instance,
        &Addr::unchecked("investor"),
        Uint128::new(81_449_143_155u128),
    );
    let res: State = app
        .wrap()
        .query_wasm_smart(
            unlock_instance.clone(),
            &QueryMsg::State { timestamp: None },
        )
        .unwrap();

    assert_eq!(
        res,
        State {
            total_astro_deposited: Uint128::new(5_000_000_000_000u128),
            remaining_astro_tokens: Uint128::new(3_983_710_171_369u128),
            unallocated_astro_tokens: Uint128::new(1_000_000_000_000u128),
        }
    );

    // Try to increase
    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::IncreaseAllocation {
                receiver: "investor".to_string(),
                amount: Uint128::from(1_000_000_000_001u128),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::UnallocatedTokensExceedsTotalDeposited(1_000_000_000_000u128.into())
    );

    let balance_before = app.wrap().query_balance(OWNER, ASTRO_DENOM).unwrap().amount;

    // Transfer unallocated tokens to owner
    app.execute_contract(
        Addr::unchecked("owner".to_string()),
        unlock_instance.clone(),
        &ExecuteMsg::TransferUnallocated {
            amount: Uint128::from(500_000_000_000u128),
            recipient: Some(OWNER.to_string()),
        },
        &[],
    )
    .unwrap();

    let balance_after = app.wrap().query_balance(OWNER, ASTRO_DENOM).unwrap().amount;
    assert_eq!((balance_after - balance_before).u128(), 500_000_000_000u128);

    // Increase allocations
    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::IncreaseAllocation {
            amount: Uint128::from(500_000_001_000u128),
            receiver: "investor".to_string(),
        },
        &coins(1_000, ASTRO_DENOM),
    )
    .unwrap();

    // Withdraw ASTRO
    app.execute_contract(
        Addr::unchecked("investor".to_string()),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    let balance = app.wrap().query_balance("investor", ASTRO_DENOM).unwrap();
    assert_eq!(balance.amount, Uint128::from(81_449_143_155u128));

    // Check allocation amount after decreasing and increasing
    check_alloc_amount(
        &mut app,
        &unlock_instance,
        &Addr::unchecked("investor"),
        Uint128::new(4_500_000_001_000u128),
    );
    // Check astro to withdraw after withdrawal
    let res: SimulateWithdrawResponse = app
        .wrap()
        .query_wasm_smart(
            unlock_instance.clone(),
            &QueryMsg::SimulateWithdraw {
                account: "investor".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(res.astro_to_withdraw, Uint128::zero());
    // Check state
    let res: State = app
        .wrap()
        .query_wasm_smart(
            unlock_instance.clone(),
            &QueryMsg::State { timestamp: None },
        )
        .unwrap();
    assert_eq!(
        res,
        State {
            total_astro_deposited: Uint128::new(4_500_000_001_000u128),
            remaining_astro_tokens: Uint128::new(4_418_550_857_845u128),
            unallocated_astro_tokens: Uint128::zero(),
        }
    );
}

#[test]
fn test_updates_schedules() {
    let mut app = mock_app();
    let (unlock_instance, _) = init_contracts(&mut app);

    let mut allocations: Vec<(String, CreateAllocationParams)> = vec![];
    allocations.push((
        "investor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 0u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "advisor_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        "team_1".to_string(),
        CreateAllocationParams {
            amount: Uint128::from(5_000_000_000000u64),
            unlock_schedule: Schedule {
                start_time: 1642402274u64,
                cliff: 7776000u64,
                duration: 31536000u64,
                percent_at_cliff: None,
            },
        },
    ));

    // ######    SUCCESSFULLY CREATES ALLOCATIONS    ######
    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::CreateAllocations {
            allocations: allocations.clone(),
        },
        &coins(15_000_000_000000, ASTRO_DENOM),
    )
    .unwrap();

    // Check state before update parameters
    let resp: State = app
        .wrap()
        .query_wasm_smart(&unlock_instance, &QueryMsg::State { timestamp: None })
        .unwrap();
    assert_eq!(
        resp.total_astro_deposited,
        Uint128::from(15_000_000_000000u64)
    );
    assert_eq!(
        resp.remaining_astro_tokens,
        Uint128::from(15_000_000_000000u64)
    );

    // Check allocation #1 before update
    check_allocation(
        &mut app,
        &unlock_instance,
        "investor_1".to_string(),
        Uint128::from(5_000_000_000000u64),
        Uint128::from(0u64),
        Schedule {
            start_time: 1642402274u64,
            cliff: 0u64,
            duration: 31536000u64,
            percent_at_cliff: None,
        },
    )
    .unwrap();

    // Check allocation #2 before update
    check_allocation(
        &mut app,
        &unlock_instance,
        "advisor_1".to_string(),
        Uint128::from(5_000_000_000000u64),
        Uint128::from(0u64),
        Schedule {
            start_time: 1642402274u64,
            cliff: 7776000u64,
            duration: 31536000u64,
            percent_at_cliff: None,
        },
    )
    .unwrap();

    // Check allocation #3 before update
    check_allocation(
        &mut app,
        &unlock_instance,
        "team_1".to_string(),
        Uint128::from(5_000_000_000000u64),
        Uint128::from(0u64),
        Schedule {
            start_time: 1642402274u64,
            cliff: 7776000u64,
            duration: 31536000u64,
            percent_at_cliff: None,
        },
    )
    .unwrap();

    // not owner try to update configs
    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            unlock_instance.clone(),
            &ExecuteMsg::UpdateUnlockSchedules {
                new_unlock_schedules: vec![(
                    "team_1".to_string(),
                    Schedule {
                        start_time: 123u64,
                        cliff: 123u64,
                        duration: 123u64,
                        percent_at_cliff: None,
                    },
                )],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    let err = app
        .execute_contract(
            Addr::unchecked(OWNER),
            unlock_instance.clone(),
            &ExecuteMsg::UpdateUnlockSchedules {
                new_unlock_schedules: vec![
                    (
                        "team_1".to_string(),
                        Schedule {
                            start_time: 123u64,
                            cliff: 123u64,
                            duration: 123u64,
                            percent_at_cliff: None,
                        },
                    ),
                    (
                        "advisor_1".to_string(),
                        Schedule {
                            start_time: 123u64,
                            cliff: 123u64,
                            duration: 123u64,
                            percent_at_cliff: None,
                        },
                    ),
                ],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "Generic error: The new cliff value should be greater than or equal to the old one: 123 >= 7776000. Account error: team_1",
        err.root_cause().to_string()
    );

    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::UpdateUnlockSchedules {
            new_unlock_schedules: vec![
                (
                    "team_1".to_string(),
                    Schedule {
                        start_time: 1642402284u64,
                        cliff: 8776000u64,
                        duration: 31536001u64,
                        percent_at_cliff: None,
                    },
                ),
                (
                    "advisor_1".to_string(),
                    Schedule {
                        start_time: 1642402284u64,
                        cliff: 8776000u64,
                        duration: 31536001u64,
                        percent_at_cliff: None,
                    },
                ),
            ],
        },
        &[],
    )
    .unwrap();

    // Check allocation #2 before update
    check_allocation(
        &mut app,
        &unlock_instance,
        "advisor_1".to_string(),
        Uint128::from(5_000_000_000000u64),
        Uint128::from(0u64),
        Schedule {
            start_time: 1642402284u64,
            cliff: 8776000u64,
            duration: 31536001u64,
            percent_at_cliff: None,
        },
    )
    .unwrap();

    // Check allocation #3 before update
    check_allocation(
        &mut app,
        &unlock_instance,
        "team_1".to_string(),
        Uint128::from(5_000_000_000000u64),
        Uint128::from(0u64),
        Schedule {
            start_time: 1642402284u64,
            cliff: 8776000u64,
            duration: 31536001u64,
            percent_at_cliff: None,
        },
    )
    .unwrap();

    // Query allocations
    let resp: Vec<(Addr, AllocationParams)> = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocations {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    let comparing_values: Vec<(Addr, AllocationParams)> = vec![
        (
            Addr::unchecked("advisor_1"),
            AllocationParams {
                unlock_schedule: Schedule {
                    start_time: 1642402284u64,
                    cliff: 8776000u64,
                    duration: 31536001u64,
                    percent_at_cliff: None,
                },
                proposed_receiver: None,
            },
        ),
        (
            Addr::unchecked("investor_1"),
            AllocationParams {
                unlock_schedule: Schedule {
                    start_time: 1642402274,
                    cliff: 0,
                    duration: 31536000,
                    percent_at_cliff: None,
                },
                proposed_receiver: None,
            },
        ),
        (
            Addr::unchecked("team_1"),
            AllocationParams {
                unlock_schedule: Schedule {
                    start_time: 1642402284u64,
                    cliff: 8776000u64,
                    duration: 31536001u64,
                    percent_at_cliff: None,
                },
                proposed_receiver: None,
            },
        ),
    ];
    assert_eq!(comparing_values, resp);

    // Query allocations by specified parameters
    let resp: Vec<(Addr, AllocationParams)> = app
        .wrap()
        .query_wasm_smart(
            &unlock_instance,
            &QueryMsg::Allocations {
                start_after: Some("investor_1".to_string()),
                limit: None,
            },
        )
        .unwrap();
    let comparing_values: Vec<(Addr, AllocationParams)> = vec![(
        Addr::unchecked("team_1"),
        AllocationParams {
            unlock_schedule: Schedule {
                start_time: 1642402284u64,
                cliff: 8776000u64,
                duration: 31536001u64,
                percent_at_cliff: None,
            },
            proposed_receiver: None,
        },
    )];
    assert_eq!(comparing_values, resp);
}

fn check_allocation(
    app: &mut App,
    unlock_instance: &Addr,
    account: String,
    total_amount: Uint128,
    astro_withdrawn: Uint128,
    unlock_schedule: Schedule,
) -> StdResult<()> {
    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            unlock_instance,
            &QueryMsg::Allocation {
                account,
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(resp.status.amount, total_amount);
    assert_eq!(resp.status.astro_withdrawn, astro_withdrawn);
    assert_eq!(resp.params.unlock_schedule, unlock_schedule);

    Ok(())
}

fn query_bal(app: &mut App, address: &Addr) -> u128 {
    app.wrap()
        .query_balance(address, ASTRO_DENOM)
        .unwrap()
        .amount
        .u128()
}

#[test]
fn test_create_allocations_with_custom_cliff() {
    let mut app = mock_app();
    let (unlock_instance, _) = init_contracts(&mut app);
    let total_astro = Uint128::new(1_000_000_000000);

    let now_ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    app.update_block(|block| block.time = Timestamp::from_seconds(now_ts));
    let day = 86400u64;

    let investor1 = Addr::unchecked("investor1");
    let investor2 = Addr::unchecked("investor2");
    let investor3 = Addr::unchecked("investor3");
    let mut allocations = vec![];
    allocations.push((
        investor1.to_string(),
        CreateAllocationParams {
            amount: Uint128::from(500_000_000000u64),
            unlock_schedule: Schedule {
                start_time: now_ts,
                cliff: day * 365,        // 1 year
                duration: 3 * day * 365, // 3 years
                percent_at_cliff: None,
            },
        },
    ));
    allocations.push((
        investor2.to_string(),
        CreateAllocationParams {
            amount: Uint128::from(100_000_000000u64),
            unlock_schedule: Schedule {
                start_time: now_ts - day * 30,                         // 1 month ago
                cliff: 6 * day * 30,                                   // 6 months
                duration: 3 * day * 365,                               // 3 years
                percent_at_cliff: Some(Decimal::from_ratio(1u8, 6u8)), // one sixth
            },
        },
    ));
    allocations.push((
        investor3.to_string(),
        CreateAllocationParams {
            amount: Uint128::from(400_000_000000u64),
            unlock_schedule: Schedule {
                start_time: now_ts - day * 365,               // 1 year ago
                cliff: 6 * day * 30,                          // 6 months
                duration: 3 * day * 365,                      // 3 years
                percent_at_cliff: Some(Decimal::percent(20)), // 20% at cliff
            },
        },
    ));

    // Create allocations
    app.execute_contract(
        Addr::unchecked(OWNER),
        unlock_instance.clone(),
        &ExecuteMsg::CreateAllocations {
            allocations: allocations.clone(),
        },
        &coins(total_astro.u128(), ASTRO_DENOM),
    )
    .unwrap();

    // Investor1's allocation just has been created
    let err = app
        .execute_contract(
            investor1.clone(),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoUnlockedAstro {}
    );

    // Investor2 needs to wait 5 months more
    let err = app
        .execute_contract(
            investor2.clone(),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoUnlockedAstro {}
    );

    // Investor3 has 20% of his allocation unlocked + linearly unlocked astro for the last 6 months
    app.execute_contract(
        investor3.clone(),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    let balance = query_bal(&mut app, &investor3);
    let amount_at_cliff = allocations[2].1.amount.u128() / 5;
    let amount_linearly_vested = 64699_453551;
    assert_eq!(balance, amount_at_cliff + amount_linearly_vested);

    // shift by 5 months
    app.update_block(|block| block.time = block.time.plus_seconds(5 * 30 * day));

    // Investor1 is still waiting
    let err = app
        .execute_contract(
            investor1.clone(),
            unlock_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoUnlockedAstro {}
    );

    // Investor2 receives his one sixth of the allocation
    app.execute_contract(
        investor2.clone(),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    let balance = query_bal(&mut app, &investor2);
    assert_eq!(balance, 16666_666666);

    // Investor3 continues to receive linearly unlocked astro
    app.execute_contract(
        investor3.clone(),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    let balance = query_bal(&mut app, &investor3);
    assert_eq!(balance, 197158_469945);

    // shift by 7 months
    app.update_block(|block| block.time = block.time.plus_seconds(215 * day));

    // Investor1 receives his allocation (linearly unlocked from start point)
    app.execute_contract(
        investor1.clone(),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    let balance = query_bal(&mut app, &investor1);
    assert_eq!(balance, 166666_666666);

    // Investor2 continues to receive linearly unlocked astro
    app.execute_contract(
        investor2.clone(),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    let balance = query_bal(&mut app, &investor2);
    assert_eq!(balance, 36247_723132);

    // Investor3 continues to receive linearly unlocked astro
    app.execute_contract(
        investor3.clone(),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    let balance = query_bal(&mut app, &investor3);
    assert_eq!(balance, 272349_726775);

    // shift by 2 years
    app.update_block(|block| block.time = block.time.plus_seconds(2 * 365 * day));

    // Investor1 receives whole allocation
    app.execute_contract(
        investor1.clone(),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    let balance = query_bal(&mut app, &investor1);
    assert_eq!(balance, 500000_000000);

    // Investor2 receives whole allocation
    app.execute_contract(
        investor2.clone(),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    let balance = query_bal(&mut app, &investor2);
    assert_eq!(balance, 100000_000000);

    // Investor3 receives whole allocation
    app.execute_contract(
        investor3.clone(),
        unlock_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();
    let balance = query_bal(&mut app, &investor3);
    assert_eq!(balance, 400000_000000);

    app.update_block(|block| block.time = block.time.plus_seconds(day));

    // No more ASTRO left for withdrawals
    for investor in &[investor1, investor2, investor3] {
        let err = app
            .execute_contract(
                investor.clone(),
                unlock_instance.clone(),
                &ExecuteMsg::Withdraw {},
                &[],
            )
            .unwrap_err();
        assert_eq!(
            err.downcast::<ContractError>().unwrap(),
            ContractError::NoUnlockedAstro {}
        );
    }
}

pub trait AppExtension {
    fn next_block(&mut self, time: u64);
}

impl AppExtension for App {
    fn next_block(&mut self, time: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(time);
            block.height += 1
        });
    }
}
