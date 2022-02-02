use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, Addr, StdResult, Uint128};

use astroport_governance::escrow_fee_distributor::{ConfigResponse, ExecuteMsg, QueryMsg, WEEK};
use astroport_tests::base::{BaseAstroportTestInitMessage, BaseAstroportTestPackage, MULTIPLIER};
use terra_multi_test::{next_block, AppBuilder, BankKeeper, Executor, TerraApp, TerraMock};

const OWNER: &str = "owner";
const EMERGENCY_RETURN: &str = "emergency_return";
const USER1: &str = "user1";

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

fn init_astroport_test_package(router: &mut TerraApp) -> StdResult<BaseAstroportTestPackage> {
    let base_msg = BaseAstroportTestInitMessage {
        owner: Addr::unchecked(OWNER),
        emergency_return: Addr::unchecked(EMERGENCY_RETURN),
        start_time: Option::from(router.block_info().time.seconds()),
    };

    Ok(BaseAstroportTestPackage::init_all(router, base_msg))
}
#[test]
fn instantiation() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER);

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    let resp: ConfigResponse = router
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::Config {},
        )
        .unwrap();

    let time_point = router.block_info().time.seconds() / WEEK * WEEK;

    assert_eq!(owner, resp.owner);
    assert_eq!(base_pack.astro_token.unwrap().address, resp.token);
    assert_eq!(base_pack.voting_escrow.unwrap().address, resp.voting_escrow);
    assert_eq!(Addr::unchecked(EMERGENCY_RETURN), resp.emergency_return);
    assert_eq!(time_point, resp.last_token_time);
    assert_eq!(time_point, resp.start_time);
    assert_eq!(false, resp.can_checkpoint_token);
    assert_eq!(time_point, resp.time_cursor);
    assert_eq!(false, resp.is_killed);
    assert_eq!(10u64, resp.max_limit_accounts_of_claim);
}

#[test]
fn test_kill_me() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let emergency_return = Addr::unchecked("emergency_return");

    let base_pack = init_astroport_test_package(router_ref).unwrap();
    let escrow_fee_distributor = base_pack.escrow_fee_distributor.clone().unwrap().address;

    let resp: ConfigResponse = router_ref
        .wrap()
        .query_wasm_smart(&escrow_fee_distributor.clone(), &QueryMsg::Config {})
        .unwrap();

    assert_eq!(false, resp.is_killed);

    // Try to kill contract from anyone
    let err = router_ref
        .execute_contract(
            Addr::unchecked("not_owner"),
            escrow_fee_distributor.clone(),
            &ExecuteMsg::KillMe {},
            &[],
        )
        .unwrap_err();

    assert_eq!("Unauthorized", err.to_string());

    BaseAstroportTestPackage::mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &escrow_fee_distributor.clone(),
        100u128,
    );

    // check if escrow_fee_distributor ASTRO balance is 100
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &escrow_fee_distributor.clone(),
        100u128,
    );

    // check if emergency_return ASTRO balance is 0
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &emergency_return,
        0u128,
    );

    let resp = router_ref
        .execute_contract(
            owner.clone(),
            escrow_fee_distributor.clone(),
            &ExecuteMsg::KillMe {},
            &[],
        )
        .unwrap();

    // check if escrow_fee_distributor ASTRO balance is 0
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &escrow_fee_distributor.clone(),
        0u128,
    );

    // check if emergency_return ASTRO balance is 100
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
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
        .query_wasm_smart(&escrow_fee_distributor.clone(), &QueryMsg::Config {})
        .unwrap();

    assert_eq!(true, resp.is_killed);

    // try call operation on the killed contract
    let resp = router_ref
        .execute_contract(
            owner.clone(),
            escrow_fee_distributor.clone(),
            &ExecuteMsg::Burn {
                token_address: base_pack.astro_token.unwrap().address.to_string(),
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
    let owner = Addr::unchecked(OWNER.clone());
    let user1 = Addr::unchecked(USER1.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    // mint 100 ASTRO to user1
    BaseAstroportTestPackage::mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &user1,
        100u128,
    );

    // check if user1 ASTRO balance is 100
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        100u128,
    );

    // check if escrow_fee_distributor ASTRO balance is 0
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0u128,
    );

    BaseAstroportTestPackage::increase_allowance(
        router_ref,
        user1.clone(),
        base_pack.escrow_fee_distributor.clone().unwrap().address,
        base_pack.astro_token.clone().unwrap().address,
        Uint128::new(100),
    );

    let resp = router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Burn {
                token_address: base_pack.astro_token.clone().unwrap().address.to_string(),
            },
            &[],
        )
        .unwrap();

    // check if escrow_fee_distributor ASTRO balance is 100
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100u128,
    );

    // check if user1 ASTRO balance is 0
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
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
    let owner = Addr::unchecked(OWNER.clone());
    let user1 = Addr::unchecked(USER1.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();
    let escrow_fee_distributor = base_pack.escrow_fee_distributor.unwrap().address;

    let resp: ConfigResponse = router_ref
        .wrap()
        .query_wasm_smart(&escrow_fee_distributor.clone(), &QueryMsg::Config {})
        .unwrap();

    assert_eq!(10u64, resp.max_limit_accounts_of_claim);
    assert_eq!(false, resp.can_checkpoint_token);

    // check if anyone can't update configs
    let err = router_ref
        .execute_contract(
            user1.clone(),
            escrow_fee_distributor.clone(),
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
            escrow_fee_distributor.clone(),
            &ExecuteMsg::UpdateConfig {
                max_limit_accounts_of_claim: Some(20u64),
                can_checkpoint_token: Some(true),
            },
            &[],
        )
        .unwrap();

    let resp_config: ConfigResponse = router_ref
        .wrap()
        .query_wasm_smart(&escrow_fee_distributor.clone(), &QueryMsg::Config {})
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

    let base_pack = init_astroport_test_package(router_ref).unwrap();
    let escrow_fee_distributor = base_pack.escrow_fee_distributor.unwrap().address;
    router_ref
        .execute_contract(
            Addr::unchecked(USER1.clone()),
            escrow_fee_distributor.clone(),
            &ExecuteMsg::CheckpointTotalSupply {},
            &[],
        )
        .unwrap();

    // checks if voting supply per week is set to zero
    let resp_config: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(&escrow_fee_distributor, &QueryMsg::VotingSupplyPerWeek {})
        .unwrap();

    let voting_supply: Vec<Uint128> = vec![Uint128::new(0); 1];
    assert_eq!(voting_supply, resp_config);
}

#[test]
fn claim() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let user1 = Addr::unchecked(USER1.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();
    BaseAstroportTestPackage::mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &user1,
        200,
    );
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        200,
    );

    BaseAstroportTestPackage::mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100,
    );
    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100,
    );

    let xastro_token = base_pack.get_staking_xastro(router_ref);
    BaseAstroportTestPackage::mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user1,
        (200 * MULTIPLIER) as u128,
    );

    BaseAstroportTestPackage::check_balance(
        router_ref,
        &xastro_token.clone(),
        &user1,
        (200 * MULTIPLIER) as u128,
    );

    base_pack
        .create_lock(router_ref, user1.clone(), WEEK * 2, 100)
        .unwrap();

    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::UpdateConfig {
                max_limit_accounts_of_claim: None,
                can_checkpoint_token: Option::from(true),
            },
            &[],
        )
        .unwrap();

    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0,
    );

    BaseAstroportTestPackage::check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        300,
    );
}
