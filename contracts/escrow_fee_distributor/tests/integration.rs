use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, StdResult, Timestamp, Uint128};

use astroport_governance::utils::{get_period, EPOCH_START, WEEK};

use astroport_governance::escrow_fee_distributor::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, QueryMsg,
};
use astroport_governance::voting_escrow::{
    LockInfoResponse, QueryMsg as VotingEscrowQueryMsg, VotingPowerResponse,
};

use astroport_tests::base::{
    check_balance, mint, BaseAstroportTestInitMessage, BaseAstroportTestPackage, MULTIPLIER,
};
use cw20::Cw20ExecuteMsg;
use terra_multi_test::{next_block, AppBuilder, BankKeeper, Executor, TerraApp, TerraMock};

const OWNER: &str = "owner";
const USER1: &str = "user1";
const USER2: &str = "user2";
const USER3: &str = "user3";
const USER4: &str = "user4";
const USER5: &str = "user5";
const MAKER: &str = "maker";

fn mock_app() -> TerraApp {
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(EPOCH_START);
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

    assert_eq!(owner, resp.owner);
    assert_eq!(base_pack.astro_token.unwrap().address, resp.astro_token);
    assert_eq!(
        base_pack.voting_escrow.unwrap().address,
        resp.voting_escrow_addr
    );
    assert_eq!(false, resp.is_claim_disabled);
    assert_eq!(10u64, resp.claim_many_limit);
}

#[test]
fn test_receive_tokens() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let maker = Addr::unchecked(MAKER.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    // Mint 1000_000_000 ASTRO for the Maker
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &maker,
        1000,
    );

    // Check if Maker's ASTRO balance is 1000_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &maker,
        1000 * MULTIPLIER as u128,
    );

    // Check if escrow_fee_distributor ASTRO balance is 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0u128,
    );

    // Try to send 100_000_000 ASTRO from Maker to distributor
    let msg = Cw20ExecuteMsg::Send {
        contract: base_pack
            .escrow_fee_distributor
            .clone()
            .unwrap()
            .address
            .to_string(),
        msg: to_binary(&Cw20HookMsg::ReceiveTokens {}).unwrap(),
        amount: Uint128::from(100 * MULTIPLIER as u128),
    };

    router_ref
        .execute_contract(
            maker.clone(),
            base_pack.astro_token.clone().unwrap().address,
            &msg,
            &[],
        )
        .unwrap();

    // Sends 100_000_000 ASTRO from Maker to distributor for the next 5 weeks
    for _i in 0..5 {
        router_ref
            .execute_contract(
                maker.clone(),
                base_pack.astro_token.clone().unwrap().address,
                &msg,
                &[],
            )
            .unwrap();

        // Going to the next week
        router_ref.update_block(next_block);
        router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));
    }

    // Check if escrow_fee_distributor's ASTRO balance is equal to 600_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        600 * MULTIPLIER as u128,
    );

    // Check if Maker's ASTRO balance is equal to 400_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &maker.clone(),
        400 * MULTIPLIER as u128,
    );

    // Checks rewards per week
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::AvailableRewardPerWeek {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(
        vec![
            Uint128::new(200_000_000),
            Uint128::new(100_000_000),
            Uint128::new(100_000_000),
            Uint128::new(100_000_000),
            Uint128::new(100_000_000),
        ],
        resp
    );
}

