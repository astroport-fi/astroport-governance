use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, Addr};

use astroport_governance::escrow_fee_distributor::{ConfigResponse, ExecuteMsg, QueryMsg};
use astroport_tests::base::BaseAstroportTestPackage;
use terra_multi_test::{AppBuilder, BankKeeper, Executor, TerraApp, TerraMock};

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

#[test]
fn instantiation() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let voting_escrow = Addr::unchecked("voting_escrow");
    let emergency_return = Addr::unchecked("emergency_return");

    let base_test_pack = BaseAstroportTestPackage::init_escrow_fee_distributor(
        router_ref,
        owner.clone(),
        voting_escrow.clone(),
        emergency_return.clone(),
    );

    let escrow_fee_distributor = base_test_pack.escrow_fee_distributor.unwrap();

    let resp: ConfigResponse = router
        .wrap()
        .query_wasm_smart(
            &escrow_fee_distributor.address.clone(),
            &QueryMsg::Config {},
        )
        .unwrap();
    assert_eq!(owner, resp.owner);
    assert_eq!(base_test_pack.astro_token.unwrap().address, resp.token);
    assert_eq!(voting_escrow, resp.voting_escrow);
    assert_eq!(emergency_return, resp.emergency_return);
    assert_eq!(0u64, resp.last_token_time);
    assert_eq!(0u64, resp.start_time);
    assert_eq!(false, resp.can_checkpoint_token);
    assert_eq!(0u64, resp.time_cursor);
    assert_eq!(false, resp.is_killed);
}

#[test]
fn test_kill_me() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let voting_escrow = Addr::unchecked("voting_escrow");
    let emergency_return = Addr::unchecked("emergency_return");

    let base_test_pack = BaseAstroportTestPackage::init_escrow_fee_distributor(
        router_ref,
        owner.clone(),
        voting_escrow.clone(),
        emergency_return.clone(),
    );

    let escrow_fee_distributor = base_test_pack.clone().escrow_fee_distributor.unwrap();
    let resp: ConfigResponse = router_ref
        .wrap()
        .query_wasm_smart(
            &escrow_fee_distributor.address.clone(),
            &QueryMsg::Config {},
        )
        .unwrap();

    assert_eq!(false, resp.is_killed);

    // Try to kill contract from anyone
    let err = router_ref
        .execute_contract(
            Addr::unchecked("not_owner"),
            escrow_fee_distributor.address.clone(),
            &ExecuteMsg::KillMe {},
            &[],
        )
        .unwrap_err();

    assert_eq!("Unauthorized", err.to_string());

    BaseAstroportTestPackage::mint_some_astro(
        router_ref,
        owner.clone(),
        base_test_pack.clone().astro_token.unwrap().address,
        escrow_fee_distributor.address.clone().as_str(),
        100u128,
    );

    // check if escrow_fee_distributor ASTRO balance is 100
    BaseAstroportTestPackage::check_token_balance(
        router_ref,
        &base_test_pack.clone().astro_token.unwrap().address,
        &escrow_fee_distributor.clone().address,
        100u128,
    );

    // check if emergency_return ASTRO balance is 0
    BaseAstroportTestPackage::check_token_balance(
        router_ref,
        &base_test_pack.clone().astro_token.unwrap().address,
        &emergency_return,
        0u128,
    );

    let resp = router_ref
        .execute_contract(
            owner.clone(),
            escrow_fee_distributor.clone().address,
            &ExecuteMsg::KillMe {},
            &[],
        )
        .unwrap();

    // check if escrow_fee_distributor ASTRO balance is 0
    BaseAstroportTestPackage::check_token_balance(
        router_ref,
        &base_test_pack.clone().astro_token.unwrap().address,
        &escrow_fee_distributor.clone().address,
        0u128,
    );

    // check if emergency_return ASTRO balance is 100
    BaseAstroportTestPackage::check_token_balance(
        router_ref,
        &base_test_pack.clone().astro_token.unwrap().address,
        &emergency_return.clone(),
        100u128,
    );

    assert_eq!(
        vec![
            attr("action", "kill_me"),
            attr("transferred_balance", "100"),
            attr("recipient", emergency_return.to_string()),
        ],
        vec![
            resp.events[1].attributes[1].clone(),
            resp.events[1].attributes[2].clone(),
            resp.events[1].attributes[3].clone(),
        ]
    );
}

#[test]
fn claim() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let voting_escrow = Addr::unchecked("voting_escrow");
    let emergency_return = Addr::unchecked("emergency_return");

    let base_test_pack = BaseAstroportTestPackage::init_escrow_fee_distributor(
        router_ref,
        owner.clone(),
        voting_escrow.clone(),
        emergency_return.clone(),
    );

    let escrow_fee_distributor = base_test_pack.escrow_fee_distributor.unwrap();

    let resp: ConfigResponse = router
        .wrap()
        .query_wasm_smart(&escrow_fee_distributor.address, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(owner, resp.owner);
    assert_eq!(base_test_pack.astro_token.unwrap().address, resp.token);
    assert_eq!(voting_escrow, resp.voting_escrow);
    assert_eq!(emergency_return, resp.emergency_return);
    assert_eq!(0u64, resp.last_token_time);
    assert_eq!(0u64, resp.start_time);
    assert_eq!(false, resp.can_checkpoint_token);
    assert_eq!(0u64, resp.time_cursor);
}
