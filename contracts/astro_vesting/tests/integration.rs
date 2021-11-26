use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use astroport_governance::astro_vesting::{AllocationParams, AllocationStatus, Config, Schedule};

use astroport_governance::astro_vesting::msg::{
    AllocationResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg,
    SimulateWithdrawResponse, StateResponse,
};
use cosmwasm_std::testing::{mock_env, MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, Timestamp, Uint128};
use terra_multi_test::{App, BankKeeper, ContractWrapper, Executor, TerraMockQuerier};

const OWNER: &str = "OWNER";

fn mock_app() -> App {
    let api = MockApi::default();
    let env = mock_env();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let tmq = TerraMockQuerier::new(MockQuerier::new(&[]));

    App::new(api, env.block, bank, storage, tmq)
}

fn init_contracts(app: &mut App) -> (Addr, Addr, InstantiateMsg) {
    // Instantiate ASTRO Token Contract
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

    // Instantiate Vesting Contract
    let vesting_contract = Box::new(ContractWrapper::new(
        astro_vesting::contract::execute,
        astro_vesting::contract::instantiate,
        astro_vesting::contract::query,
    ));

    let vesting_code_id = app.store_code(vesting_contract);

    let vesting_instantiate_msg = InstantiateMsg {
        owner: OWNER.clone().to_string(),
        refund_recepient: "refund_recepient".to_string(),
        astro_token: astro_token_instance.to_string(),
    };

    // Init contract
    let vesting_instance = app
        .instantiate_contract(
            vesting_code_id,
            Addr::unchecked(OWNER.clone()),
            &vesting_instantiate_msg,
            &[],
            "vesting",
            None,
        )
        .unwrap();

    (
        vesting_instance,
        astro_token_instance,
        vesting_instantiate_msg,
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
    let (vesting_instance, astro_instance, init_msg) = init_contracts(&mut app);

    let resp: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&vesting_instance, &QueryMsg::Config {})
        .unwrap();

    // Check config
    assert_eq!(init_msg.owner, resp.owner);
    assert_eq!(init_msg.refund_recepient, resp.refund_recepient);
    assert_eq!(init_msg.astro_token, resp.astro_token);

    // Check state
    let resp: StateResponse = app
        .wrap()
        .query_wasm_smart(&vesting_instance, &QueryMsg::State {})
        .unwrap();

    assert_eq!(Uint128::zero(), resp.total_astro_deposited);
    assert_eq!(Uint128::zero(), resp.remaining_astro_tokens);
}

#[test]
fn test_transfer_ownership() {
    let mut app = mock_app();
    let (vesting_instance, _, init_msg) = init_contracts(&mut app);

    // ######    ERROR :: Unauthorized     ######

    let err = app
        .execute_contract(
            Addr::unchecked("not_owner".to_string()),
            vesting_instance.clone(),
            &ExecuteMsg::TransferOwnership {
                new_owner: Some("new_owner".to_string()),
                new_refund_recepient: Some("new_refund_recepient".to_string()),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Only owner can transfer ownership"
    );

    // ######    SUCCESSFULLY TRANSFERS OWNERSHIP :: UPDATES OWNER    ######

    app.execute_contract(
        Addr::unchecked(OWNER.to_string()),
        vesting_instance.clone(),
        &ExecuteMsg::TransferOwnership {
            new_owner: Some("new_owner".to_string()),
            new_refund_recepient: None,
        },
        &[],
    )
    .unwrap();

    let resp: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&vesting_instance, &QueryMsg::Config {})
        .unwrap();

    // Check config
    assert_eq!("new_owner".to_string(), resp.owner);
    assert_eq!(init_msg.refund_recepient, resp.refund_recepient);
    assert_eq!(init_msg.astro_token, resp.astro_token);

    // ######    SUCCESSFULLY TRANSFERS OWNERSHIP :: UPDATES REFUND RECEPIENT    ######

    app.execute_contract(
        Addr::unchecked("new_owner".to_string()),
        vesting_instance.clone(),
        &ExecuteMsg::TransferOwnership {
            new_owner: None,
            new_refund_recepient: Some("new_refund_recepient".to_string()),
        },
        &[],
    )
    .unwrap();

    let resp: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&vesting_instance, &QueryMsg::Config {})
        .unwrap();

    // Check config
    assert_eq!("new_owner".to_string(), resp.owner);
    assert_eq!("new_refund_recepient".to_string(), resp.refund_recepient);
    assert_eq!(init_msg.astro_token, resp.astro_token);
}