#[test]
fn update_config() {
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

    assert_eq!(10u64, resp.claim_many_limit);
    assert_eq!(false, resp.is_claim_disabled);

    // Check if a random address can update the config
    let err = router_ref
        .execute_contract(
            user1.clone(),
            escrow_fee_distributor.clone(),
            &ExecuteMsg::UpdateConfig {
                claim_many_limit: Some(20u64),
                is_claim_disabled: Some(true),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!("Unauthorized", err.to_string());

    // Check that the owner can update the config
    let resp = router_ref
        .execute_contract(
            owner.clone(),
            escrow_fee_distributor.clone(),
            &ExecuteMsg::UpdateConfig {
                claim_many_limit: Some(20u64),
                is_claim_disabled: Some(true),
            },
            &[],
        )
        .unwrap();

    let resp_config: ConfigResponse = router_ref
        .wrap()
        .query_wasm_smart(&escrow_fee_distributor.clone(), &QueryMsg::Config {})
        .unwrap();

    assert_eq!(20u64, resp_config.claim_many_limit);
    assert_eq!(true, resp_config.is_claim_disabled);

    assert_eq!(
        vec![
            attr("action", "update_config"),
            attr("is_claim_disabled", "true"),
            attr("claim_many_limit", "20"),
        ],
        vec![
            resp.events[1].attributes[1].clone(),
            resp.events[1].attributes[2].clone(),
            resp.events[1].attributes[3].clone(),
        ]
    );
}

#[test]
fn check_if_user_exists_after_withdraw() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let user1 = Addr::unchecked(USER1.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();
    let xastro_token = base_pack.get_staking_xastro(router_ref);

    // Send 200_000_000 xASTRO tokens to user1
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user1,
        200,
    );

    // Create lock for user1 for WEEK
    base_pack
        .create_lock(router_ref, user1.clone(), WEEK, 200)
        .unwrap();

    // Going to the last week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    let resp: LockInfoResponse = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.voting_escrow.clone().unwrap().address,
            &VotingEscrowQueryMsg::LockInfo {
                user: user1.to_string(),
            },
        )
        .unwrap();

    assert_eq!(Uint128::new(200_000_000), resp.amount);
    assert_eq!(
        get_period(router_ref.block_info().time.seconds() - WEEK).unwrap(),
        resp.start
    );
    assert_eq!(
        get_period(router_ref.block_info().time.seconds()).unwrap(),
        resp.end
    );

    base_pack.withdraw(router_ref, user1.as_str()).unwrap();

    let resp: LockInfoResponse = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.voting_escrow.clone().unwrap().address,
            &VotingEscrowQueryMsg::LockInfo {
                user: user1.to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.amount, Uint128::zero());
    assert_eq!(
        resp.start,
        get_period(router_ref.block_info().time.minus_seconds(WEEK).seconds()).unwrap()
    );
    assert_eq!(
        resp.end,
        get_period(router_ref.block_info().time.seconds()).unwrap()
    );
}

#[test]
fn claim_without_fee_on_distributor() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let user1 = Addr::unchecked(USER1.clone());
    let user2 = Addr::unchecked(USER2.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    let xastro_token = base_pack.get_staking_xastro(router_ref);

    // Sets 200_000_000 xASTRO tokens to user1
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user1,
        200,
    );

    // Send 200_000_000 xASTRO tokens to user2
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user2,
        200,
    );

    // Create lock for user1 for WEEK * 104
    base_pack
        .create_lock(router_ref, user1.clone(), WEEK * 104, 200)
        .unwrap();

    // Create lock for user2 for WEEK * 104
    base_pack
        .create_lock(router_ref, user2.clone(), WEEK * 104, 200)
        .unwrap();

    // Going to the last week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK * 103));

    // Try to claim fees for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // Try to claim fees for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // Check if user1's ASTRO balance is equal to 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        0,
    );

    // Check if user2's ASTRO balance is equal to 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        0,
    );
}

