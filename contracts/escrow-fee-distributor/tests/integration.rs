use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, Addr, Uint128};

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
    assert_eq!(10u64, resp.max_limit_accounts_of_claim);
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

    let resp: ConfigResponse = router_ref
        .wrap()
        .query_wasm_smart(
            &escrow_fee_distributor.address.clone(),
            &QueryMsg::Config {},
        )
        .unwrap();

    assert_eq!(true, resp.is_killed);

    // check if the contract is killed
    let resp = router_ref
        .execute_contract(
            owner.clone(),
            escrow_fee_distributor.clone().address,
            &ExecuteMsg::Burn {
                token_address: base_test_pack.astro_token.unwrap().address.to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!("Contract is killed!", resp.to_string());
}

#[test]
fn test_burn() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let voting_escrow = Addr::unchecked("voting_escrow");
    let emergency_return = Addr::unchecked("emergency_return");
    let user1 = Addr::unchecked("user1");

    let base_test_pack = BaseAstroportTestPackage::init_escrow_fee_distributor(
        router_ref,
        owner.clone(),
        voting_escrow.clone(),
        emergency_return.clone(),
    );

    let escrow_fee_distributor = base_test_pack.clone().escrow_fee_distributor.unwrap();
    let astro_token = base_test_pack.clone().astro_token.unwrap();
    // mint 100 ASTRO to user1
    BaseAstroportTestPackage::mint_some_astro(
        router_ref,
        owner.clone(),
        astro_token.address.clone(),
        user1.clone().as_str(),
        100u128,
    );

    // check if user1 ASTRO balance is 100
    BaseAstroportTestPackage::check_token_balance(
        router_ref,
        &astro_token.address.clone(),
        &user1.clone(),
        100u128,
    );

    // check if escrow_fee_distributor ASTRO balance is 0
    BaseAstroportTestPackage::check_token_balance(
        router_ref,
        &astro_token.address.clone(),
        &escrow_fee_distributor.clone().address,
        0u128,
    );

    BaseAstroportTestPackage::allowance_token(
        router_ref,
        user1.clone(),
        escrow_fee_distributor.clone().address,
        astro_token.address.clone(),
        Uint128::new(100),
    );

    let resp = router_ref
        .execute_contract(
            user1.clone(),
            escrow_fee_distributor.clone().address,
            &ExecuteMsg::Burn {
                token_address: astro_token.address.clone().to_string(),
            },
            &[],
        )
        .unwrap();

    // check if escrow_fee_distributor ASTRO balance is 100
    BaseAstroportTestPackage::check_token_balance(
        router_ref,
        &astro_token.address.clone(),
        &escrow_fee_distributor.clone().address,
        100u128,
    );

    // check if user1 ASTRO balance is 0
    BaseAstroportTestPackage::check_token_balance(
        router_ref,
        &astro_token.address.clone(),
        &user1.clone(),
        0u128,
    );

    assert_eq!(
        vec![attr("action", "burn"), attr("amount", "100"),],
        vec![
            resp.events[1].attributes[1].clone(),
            resp.events[1].attributes[2].clone(),
        ]
    );
}

#[test]
fn test_update_config() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let voting_escrow = Addr::unchecked("voting_escrow");
    let emergency_return = Addr::unchecked("emergency_return");
    let user1 = Addr::unchecked("user1");

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

    assert_eq!(10u64, resp.max_limit_accounts_of_claim);
    assert_eq!(false, resp.can_checkpoint_token);

    // check if anyone can't update configs
    let err = router_ref
        .execute_contract(
            user1.clone(),
            escrow_fee_distributor.clone().address,
            &ExecuteMsg::UpdateConfig {
                max_limit_accounts_of_claim: Some(20u64),
                can_checkpoint_token: Some(true),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!("Unauthorized", err.to_string());

    // check if only owner can update configs
    let resp = router_ref
        .execute_contract(
            owner.clone(),
            escrow_fee_distributor.clone().address,
            &ExecuteMsg::UpdateConfig {
                max_limit_accounts_of_claim: Some(20u64),
                can_checkpoint_token: Some(true),
            },
            &[],
        )
        .unwrap();

    let resp_config: ConfigResponse = router_ref
        .wrap()
        .query_wasm_smart(
            &escrow_fee_distributor.address.clone(),
            &QueryMsg::Config {},
        )
        .unwrap();

    assert_eq!(20u64, resp_config.max_limit_accounts_of_claim);
    assert_eq!(true, resp_config.can_checkpoint_token);

    assert_eq!(
        vec![
            attr("action", "set_config"),
            attr("can_checkpoint_token", "true"),
            attr("max_limit_accounts_of_claim", "20"),
        ],
        vec![
            resp.events[1].attributes[1].clone(),
            resp.events[1].attributes[2].clone(),
            resp.events[1].attributes[3].clone(),
        ]
    );
}

#[test]
fn test_checkpoint_total_supply() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let emergency_return = Addr::unchecked("emergency_return");
    let user1 = Addr::unchecked("user1");

    let voting_escrow_pack =
        BaseAstroportTestPackage::init_voting_escrow(router_ref, owner.clone());

    let base_test_pack = BaseAstroportTestPackage::init_escrow_fee_distributor(
        router_ref,
        owner.clone(),
        voting_escrow_pack.voting_escrow.unwrap().address,
        emergency_return.clone(),
    );

    let escrow_fee_distributor = base_test_pack.clone().escrow_fee_distributor.unwrap();

    router_ref
        .execute_contract(
            user1.clone(),
            escrow_fee_distributor.clone().address,
            &ExecuteMsg::CheckpointTotalSupply {},
            &[],
        )
        .unwrap();

    // checks if voting supply per week is set to zero
    let resp_config: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &escrow_fee_distributor.address.clone(),
            &QueryMsg::VotingSupplyPerWeek {},
        )
        .unwrap();

    let voting_supply: Vec<Uint128> = vec![Uint128::new(0); 19];
    assert_eq!(voting_supply, resp_config);
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

    let _escrow_fee_distributor = base_test_pack.escrow_fee_distributor.unwrap();

    // let resp = router_ref
    //     .execute_contract(
    //         owner.clone(),
    //         escrow_fee_distributor.clone().address,
    //         &ExecuteMsg::KillMe {},
    //         &[],
    //     )
    //     .unwrap();
}