#[test]
fn test_create_allocations() {
    let mut app = mock_app();
    let (vesting_instance, astro_instance, _) = init_contracts(&mut app);

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
            vest_schedule: Schedule {
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
            vest_schedule: Schedule {
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
            vest_schedule: Schedule {
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
                contract: vesting_instance.clone().to_string(),
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
        "Generic error: Only owner can create allocations"
    );

    // ######    ERROR :: Only ASTRO Token can be  can be deposited     ######

    // Instantiate ASTRO Token Contract
    let not_astro_token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let not_astro_token_code_id = app.store_code(not_astro_token_contract);

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
                contract: vesting_instance.clone().to_string(),
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
        "Generic error: Only ASTRO token can be deposited"
    );

    // ######    ERROR :: ASTRO deposit amount mismatch     ######

    err = app
        .execute_contract(
            Addr::unchecked(OWNER.clone()),
            astro_instance.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: vesting_instance.clone().to_string(),
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
            contract: vesting_instance.clone().to_string(),
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
        .query_wasm_smart(&vesting_instance, &QueryMsg::State {})
        .unwrap();
    assert_eq!(
        resp.total_astro_deposited,
        Uint128::from(15_000_000_000000u64)
    );
    assert_eq!(
        resp.remaining_astro_tokens,
        Uint128::from(15_000_000_000000u64)
    );

    let resp: StateResponse = app
        .wrap()
        .query_wasm_smart(&vesting_instance, &QueryMsg::State {})
        .unwrap();

    // Check allocation #1
    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &vesting_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(
        resp.params.vest_schedule,
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
            &vesting_instance,
            &QueryMsg::Allocation {
                account: "advisor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(
        resp.params.vest_schedule,
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
            &vesting_instance,
            &QueryMsg::Allocation {
                account: "team_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(
        resp.params.vest_schedule,
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
                contract: vesting_instance.clone().to_string(),
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
    let (vesting_instance, astro_instance, _) = init_contracts(&mut app);

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
            vest_schedule: Schedule {
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
            vest_schedule: Schedule {
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
            vest_schedule: Schedule {
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
            contract: vesting_instance.clone().to_string(),
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
            vesting_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "white_astro::vesting::AllocationParams not found"
    );

    // ######   SUCCESSFULLY WITHDRAWS ASTRO #1   ######

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1642402275)
    });

    app.execute_contract(
        Addr::unchecked("investor_1".clone()),
        vesting_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    // Check allocation #1
    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &vesting_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(158548u64));

    // ######    ERROR :: No vested ASTRO to be withdrawn   ######

    let err = app
        .execute_contract(
            Addr::unchecked("investor_1".clone()),
            vesting_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: No vested ASTRO to be withdrawn"
    );

    // ######   SUCCESSFULLY WITHDRAWS ASTRO #2   ######

    let resp: SimulateWithdrawResponse = app
        .wrap()
        .query_wasm_smart(
            &vesting_instance,
            &QueryMsg::SimulateWithdraw {
                account: "investor_1".to_string(),
                timestamp: Some(1642402285u64),
            },
        )
        .unwrap();
    assert_eq!(resp.total_astro_locked, Uint128::from(5_000_000_000000u64));
    assert_eq!(
        resp.total_astro_unlocked,
        Uint128::from(5_000_000_000000u64)
    );
    assert_eq!(resp.total_astro_vested, Uint128::from(1744038u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(158548u64));
    assert_eq!(resp.withdrawable_amount, Uint128::from(1585490u64));

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1642402285)
    });

    app.execute_contract(
        Addr::unchecked("investor_1".clone()),
        vesting_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &vesting_instance,
            &QueryMsg::Allocation {
                account: "investor_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(1744038u64));

    // ######    ERROR :: No unlocked ASTRO to be withdrawn   ######

    let err = app
        .execute_contract(
            Addr::unchecked("investor_1".clone()),
            vesting_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: No unlocked ASTRO to be withdrawn"
    );

    // ######   SUCCESSFULLY WITHDRAWS ASTRO #3   ######

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1650170001)
    });

    let resp: SimulateWithdrawResponse = app
        .wrap()
        .query_wasm_smart(
            &vesting_instance,
            &QueryMsg::SimulateWithdraw {
                account: "team_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(resp.total_astro_locked, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.total_astro_unlocked, Uint128::from(1231925577118u64));
    assert_eq!(resp.total_astro_vested, Uint128::from(1231565036783u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(0u64));
    assert_eq!(resp.withdrawable_amount, Uint128::from(0u64));

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(1650178275)
    });

    let resp: SimulateWithdrawResponse = app
        .wrap()
        .query_wasm_smart(
            &vesting_instance,
            &QueryMsg::SimulateWithdraw {
                account: "team_1".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
    assert_eq!(resp.total_astro_locked, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.total_astro_vested, Uint128::from(1232876870877u64));
    assert_eq!(resp.withdrawable_amount, Uint128::from(1232876870877u64));

    app.execute_contract(
        Addr::unchecked("team_1".clone()),
        vesting_instance.clone(),
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    let resp: AllocationResponse = app
        .wrap()
        .query_wasm_smart(
            &vesting_instance,
            &QueryMsg::Allocation {
                account: "team_1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.params.amount, Uint128::from(5_000_000_000000u64));
    assert_eq!(resp.status.astro_withdrawn, Uint128::from(1232876870877u64));
}