#[test]
fn claim_max_period() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let maker = Addr::unchecked(MAKER.clone());
    let user1 = Addr::unchecked(USER1.clone());
    let user2 = Addr::unchecked(USER2.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    let xastro_token = base_pack.get_staking_xastro(router_ref);

    // Send 200_000_000 xASTRO tokens to user1
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user1,
        200,
    );

    // Send 200_000_000 xASTRO tokens to user2
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user2,
        200,
    );

    // Create lock for user1 for WEEK * 104
    base_pack
        .create_lock(router_ref, user1.clone(), WEEK * 104, 200)
        .unwrap();

    // Create lock for user2 for WEEK * 104
    base_pack
        .create_lock(router_ref, user2.clone(), WEEK * 104, 200)
        .unwrap();

    // Mint 100_000_000 ASTRO for the Maker
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &maker,
        100,
    );

    // Try to send 100_000_000 ASTRO from Maker to distributor for the first period
    let msg = Cw20ExecuteMsg::Send {
        contract: base_pack
            .escrow_fee_distributor
            .clone()
            .unwrap()
            .address
            .to_string(),
        msg: to_binary(&Cw20HookMsg::ReceiveTokens {}).unwrap(),
        amount: Uint128::from(100 * MULTIPLIER as u128),
    };

    router_ref
        .execute_contract(
            maker.clone(),
            base_pack.astro_token.clone().unwrap().address,
            &msg,
            &[],
        )
        .unwrap();

    // Going to the next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // Mint 100_000_000 ASTRO for the Maker
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &maker,
        100,
    );

    // Try to send 100_000_000 ASTRO from Maker to distributor for the second period
    let msg = Cw20ExecuteMsg::Send {
        contract: base_pack
            .escrow_fee_distributor
            .clone()
            .unwrap()
            .address
            .to_string(),
        msg: to_binary(&Cw20HookMsg::ReceiveTokens {}).unwrap(),
        amount: Uint128::from(100 * MULTIPLIER as u128),
    };

    router_ref
        .execute_contract(
            maker.clone(),
            base_pack.astro_token.clone().unwrap().address,
            &msg,
            &[],
        )
        .unwrap();

    // Warping to the week after user's lock period ends
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK * 105));

    // Check if rewards for the first and the second weeks equal 100_000_000 ASTRO
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::AvailableRewardPerWeek {
                start_after: None,
                limit: Some(2),
            },
        )
        .unwrap();
    assert_eq!(
        vec![Uint128::new(100_000_000), Uint128::new(100_000_000)],
        resp
    );

    // Claim fees for max period for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // Claim fees for max period for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // Check if user1's ASTRO balance is equal to 100_000_000 ASTRO
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        100_000_000,
    );

    // Check if user2's ASTRO balance equal to 100_000_000 ASTRO
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        100_000_000,
    );

    // Check if distributor's ASTRO balance equal to 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0,
    );
}

