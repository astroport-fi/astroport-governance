use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, StdResult, Uint128};

use astroport_governance::utils::{get_period, WEEK};

use astroport_governance::escrow_fee_distributor::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, QueryMsg,
};
use astroport_governance::voting_escrow::{QueryMsg as VotingEscrowQueryMsg, VotingPowerResponse};

use astroport_tests::base::{
    check_balance, mint, BaseAstroportTestInitMessage, BaseAstroportTestPackage, MULTIPLIER,
};
use cw20::Cw20ExecuteMsg;
use terra_multi_test::{next_block, AppBuilder, BankKeeper, Executor, TerraApp, TerraMock};

const OWNER: &str = "owner";
const EMERGENCY_RETURN: &str = "emergency_return";
const USER1: &str = "user1";
const USER2: &str = "user2";
const USER3: &str = "user3";
const USER4: &str = "user4";
const MAKER: &str = "maker";

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
    assert_eq!(false, resp.checkpoint_token_enabled);
    assert_eq!(time_point, resp.time_cursor);
    assert_eq!(10u64, resp.max_limit_accounts_of_claim);
}

#[test]
fn test_burn() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let maker = Addr::unchecked(MAKER.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    // mint 100_000_000 ASTRO to maker
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &maker,
        100,
    );

    // check if maker ASTRO balance is 100_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &maker,
        100 * MULTIPLIER as u128,
    );

    // check if escrow_fee_distributor ASTRO balance is 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0u128,
    );

    // try to send 100 ASTRO from maker to distributor
    let msg = Cw20ExecuteMsg::Send {
        contract: base_pack
            .escrow_fee_distributor
            .clone()
            .unwrap()
            .address
            .to_string(),
        msg: to_binary(&Cw20HookMsg::Burn {}).unwrap(),
        amount: Uint128::from(100u128),
    };

    router_ref
        .execute_contract(
            maker.clone(),
            base_pack.astro_token.clone().unwrap().address,
            &msg,
            &[],
        )
        .unwrap();

    // check if escrow_fee_distributor's ASTRO balance is 100
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100u128,
    );

    // check if maker ASTRO balance is 99999900
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &maker.clone(),
        99999900u128,
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

    assert_eq!(10u64, resp.max_limit_accounts_of_claim);
    assert_eq!(false, resp.checkpoint_token_enabled);

    // check if anyone can't update configs
    let err = router_ref
        .execute_contract(
            user1.clone(),
            escrow_fee_distributor.clone(),
            &ExecuteMsg::UpdateConfig {
                max_limit_accounts_of_claim: Some(20u64),
                checkpoint_token_enabled: Some(true),
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
                checkpoint_token_enabled: Some(true),
            },
            &[],
        )
        .unwrap();

    let resp_config: ConfigResponse = router_ref
        .wrap()
        .query_wasm_smart(&escrow_fee_distributor.clone(), &QueryMsg::Config {})
        .unwrap();

    assert_eq!(20u64, resp_config.max_limit_accounts_of_claim);
    assert_eq!(true, resp_config.checkpoint_token_enabled);

    assert_eq!(
        vec![
            attr("action", "update_config"),
            attr("checkpoint_token_enabled", "true"),
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
fn claim_without_fee_on_distributor() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let user1 = Addr::unchecked(USER1.clone());
    let user2 = Addr::unchecked(USER2.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    let xastro_token = base_pack.get_staking_xastro(router_ref);

    // sets 200_000_000 xASTRO tokens to user1
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user1,
        200,
    );

    // sets 200_000_000 xASTRO tokens to user2
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user2,
        200,
    );

    // locks 100 xASTRO from user1 for WEEK * 2
    base_pack
        .create_lock(router_ref, user1.clone(), WEEK * 104, 200)
        .unwrap();

    // locks 100 xASTRO from user2 for WEEK * 2
    base_pack
        .create_lock(router_ref, user2.clone(), WEEK * 104, 200)
        .unwrap();

    // going to the last week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK * 103));

    // set checkpoints for each week
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap();

    // check if tokens for last week equal to 0
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::FeeTokensPerWeek {
                start_after: Some(get_period(router_ref.block_info().time.seconds() - WEEK)),
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(vec![Uint128::new(0)], resp);

    // check if voting supply per week is set
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::VotingSupplyPerWeek {
                start_after: None,
                limit: Some(2),
            },
        )
        .unwrap();
    assert_eq!(
        vec![Uint128::new(1000000000), Uint128::new(990384615)],
        resp
    );

    // try to claim fee for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // try to claim fee for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

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
}

