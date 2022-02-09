use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, Addr, StdResult, Uint128};

use astroport_escrow_fee_distributor::utils::get_period;
use astroport_governance::escrow_fee_distributor::{ConfigResponse, ExecuteMsg, QueryMsg, WEEK};
use astroport_tests::base::{
    check_balance, increase_allowance, mint, BaseAstroportTestInitMessage,
    BaseAstroportTestPackage, MULTIPLIER,
};
use terra_multi_test::{next_block, AppBuilder, BankKeeper, Executor, TerraApp, TerraMock};

const OWNER: &str = "owner";
const EMERGENCY_RETURN: &str = "emergency_return";
const USER1: &str = "user1";
const USER2: &str = "user2";
const USER3: &str = "user3";

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
    assert_eq!(base_pack.astro_token.unwrap().address, resp.astro_token);
    assert_eq!(
        base_pack.voting_escrow.unwrap().address,
        resp.voting_escrow_addr
    );
    assert_eq!(
        Addr::unchecked(EMERGENCY_RETURN),
        resp.emergency_return_addr
    );
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

    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &escrow_fee_distributor.clone(),
        100u128,
    );

    // check if escrow_fee_distributor ASTRO balance is 100
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &escrow_fee_distributor.clone(),
        100u128,
    );

    // check if emergency_return ASTRO balance is 0
    check_balance(
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
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &escrow_fee_distributor.clone(),
        0u128,
    );

    // check if emergency_return ASTRO balance is 100
    check_balance(
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
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &user1,
        100u128,
    );

    // check if user1 ASTRO balance is 100
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        100u128,
    );

    // check if escrow_fee_distributor ASTRO balance is 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0u128,
    );

    increase_allowance(
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
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100u128,
    );

    // check if user1 ASTRO balance is 0
    check_balance(
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
            attr("action", "update_config"),
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
        .query_wasm_smart(
            &escrow_fee_distributor,
            &QueryMsg::VotingSupplyPerWeek {
                start_after: Option::from(get_period(
                    router_ref.block_info().time.seconds() - WEEK,
                )),
                limit: None,
            },
        )
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
    let user2 = Addr::unchecked(USER2.clone());
    let user3 = Addr::unchecked(USER3.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    // sets 100 ASTRO tokens to distributor (simulate receive astro from maker)
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100,
    );

    // checks if distributor's ASTRO token balance is equal to 100
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100,
    );

    let xastro_token = base_pack.get_staking_xastro(router_ref);

    // sets 200 * 1000_000 xASTRO tokens to user1
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user1,
        (200 * MULTIPLIER) as u128,
    );

    // checks if user1's xASTRO token balance is equal to 200 * 1000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user1,
        (200 * MULTIPLIER) as u128,
    );

    // sets 200 * 1000_000 xASTRO tokens to user2
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user2,
        (200 * MULTIPLIER) as u128,
    );

    // checks if user2's xASTRO token balance is equal to 200 * 1000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user2,
        (200 * MULTIPLIER) as u128,
    );

    // locks 100 xASTRO from user1 for WEEK * 2
    base_pack
        .create_lock(router_ref, user1.clone(), WEEK * 2, 100)
        .unwrap();

    // locks 200 vxASTRO from user2 for WEEK * 2
    base_pack
        .create_lock(router_ref, user2.clone(), WEEK * 2, 200)
        .unwrap();

    // try set checkpoint from user1 when it is disabled
    let err = router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap_err();

    assert_eq!("Checkpoint token is not available!", err.to_string());

    // try set checkpoint from owner
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap();

    // check if tokens per week is set
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::FeeTokensPerWeek {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(vec![Uint128::new(100)], resp);

    // going to the next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // sets 900 ASTRO tokens to distributor (simulate receive astro from maker)
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        900,
    );

    // try to claim some fee when is checkpoint per week is disabled
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if voting supply per week is set
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::VotingSupplyPerWeek {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(vec![Uint128::new(1442307), Uint128::new(721153),], resp);

    // check if distributor's ASTRO balance equal to 1000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        1000,
    );

    // check if user1's token balance equal to 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        0,
    );

    // check if user2's token balance equal to 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        0,
    );

    // allow checkpoint fee on the distributor
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

    // claim fee for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if tokens per week is set
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::FeeTokensPerWeek {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(vec![Uint128::new(215), Uint128::new(784)], resp); // one coin settles on the distributor.

    // check if distributor ASTRO balance equal to 929.
    // user1 fee: 4,807692308(user1 VP per week) × 215(tokens per week) ÷ 14,42307(total VP per week) = 71
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        929,
    );

    // check if user's token balance equal to 71
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        71,
    );

    // claim fee for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if distributor ASTRO balance equal to 786 = 929 - 143.
    // user2 fee: 9,615384615(user2 VP per week) × 215(tokens per week) ÷ 14,42307(total VP per week) = 143
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        786,
    );

    // check if user's token balance equal to 143
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        143,
    );

    // going to next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // claim fee for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if distributor ASTRO balance equal to 525 = 786 - 261.
    // user1 fee: 2,403846154(user1 VP per week) × 784(tokens per week) ÷ 7,211535(total VP per week) = 261
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        525,
    );

    // check if user1's token balance equal to 332 = 71 + 261
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        332,
    );

    // claim fee for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if distributor ASTRO balance equal to 3 = 525 - 522.
    // user1 fee: 4,807692307(user1 VP per week) × 784(tokens per week) ÷ 7,211535(total VP per week) = 522
    // 3 coins settles on the distributor.
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        3,
    );

    // check if user2's token balance equal to 665 = 143 + 522
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        665,
    );

    // sets 100 ASTRO tokens to distributor (simulate receive astro from maker)
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100,
    );

    // sets 200 * 1000_000 xASTRO tokens to user3
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user3,
        (200 * MULTIPLIER) as u128,
    );

    // checks if user3's xASTRO token balance is equal to 200 * 1000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user3,
        (200 * MULTIPLIER) as u128,
    );

    // locks 100 vxASTRO from user3 for WEEK
    base_pack
        .create_lock(router_ref, user3.clone(), WEEK, 100)
        .unwrap();

    // going to next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // checkpoint token
    router_ref
        .execute_contract(
            user3.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap();

    // check if distributor's ASTRO balance equal to 103 = 3 (from previous checkpoint) - 100 ( current checkpoint )
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        103,
    );
}