#[test]
fn claim_multiple_users() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let maker = Addr::unchecked(MAKER.clone());
    let user1 = Addr::unchecked(USER1.clone());
    let user2 = Addr::unchecked(USER2.clone());
    let user3 = Addr::unchecked(USER3.clone());
    let user4 = Addr::unchecked(USER4.clone());
    let user5 = Addr::unchecked(USER5.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    let xastro_token = base_pack.get_staking_xastro(router_ref);

    for user in [user1.clone(), user2.clone(), user3.clone(), user4.clone()] {
        // Sends 200_000_000 xASTRO tokens to users
        mint(
            router_ref,
            base_pack.staking.clone().unwrap().address,
            xastro_token.clone(),
            &user,
            200,
        );

        // Checks if user's xASTRO balance is equal to 200 * 1000_000 ASTRO
        check_balance(
            router_ref,
            &xastro_token.clone(),
            &user,
            200 * MULTIPLIER as u128,
        );

        // Create lock for user for WEEK * 2
        base_pack
            .create_lock(router_ref, user.clone(), WEEK * 2, 100)
            .unwrap();
    }

    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &maker,
        1000,
    );

    // Sends 100_000_000 ASTRO from Maker to distributor for the first period
    let msg = Cw20ExecuteMsg::Send {
        contract: base_pack
            .escrow_fee_distributor
            .clone()
            .unwrap()
            .address
            .to_string(),
        msg: to_binary(&Cw20HookMsg::ReceiveTokens {}).unwrap(),
        amount: Uint128::from(100 * MULTIPLIER as u128),
    };

    router_ref
        .execute_contract(
            maker.clone(),
            base_pack.astro_token.clone().unwrap().address,
            &msg,
            &[],
        )
        .unwrap();

    // Checks if distributor's ASTRO balance is equal to 100_000_000 ASTRO
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100 * MULTIPLIER as u128,
    );

    // Check if rewards per week are set to 100_000_000 ASTRO
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::AvailableRewardPerWeek {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(vec![Uint128::new(100_000_000)], resp);

    // Check if weekly voting supply can be queried
    let resp: VotingPowerResponse = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.voting_escrow.clone().unwrap().address,
            &VotingEscrowQueryMsg::TotalVotingPowerAt {
                time: router_ref.block_info().time.seconds(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::new(411_538_456), resp.voting_power);

    // Go to the next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // Perform an operation for an unlimited number of users
    let err = router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::ClaimMany {
                receivers: vec![
                    user1.to_string(),
                    user2.to_string(),
                    user3.to_string(),
                    user4.to_string(),
                    Addr::unchecked("user5").to_string(),
                    Addr::unchecked("user6").to_string(),
                    Addr::unchecked("user7").to_string(),
                    Addr::unchecked("user8").to_string(),
                    Addr::unchecked("user9").to_string(),
                    Addr::unchecked("user10").to_string(),
                    Addr::unchecked("user11").to_string(),
                ],
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        "Exceeded account limit for claim operation!",
        err.to_string()
    );

    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user5,
        200,
    );

    // Check if user5's xASTRO balance is equal to 200 * 1000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user5,
        200 * MULTIPLIER as u128,
    );

    // Create lock for user5 for WEEK * 2
    base_pack
        .create_lock(router_ref, user5.clone(), WEEK * 2, 100)
        .unwrap();

    // Claim for all users
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::ClaimMany {
                receivers: vec![
                    user1.to_string(),
                    user2.to_string(),
                    user3.to_string(),
                    user4.to_string(),
                    user5.to_string(),
                ],
            },
            &[],
        )
        .unwrap();

    // Checks if user's ASTRO balance is equal to 100 / 4 = 25 * 1_000_000
    for user in [user1.clone(), user2.clone(), user3.clone(), user4.clone()] {
        check_balance(
            router_ref,
            &base_pack.astro_token.clone().unwrap().address,
            &user,
            25 * MULTIPLIER as u128,
        );
    }

    // Checks if user5's ASTRO balance is equal to 0. Cannot claim for the current period
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user5,
        0,
    );

    // Check if distributor's ASTRO balance equal to 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0,
    );

    // Going to next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // Sends 900_000_000 ASTRO from the Maker to the distributor for the third period
    let msg = Cw20ExecuteMsg::Send {
        contract: base_pack
            .escrow_fee_distributor
            .clone()
            .unwrap()
            .address
            .to_string(),
        msg: to_binary(&Cw20HookMsg::ReceiveTokens {}).unwrap(),
        amount: Uint128::from(900 * MULTIPLIER as u128),
    };

    router_ref
        .execute_contract(
            maker.clone(),
            base_pack.astro_token.clone().unwrap().address,
            &msg,
            &[],
        )
        .unwrap();

    // Check if rewards per week are set to 900_000_000 ASTRO
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::AvailableRewardPerWeek {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(
        vec![Uint128::new(100_000_000), Uint128::new(900_000_000),],
        resp
    );

    // Try to claim for all users for the current period
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::ClaimMany {
                receivers: vec![
                    user1.to_string(),
                    user2.to_string(),
                    user3.to_string(),
                    user4.to_string(),
                    user5.to_string(),
                ],
            },
            &[],
        )
        .unwrap();

    // Checks if the user's ASTRO token balance is still equal to 100 / 4 = 25 * 1_000_000
    for user in [user1.clone(), user2.clone(), user3.clone(), user4.clone()] {
        check_balance(
            router_ref,
            &base_pack.astro_token.clone().unwrap().address,
            &user,
            25 * MULTIPLIER as u128,
        );
    }

    // Check if user5's ASTRO balance is 0 for the first lock week
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user5,
        0,
    );

    // Check if the distributor's ASTRO balance is still equal to 900_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        900_000_000,
    );

    // Going to next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // Try to claim for all users
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::ClaimMany {
                receivers: vec![
                    user1.to_string(),
                    user2.to_string(),
                    user3.to_string(),
                    user4.to_string(),
                    user5.to_string(),
                ],
            },
            &[],
        )
        .unwrap();

    // Checks if the user's ASTRO balance is still equal to 25 * 100_000_000.
    for user in [user1.clone(), user2.clone(), user3.clone(), user4.clone()] {
        check_balance(
            router_ref,
            &base_pack.astro_token.clone().unwrap().address,
            &user,
            25 * MULTIPLIER as u128,
        );
    }

    // Checks if user5's ASTRO balance equal to 900_000_000 for the second week of lock
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user5,
        900_000_000,
    );

    // Check if distributor's ASTRO balance still equal to 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0,
    );
}