#[test]
fn claim_max_period() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let user1 = Addr::unchecked(USER1.clone());
    let user2 = Addr::unchecked(USER2.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    let xastro_token = base_pack.get_staking_xastro(router_ref);

    // sets 200_000_000 xASTRO tokens to user1
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user1,
        200,
    );

    // sets 200_000_000 xASTRO tokens to user2
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user2,
        200,
    );

    // locks 200 xASTRO from user1 for WEEK * 104
    base_pack
        .create_lock(router_ref, user1.clone(), WEEK * 104, 200)
        .unwrap();

    // locks 200 xASTRO from user2 for WEEK * 104
    base_pack
        .create_lock(router_ref, user2.clone(), WEEK * 104, 200)
        .unwrap();

    // going to the last week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK * 103));

    // sets 100_000_000 ASTRO tokens to distributor (simulate receive astro from maker)
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        1000,
    );

    // try set checkpoint from owner
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap();

    // check if tokens for each week equal to 9_627_288 = 1000 * 7 * 86400 / 86400 * 365 * 2
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::FeeTokensPerWeek {
                start_after: None,
                limit: Some(2),
            },
        )
        .unwrap();
    assert_eq!(vec![Uint128::new(9_627_288), Uint128::new(9_627_288)], resp);

    // claim fee for max period for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // claim fee for max period for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if user1's token balance equal to 495_845_352
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        495_805_309,
    );

    // check if user2's token balance equal to 495_845_352
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        495_805_309,
    );

    // check if distributor's token balance equal to 8_389_382 = 1000_000_000 − 495_805_309 − 495_805_309
    // 8_389_382 coin settles on the distributor for the next checkpoint.
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        8_389_382,
    );
}

#[test]
fn claim_multiple_users() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let user1 = Addr::unchecked(USER1.clone());
    let user2 = Addr::unchecked(USER2.clone());
    let user3 = Addr::unchecked(USER3.clone());
    let user4 = Addr::unchecked(USER4.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    let xastro_token = base_pack.get_staking_xastro(router_ref);

    // sets 200_000_000 xASTRO tokens to user1
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user1,
        200,
    );

    // checks if user1's xASTRO token balance is equal to 200 * 1000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user1,
        200 * MULTIPLIER as u128,
    );

    // sets 200_000_000 xASTRO tokens to user2
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user2,
        200,
    );

    // checks if user2's xASTRO token balance is equal to 200 * 1000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user2,
        200 * MULTIPLIER as u128,
    );

    // locks 100 xASTRO from user1 for WEEK * 2
    base_pack
        .create_lock(router_ref, user1.clone(), WEEK * 2, 100)
        .unwrap();

    // locks 100 xASTRO from user2 for WEEK * 2
    base_pack
        .create_lock(router_ref, user2.clone(), WEEK * 2, 100)
        .unwrap();

    // sets 100_000_000 ASTRO tokens to distributor (simulate receive astro from maker)
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100,
    );

    // checks if distributor's ASTRO token balance is equal to 100_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        100 * MULTIPLIER as u128,
    );

    // try set checkpoint from user1 when it is disabled
    let err = router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap_err();

    assert_eq!("Unauthorized", err.to_string());

    // try set checkpoint from owner
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap();

    // check if tokens are set
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
    assert_eq!(vec![Uint128::new(100_000_000)], resp);

    // check if voting supply per week is set
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::VotingSupplyPerWeek {
                start_after: None,
                limit: Some(2),
            },
        )
        .unwrap();
    assert_eq!(vec![Uint128::new(205769230), Uint128::new(102884615)], resp);

    // going to the next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // enable checkpoint for everyone
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::UpdateConfig {
                max_limit_accounts_of_claim: None,
                checkpoint_token_enabled: Some(true),
            },
            &[],
        )
        .unwrap();

    // sets 200_000_000 xASTRO tokens to user3
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user3,
        200,
    );

    // checks if user3's xASTRO token balance is equal to 200 * 1_000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user3,
        200 * MULTIPLIER as u128,
    );

    // locks 200 xASTRO from user3 for WEEK * 10
    base_pack
        .create_lock(router_ref, user3.clone(), WEEK * 10, 200)
        .unwrap();

    // check if voting supply are set
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::VotingSupplyPerWeek {
                start_after: None,
                limit: Some(2),
            },
        )
        .unwrap();
    assert_eq!(
        vec![Uint128::new(205_769_230), Uint128::new(331_730_768),],
        resp
    );

    // try to claim fee of first week for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // try to claim fee of first week for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // try to claim fee of first week for user3
    router_ref
        .execute_contract(
            user3.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if user1's token balance equal to 50_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        50_000_000,
    );

    // check if user2's token balance equal to 50_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        50_000_000,
    );

    // check if user3's token balance equal to 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user3,
        0,
    );

    // check if distributor's ASTRO balance equal to 0
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        0,
    );

    // going to next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // sets 900_000_000 ASTRO tokens to distributor (simulate receive astro from maker)
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        900,
    );

    // try set checkpoint from user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap();

    // check if tokens per week are set
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
    assert_eq!(
        vec![
            Uint128::new(100_000_000),
            Uint128::new(115_737_138),
            Uint128::new(784_262_861),
        ],
        resp
    ); // coin settles on the distributor.

    // check if voting supply are set
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::VotingSupplyPerWeek {
                start_after: None,
                limit: Some(3),
            },
        )
        .unwrap();
    assert_eq!(
        vec![
            Uint128::new(205_769_230),
            Uint128::new(331_730_768),
            Uint128::new(205_961_538),
        ],
        resp
    );

    // claim fee of second week for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if voting power for second week for user1
    let resp: VotingPowerResponse = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.voting_escrow.clone().unwrap().address,
            &VotingEscrowQueryMsg::UserVotingPowerAt {
                user: user1.to_string(),
                time: router_ref.block_info().time.seconds() - WEEK,
            },
        )
        .unwrap();
    assert_eq!(Uint128::new(51_442_307), resp.voting_power);

    // check if user1's token balance equal to 67_947_583 = 50_000_000 + 51_442_307 × 115_737_138 ÷ 331_730_768
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        67_947_642,
    );

    // claim fee of second week for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if user1's token balance equal to 67_947_583 = 50_000_000 + 51_442_307 × 115_737_138 ÷ 331_730_768
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        67_947_642,
    );

    // claim fee of second week for user3
    router_ref
        .execute_contract(
            user3.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if voting power for first week for user3
    let resp: VotingPowerResponse = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.voting_escrow.clone().unwrap().address,
            &VotingEscrowQueryMsg::UserVotingPowerAt {
                user: user3.to_string(),
                time: router_ref.block_info().time.seconds() - WEEK,
            },
        )
        .unwrap();
    assert_eq!(Uint128::new(228_846_153), resp.voting_power);

    // check if user1's token balance equal to 79_841_851 = 228_846_153 × 115_737_138 ÷ 331_730_768
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user3,
        79_841_851,
    );

    // check if distributor ASTRO balance equal to
    // 864_104_716 = 1000000000(total distributor amount) − 67_947_642(user1 fee) - 67_947_642(user2 fee) - 79_841_851(user3)
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        784_262_865,
    );

    // going to next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // sets 200_000_000 xASTRO tokens to user4
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user4,
        200,
    );

    // checks if user4's xASTRO token balance is equal to 200 * 1_000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user4,
        200 * MULTIPLIER as u128,
    );

    // locks 200 xASTRO from user4 for WEEK * 8
    base_pack
        .create_lock(router_ref, user4.clone(), WEEK * 8, 200)
        .unwrap();

    // going to next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK * 7));

    // set checkpoint for distribute fees for user3 and user4
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap();

    // check if tokens per week are set
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::FeeTokensPerWeek {
                start_after: None,
                limit: Some(3),
            },
        )
        .unwrap();
    assert_eq!(
        vec![
            Uint128::new(100_000_000),
            Uint128::new(115_737_138),
            Uint128::new(784_262_861),
        ],
        resp
    ); // coin settles on the distributor.

    // check if voting supply are set
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
    assert_eq!(
        vec![
            Uint128::new(205_769_230),
            Uint128::new(331_730_768),
            Uint128::new(205_961_538),
            Uint128::new(406_153_846),
            Uint128::new(355_384_615),
            Uint128::new(304_615_385),
            Uint128::new(253_846_154),
            Uint128::new(203_076_923),
            Uint128::new(152_307_693),
            Uint128::new(101_538_462),
        ],
        resp
    );

    // claim fee for user3
    router_ref
        .execute_contract(
            user3.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if user3's token balance equal to 864_104_712 = 79_841_851 + 784_262_865
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user3,
        864_104_712,
    );

    // claim fee for user4
    router_ref
        .execute_contract(
            user4.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if user4's token balance equal to 0,
    // there was no commission on the distributor after the third week.
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user4,
        0,
    );

    // check if distributor ASTRO balance is equal to 000_004
    // settles some coins to the next checkpoint
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        4,
    );
}