#[test]
fn is_claim_enabled() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let maker = Addr::unchecked(MAKER.clone());
    let user1 = Addr::unchecked(USER1.clone());
    let user2 = Addr::unchecked(USER2.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    let xastro_token = base_pack.get_staking_xastro(router_ref);

    // Sends 200_000_000 xASTRO tokens to users
    for user in [user1.clone(), user2.clone()] {
        mint(
            router_ref,
            base_pack.staking.clone().unwrap().address,
            xastro_token.clone(),
            &user,
            200,
        );

        // Checks if user's xASTRO token balance is equal to 200 * 1000_000
        check_balance(
            router_ref,
            &xastro_token.clone(),
            &user,
            200 * MULTIPLIER as u128,
        );

        // Create a lock for user for WEEK * 3
        base_pack
            .create_lock(router_ref, user.clone(), WEEK * 3, 100)
            .unwrap();
    }

    // Send 1000_000_000 ASTRO tokens to the Maker
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &maker,
        1000,
    );

    // Send 100_000_000 ASTRO from the Maker to the distributor for the first period
    let msg = Cw20ExecuteMsg::Send {
        contract: base_pack
            .escrow_fee_distributor
            .clone()
            .unwrap()
            .address
            .to_string(),
        msg: to_binary(&Cw20HookMsg::ReceiveTokens {}).unwrap(),
        amount: Uint128::from(100 * MULTIPLIER as u128),
    };

    router_ref
        .execute_contract(
            maker.clone(),
            base_pack.astro_token.clone().unwrap().address,
            &msg,
            &[],
        )
        .unwrap();

    // Check if distributor's ASTRO balance is equal to 100_000_000 ASTRO
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100 * MULTIPLIER as u128,
    );

    // Checl if rewards are set to 100_000_000 ASTRO
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::AvailableRewardPerWeek {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(vec![Uint128::new(100_000_000)], resp);

    // Going to the next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // Disable claiming
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::UpdateConfig {
                claim_many_limit: None,
                is_claim_disabled: Some(true),
            },
            &[],
        )
        .unwrap();

    // Try to claim fees for all users for the first week
    let err = router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::ClaimMany {
                receivers: vec![user1.to_string(), user2.to_string()],
            },
            &[],
        )
        .unwrap_err();

    assert_eq!("Claim is disabled!", err.to_string());

    // Send 100_000_000 ASTRO from the Maker to the distributor for the first period
    let msg = Cw20ExecuteMsg::Send {
        contract: base_pack
            .escrow_fee_distributor
            .clone()
            .unwrap()
            .address
            .to_string(),
        msg: to_binary(&Cw20HookMsg::ReceiveTokens {}).unwrap(),
        amount: Uint128::from(100 * MULTIPLIER as u128),
    };

    router_ref
        .execute_contract(
            maker.clone(),
            base_pack.astro_token.clone().unwrap().address,
            &msg,
            &[],
        )
        .unwrap();

    // Going to the next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // Try to claim fees for all users
    let err = router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::ClaimMany {
                receivers: vec![user1.to_string(), user2.to_string()],
            },
            &[],
        )
        .unwrap_err();

    assert_eq!("Claim is disabled!", err.to_string());

    // Going to the next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // Enable claiming
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::UpdateConfig {
                claim_many_limit: None,
                is_claim_disabled: Some(false),
            },
            &[],
        )
        .unwrap();

    // Try to claim fees for all users
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::ClaimMany {
                receivers: vec![user1.to_string(), user2.to_string()],
            },
            &[],
        )
        .unwrap();

    // Check if the user's ASTRO token balance is equal to 25 * 1_000_000
    for user in [user1.clone(), user2.clone()] {
        check_balance(
            router_ref,
            &base_pack.astro_token.clone().unwrap().address,
            &user,
            100 * MULTIPLIER as u128,
        );
    }

    // Check if the distributor's ASTRO balance is 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0,
    );
}