#[test]
fn test_checkpoint_total_supply() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked(OWNER.clone());
    let user1 = Addr::unchecked(USER1.clone());
    let user2 = Addr::unchecked(USER2.clone());

    let base_pack = init_astroport_test_package(router_ref).unwrap();

    let xastro_token = base_pack.get_staking_xastro(router_ref);

    // sets 200_000_000 xASTRO tokens to user1
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user1,
        200,
    );

    // checks if user1's xASTRO token balance is equal to 200 * 1000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user1,
        200 * MULTIPLIER as u128,
    );

    // sets 200_000_000 xASTRO tokens to user2
    mint(
        router_ref,
        base_pack.staking.clone().unwrap().address,
        xastro_token.clone(),
        &user2,
        200,
    );

    // checks if user2's xASTRO token balance is equal to 200 * 1000_000
    check_balance(
        router_ref,
        &xastro_token.clone(),
        &user2,
        200 * MULTIPLIER as u128,
    );

    // locks 100 xASTRO from user1 for WEEK * 104
    base_pack
        .create_lock(router_ref, user1.clone(), WEEK * 104, 100)
        .unwrap();

    // going to the next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // locks 100 xASTRO from user2 for WEEK * 104
    base_pack
        .create_lock(router_ref, user2.clone(), WEEK * 104, 100)
        .unwrap();

    // check if voting supply per week is valid
    let resp: Vec<Uint128> = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.escrow_fee_distributor.clone().unwrap().address,
            &QueryMsg::VotingSupplyPerWeek {
                start_after: None,
                limit: Some(2),
            },
        )
        .unwrap();
    assert_eq!(
        vec![
            Uint128::new(250_000_000), // first week, total vxASTRO 100 × (1+1,5×104÷104)
            Uint128::new(497_596_154), // second week, total vxASTRO 100 × (1+1,5×103÷104) + 100 × (1+1,5×104÷104)
        ],
        resp
    );

    // sets 200_000_000 ASTRO tokens to distributor (simulate receive astro from maker)
    mint(
        router_ref,
        owner.clone(),
        base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        200,
    );

    // checks if distributor's ASTRO token balance is equal to 200_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        200 * MULTIPLIER as u128,
    );

    // try set checkpoint from owner
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::CheckpointToken {},
            &[],
        )
        .unwrap();

    // check if tokens are set
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
    assert_eq!(
        vec![Uint128::new(106_871_739), Uint128::new(93_128_260)],
        resp
    );

    // try to claim fee of first week for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if user1's token balance equal to
    // claim_amount = user_vp_per_week * fee_amount_per_week / total_vp_per_week
    // 106_871_739 = 250_000_000 * 106_871_739 / 250_000_000
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        106_871_739,
    );

    // try to claim fee of first week for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if user2's token balance equal to 0, no lock no fees
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        0,
    );

    // check distributor's token balance equal to 93_128_261 = 200_000_00 - 106_871_739
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        93_128_261,
    );

    // going to the next week
    router_ref.update_block(next_block);
    router_ref.update_block(|b| b.time = b.time.plus_seconds(WEEK));

    // enable checkpoint for everyone
    router_ref
        .execute_contract(
            owner.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::UpdateConfig {
                max_limit_accounts_of_claim: None,
                checkpoint_token_enabled: Some(true),
            },
            &[],
        )
        .unwrap();

    // try to claim fee of second week for user1
    router_ref
        .execute_contract(
            user1.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // try to claim fee of second week for user2
    router_ref
        .execute_contract(
            user2.clone(),
            base_pack.escrow_fee_distributor.clone().unwrap().address,
            &ExecuteMsg::Claim { recipient: None },
            &[],
        )
        .unwrap();

    // check if user voting power of second week equal to 247596154
    let resp: VotingPowerResponse = router_ref
        .wrap()
        .query_wasm_smart(
            &base_pack.voting_escrow.clone().unwrap().address,
            &VotingEscrowQueryMsg::UserVotingPowerAt {
                user: user1.to_string(),
                time: router_ref.block_info().time.minus_seconds(WEEK).seconds(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::new(247596154), resp.voting_power);

    // check if user1's token balance equal to 153_210_921 = 106_871_739 + 247_596_154 * 100_000_000 / 497_596_154
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user1,
        153_210_921,
    );

    // check if user2's token balance equal to 46_789_077 = 250_000_000 * 93_128_261 / 497_596_154
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &user2,
        46_789_077,
    );

    // 2 coins settles on the distributor for the next checkpoint.
    check_balance(
        router_ref,
        &base_pack.astro_token.clone().unwrap().address,
        &base_pack.escrow_fee_distributor.clone().unwrap().address,
        2,
    );
}
