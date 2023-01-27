use astroport::asset::AssetInfo;
use astroport::generator::PoolInfoResponse;
use cosmwasm_std::{attr, Addr, Decimal, StdResult, Uint128};
use cw_multi_test::{App, ContractWrapper, Executor};
use generator_controller::astroport;
use std::str::FromStr;

use crate::astroport::asset::PairInfo;
use astroport_governance::generator_controller::{
    ConfigResponse, ExecuteMsg, QueryMsg, VOTERS_MAX_LIMIT,
};
use astroport_governance::utils::{get_period, MAX_LOCK_TIME, WEEK};
use astroport_tests::{
    controller_helper::ControllerHelper, escrow_helper::MULTIPLIER, mock_app, TerraAppExtension,
};
use generator_controller::state::TuneInfo;

#[test]
fn update_configs() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner);

    let config = helper.query_config(&mut router).unwrap();
    assert_eq!(config.blacklisted_voters_limit, None);

    // check if user2 cannot update config
    let err = helper
        .update_blacklisted_limit(&mut router, "user2", Some(4u32))
        .unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    // successful update config by owner
    helper
        .update_blacklisted_limit(&mut router, "owner", Some(4u32))
        .unwrap();

    let config = helper.query_config(&mut router).unwrap();
    assert_eq!(config.blacklisted_voters_limit, Some(4u32));
}

#[test]
fn check_kick_holders_works() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner);
    let pools = vec![
        helper
            .create_pool_with_tokens(&mut router, "FOO", "BAR")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "BAR", "ADN")
            .unwrap(),
    ];

    let err = helper
        .vote(&mut router, "user1", vec![(pools[0].as_str(), 1000)])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "You can't vote with zero voting power"
    );

    helper.escrow_helper.mint_xastro(&mut router, "user1", 100);
    // Create short lock
    helper
        .escrow_helper
        .create_lock(&mut router, "user1", WEEK, 100f32)
        .unwrap();

    // Votes from user1
    helper
        .vote(&mut router, "user1", vec![(pools[0].as_str(), 1000)])
        .unwrap();

    helper.escrow_helper.mint_xastro(&mut router, "user2", 100);
    helper
        .escrow_helper
        .create_lock(&mut router, "user2", 10 * WEEK, 100f32)
        .unwrap();

    // Votes from user2
    helper
        .vote(
            &mut router,
            "user2",
            vec![(pools[0].as_str(), 3000), (pools[1].as_str(), 7000)],
        )
        .unwrap();

    let ve_slope = helper
        .escrow_helper
        .query_lock_info(&mut router, "user2")
        .unwrap()
        .slope;
    let ve_power = helper
        .escrow_helper
        .query_user_vp(&mut router, "user2")
        .unwrap();
    let user_info = helper.query_user_info(&mut router, "user2").unwrap();
    assert_eq!(ve_slope, user_info.slope);
    assert_eq!(router.block_info().time.seconds(), user_info.vote_ts);
    assert_eq!(
        ve_power,
        user_info.voting_power.u128() as f32 / MULTIPLIER as f32
    );
    let resp_votes = user_info
        .votes
        .clone()
        .into_iter()
        .map(|(addr, bps)| (addr.to_string(), bps.into()))
        .collect::<Vec<_>>();
    assert_eq!(
        vec![(pools[0].to_string(), 3000), (pools[1].to_string(), 7000)],
        resp_votes
    );

    // Add user2 to the blacklist
    let res = helper
        .escrow_helper
        .update_blacklist(&mut router, Some(vec!["user2".to_string()]), None)
        .unwrap();
    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "update_blacklist")
    );

    // Let's take the period for which the vote was applied.
    let current_period = router.block_period() + 1u64;

    // Get pools info before kick holder
    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[0].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(13_576_922), res.slope);
    assert_eq!(Uint128::new(44_471_151), res.vxastro_amount);

    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[1].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(8_009_614), res.slope);
    assert_eq!(Uint128::new(80_096_149), res.vxastro_amount);

    // check if blacklisted voters limit exceeded for kick operation
    let err = helper
        .kick_holders(
            &mut router,
            "user1",
            vec!["user2".to_string(); (VOTERS_MAX_LIMIT + 1) as usize],
        )
        .unwrap_err();
    assert_eq!(
        "Exceeded voters limit for kick blacklisted voters operation!",
        err.root_cause().to_string()
    );

    // Removes votes for user2
    helper
        .kick_holders(&mut router, "user1", vec!["user2".to_string()])
        .unwrap();

    let ve_slope = helper
        .escrow_helper
        .query_lock_info(&mut router, "user2")
        .unwrap()
        .slope;
    let ve_power = helper
        .escrow_helper
        .query_user_vp(&mut router, "user2")
        .unwrap();

    let user_info = helper.query_user_info(&mut router, "user2").unwrap();
    assert_eq!(ve_slope, user_info.slope);
    assert_eq!(router.block_info().time.seconds(), user_info.vote_ts);
    assert_eq!(
        ve_power,
        user_info.voting_power.u128() as f32 / MULTIPLIER as f32
    );
    assert_eq!(user_info.votes, vec![]);

    // Get pool info after kick holder
    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[0].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(10_144_230), res.slope);
    assert_eq!(Uint128::new(10_144_230), res.vxastro_amount);

    let res1 = helper
        .query_voted_pool_info_at_period(&mut router, pools[1].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res1.slope);
    assert_eq!(Uint128::new(0), res1.vxastro_amount);
}

#[test]
fn check_vote_works() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner);
    let pools = vec![
        helper
            .create_pool_with_tokens(&mut router, "FOO", "BAR")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "BAR", "ADN")
            .unwrap(),
    ];

    let err = helper
        .vote(&mut router, "user1", vec![(pools[0].as_str(), 1000)])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "You can't vote with zero voting power"
    );

    helper.escrow_helper.mint_xastro(&mut router, "user1", 100);
    // Create short lock
    helper
        .escrow_helper
        .create_lock(&mut router, "user1", WEEK, 100f32)
        .unwrap();
    helper
        .vote(&mut router, "user1", vec![(pools[0].as_str(), 1000)])
        .unwrap();

    helper.escrow_helper.mint_xastro(&mut router, "user2", 100);
    helper
        .escrow_helper
        .create_lock(&mut router, "user2", 10 * WEEK, 100f32)
        .unwrap();

    // Bps is > 10000
    let err = helper
        .vote(&mut router, "user2", vec![(pools[1].as_str(), 10001)])
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Basic points conversion error. 10001 > 10000"
    );

    // Bps sum is > 10000
    let err = helper
        .vote(
            &mut router,
            "user2",
            vec![(pools[0].as_str(), 3000), (pools[1].as_str(), 8000)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Basic points sum exceeds limit"
    );

    // Duplicated pools
    let err = helper
        .vote(
            &mut router,
            "user2",
            vec![(pools[0].as_str(), 3000), (pools[0].as_str(), 7000)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Votes contain duplicated pool addresses"
    );

    // Valid votes
    helper
        .vote(
            &mut router,
            "user2",
            vec![(pools[0].as_str(), 3000), (pools[1].as_str(), 7000)],
        )
        .unwrap();

    let err = helper
        .vote(
            &mut router,
            "user2",
            vec![(pools[0].as_str(), 7000), (pools[1].as_str(), 3000)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "You can only run this action every 10 days"
    );

    let ve_slope = helper
        .escrow_helper
        .query_lock_info(&mut router, "user2")
        .unwrap()
        .slope;
    let ve_power = helper
        .escrow_helper
        .query_user_vp(&mut router, "user2")
        .unwrap();
    let user_info = helper.query_user_info(&mut router, "user2").unwrap();
    assert_eq!(ve_slope, user_info.slope);
    assert_eq!(router.block_info().time.seconds(), user_info.vote_ts);
    assert_eq!(
        ve_power,
        user_info.voting_power.u128() as f32 / MULTIPLIER as f32
    );
    let resp_votes = user_info
        .votes
        .into_iter()
        .map(|(addr, bps)| (addr.to_string(), bps.into()))
        .collect::<Vec<_>>();
    assert_eq!(
        vec![(pools[0].to_string(), 3000), (pools[1].to_string(), 7000)],
        resp_votes
    );

    router.next_block(86400 * 10);
    // In 10 days user will be able to vote again
    helper
        .vote(
            &mut router,
            "user2",
            vec![(pools[0].as_str(), 500), (pools[1].as_str(), 9500)],
        )
        .unwrap();
}

fn create_unregistered_pool(
    router: &mut App,
    helper: &mut ControllerHelper,
) -> StdResult<PairInfo> {
    let pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    );

    let pair_code_id = router.store_code(pair_contract);

    let test_token1 = helper.init_cw20_token(router, "TST").unwrap();
    let test_token2 = helper.init_cw20_token(router, "TSB").unwrap();

    let pair_addr = router
        .instantiate_contract(
            pair_code_id,
            Addr::unchecked("owner"),
            &astroport::pair::InstantiateMsg {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: test_token1.clone(),
                    },
                    AssetInfo::Token {
                        contract_addr: test_token2.clone(),
                    },
                ],
                token_code_id: 1,
                factory_addr: helper.factory.to_string(),
                init_params: None,
            },
            &[],
            "Unregistered pair".to_string(),
            None,
        )
        .unwrap();

    let res: PairInfo = router
        .wrap()
        .query_wasm_smart(pair_addr, &astroport::pair::QueryMsg::Pair {})?;

    Ok(res)
}

#[test]
fn check_tuning() {
    let mut router = mock_app();
    let owner = "owner";
    let owner_addr = Addr::unchecked(owner);
    let mut helper = ControllerHelper::init(&mut router, &owner_addr);
    let user1 = "user1";
    let user2 = "user2";
    let user3 = "user3";
    let ve_locks = vec![(user1, 10), (user2, 5), (user3, 50)];

    let pools = vec![
        helper
            .create_pool_with_tokens(&mut router, "FOO", "BAR")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "BAR", "ADN")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "FOO", "ADN")
            .unwrap(),
    ];

    for (user, duration) in ve_locks {
        helper.escrow_helper.mint_xastro(&mut router, user, 1000);
        helper
            .escrow_helper
            .create_lock(&mut router, user, duration * WEEK, 100f32)
            .unwrap();
    }

    let res = create_unregistered_pool(&mut router, &mut helper).unwrap();
    let err = helper
        .vote(
            &mut router,
            user1,
            vec![
                (pools[0].as_str(), 5000),
                (pools[1].as_str(), 4000),
                (res.liquidity_token.as_str(), 1000),
            ],
        )
        .unwrap_err();
    assert_eq!(
        "Generic error: The pair aren't registered: contract20-contract21",
        err.root_cause().to_string()
    );

    helper
        .vote(
            &mut router,
            user1,
            vec![(pools[0].as_str(), 5000), (pools[1].as_str(), 5000)],
        )
        .unwrap();
    helper
        .vote(
            &mut router,
            user2,
            vec![
                (pools[0].as_str(), 5000),
                (pools[1].as_str(), 2000),
                (pools[2].as_str(), 3000),
            ],
        )
        .unwrap();
    helper
        .vote(
            &mut router,
            user3,
            vec![
                (pools[0].as_str(), 2000),
                (pools[1].as_str(), 3000),
                (pools[2].as_str(), 5000),
            ],
        )
        .unwrap();

    // The contract was just created so we need to wait for 2 weeks
    let err = helper.tune(&mut router).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "You can only run this action every 14 days"
    );

    router.next_block(WEEK);
    let err = helper.tune(&mut router).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "You can only run this action every 14 days"
    );

    router.next_block(WEEK);

    helper.tune(&mut router).unwrap();

    let resp: TuneInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::TuneInfo {})
        .unwrap();
    assert_eq!(get_period(resp.tune_ts).unwrap(), router.block_period());
    assert_eq!(resp.pool_alloc_points.len(), pools.len());
    let total_apoints: u128 = resp
        .pool_alloc_points
        .iter()
        .cloned()
        .map(|(_, apoints)| apoints.u128())
        .sum();
    assert_eq!(total_apoints, 357423036);

    router.next_block(2 * WEEK);
    // Reduce pools limit 5 -> 2 (5 is initial limit in integration tests)
    let limit = 2u64;
    let err = router
        .execute_contract(
            Addr::unchecked("somebody"),
            helper.controller.clone(),
            &ExecuteMsg::ChangePoolsLimit { limit },
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    router
        .execute_contract(
            owner_addr.clone(),
            helper.controller.clone(),
            &ExecuteMsg::ChangePoolsLimit { limit },
            &[],
        )
        .unwrap();

    let err = router
        .execute_contract(
            owner_addr.clone(),
            helper.controller.clone(),
            &ExecuteMsg::ChangePoolsLimit { limit: 101 },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Invalid pool number: 101. Must be within [2, 100] range"
    );

    helper.tune(&mut router).unwrap();

    let resp: TuneInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::TuneInfo {})
        .unwrap();
    assert_eq!(get_period(resp.tune_ts).unwrap(), router.block_period());
    assert_eq!(resp.pool_alloc_points.len(), limit as usize);
    let total_apoints: u128 = resp
        .pool_alloc_points
        .iter()
        .cloned()
        .map(|(_, apoints)| apoints.u128())
        .sum();
    assert_eq!(total_apoints, 191009600);

    // Check alloc points are properly set in generator
    for (pool_addr, apoints) in resp.pool_alloc_points {
        let resp: PoolInfoResponse = router
            .wrap()
            .query_wasm_smart(
                helper.generator.clone(),
                &astroport::generator::QueryMsg::PoolInfo {
                    lp_token: pool_addr.to_string(),
                },
            )
            .unwrap();
        assert_eq!(apoints, resp.alloc_point)
    }

    // Check the last pool did not receive alloc points
    let generator_resp: PoolInfoResponse = router
        .wrap()
        .query_wasm_smart(
            helper.generator.clone(),
            &astroport::generator::QueryMsg::PoolInfo {
                lp_token: pools[2].to_string(),
            },
        )
        .unwrap();
    assert_eq!(generator_resp.alloc_point.u128(), 0)
}

#[test]
fn check_bad_pools_filtering() {
    let mut router = mock_app();
    let owner = "owner";
    let owner_addr = Addr::unchecked(owner);
    let helper = ControllerHelper::init(&mut router, &owner_addr);
    let user = "user1";

    let foo_token = helper.init_cw20_token(&mut router, "FOO").unwrap();
    let bar_token = helper.init_cw20_token(&mut router, "BAR").unwrap();
    let adn_token = helper.init_cw20_token(&mut router, "ADN").unwrap();
    let pools = vec![
        helper
            .create_pool(&mut router, &foo_token, &bar_token)
            .unwrap(),
        helper
            .create_pool(&mut router, &foo_token, &adn_token)
            .unwrap(),
        helper
            .create_pool(&mut router, &bar_token, &adn_token)
            .unwrap(),
    ];

    helper.escrow_helper.mint_xastro(&mut router, user, 1000);
    helper
        .escrow_helper
        .create_lock(&mut router, user, 10 * WEEK, 100f32)
        .unwrap();

    let err = helper
        .vote(
            &mut router,
            user,
            vec![("random_pool", 5000), (pools[0].as_str(), 5000)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Invalid lp token address: random_pool"
    );
    helper
        .vote(&mut router, user, vec![(pools[0].as_str(), 5000)])
        .unwrap();

    router.next_block(2 * WEEK);

    helper.tune(&mut router).unwrap();
    let resp: TuneInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::TuneInfo {})
        .unwrap();
    // There was only one valid pool
    assert_eq!(resp.pool_alloc_points.len(), 1);

    router.next_block(2 * WEEK);

    // Deregister first pair
    let asset_infos = [
        AssetInfo::Token {
            contract_addr: foo_token.clone(),
        },
        AssetInfo::Token {
            contract_addr: bar_token.clone(),
        },
    ];
    router
        .execute_contract(
            owner_addr.clone(),
            helper.factory.clone(),
            &astroport::factory::ExecuteMsg::Deregister { asset_infos },
            &[],
        )
        .unwrap();

    // We cannot vote for deregistered pool
    let err = helper
        .vote(&mut router, user, vec![(pools[0].as_str(), 10000)])
        .unwrap_err();
    assert_eq!(
        "Generic error: The pair aren't registered: contract8-contract9",
        err.root_cause().to_string()
    );

    let err = helper.tune(&mut router).unwrap_err();
    assert_eq!(err.root_cause().to_string(), "There are no pools to tune");

    router.next_block(2 * WEEK);

    // Blocking FOO token so pair[0] and pair[1] become blocked as well
    let foo_asset_info = AssetInfo::Token {
        contract_addr: foo_token.clone(),
    };
    router
        .execute_contract(
            owner_addr.clone(),
            helper.generator.clone(),
            &astroport::generator::ExecuteMsg::UpdateBlockedTokenslist {
                add: Some(vec![foo_asset_info]),
                remove: None,
            },
            &[],
        )
        .unwrap();

    // Voting for 2 valid pools
    helper
        .vote(
            &mut router,
            user,
            vec![(pools[1].as_str(), 1000), (pools[2].as_str(), 8000)],
        )
        .unwrap();

    router.next_block(WEEK);
    helper.tune(&mut router).unwrap();

    let resp: TuneInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::TuneInfo {})
        .unwrap();
    // Only one pool is eligible to receive alloc points
    assert_eq!(resp.pool_alloc_points.len(), 1);
    let total_apoints: u128 = resp
        .pool_alloc_points
        .iter()
        .cloned()
        .map(|(_, apoints)| apoints.u128())
        .sum();
    assert_eq!(total_apoints, 36615382)
}

#[test]
fn check_update_owner() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut app, &owner);

    let new_owner = String::from("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        new_owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // Unauthed check
    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            helper.controller.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.controller.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    app.execute_contract(
        Addr::unchecked("owner"),
        helper.controller.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // Claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.controller.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        helper.controller.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // Let's query the contract state
    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&helper.controller, &msg)
        .unwrap();

    assert_eq!(res.owner, new_owner)
}

#[test]
fn check_main_pool() {
    let mut router = mock_app();
    let owner_addr = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner_addr);
    let pools = vec![
        helper
            .create_pool_with_tokens(&mut router, "FOO", "BAR")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "BAR", "ADN")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "FOO", "ADN")
            .unwrap(),
    ];

    for user in ["user1", "user2"] {
        helper.escrow_helper.mint_xastro(&mut router, user, 100);
        helper
            .escrow_helper
            .create_lock(&mut router, user, MAX_LOCK_TIME, 100f32)
            .unwrap();
    }
    helper
        .vote(
            &mut router,
            "user1",
            vec![
                (pools[0].as_str(), 1000),
                (pools[1].as_str(), 5000),
                (pools[2].as_str(), 4000),
            ],
        )
        .unwrap();
    let block_period = router.block_period();
    let main_pool_info = helper
        .query_voted_pool_info_at_period(&mut router, pools[0].as_str(), block_period + 2)
        .unwrap();
    assert_eq!(main_pool_info.vxastro_amount.u128(), 24759614);

    let err = helper
        .update_main_pool(
            &mut router,
            "owner",
            Some(&pools[0]),
            Some(Decimal::zero()),
            false,
        )
        .unwrap_err();
    assert_eq!(
        &err.root_cause().to_string(),
        "main_pool_min_alloc should be more than 0 and less than 1"
    );
    let err = helper
        .update_main_pool(
            &mut router,
            "owner",
            Some(&pools[0]),
            Some(Decimal::one()),
            false,
        )
        .unwrap_err();
    assert_eq!(
        &err.root_cause().to_string(),
        "main_pool_min_alloc should be more than 0 and less than 1"
    );
    helper
        .update_main_pool(
            &mut router,
            "owner",
            Some(&pools[0]),
            Decimal::from_str("0.3").ok(),
            false,
        )
        .unwrap();

    // From now users can't vote for the main pool
    let err = helper
        .vote(
            &mut router,
            "user2",
            vec![(pools[0].as_str(), 1000), (pools[1].as_str(), 9000)],
        )
        .unwrap_err();
    assert_eq!(
        &err.root_cause().to_string(),
        "contract11 is the main pool. Voting for the main pool is prohibited"
    );

    router
        .execute_contract(
            owner_addr.clone(),
            helper.controller.clone(),
            &ExecuteMsg::ChangePoolsLimit { limit: 2 },
            &[],
        )
        .unwrap();

    router.next_block(2 * WEEK);
    helper.tune(&mut router).unwrap();

    let resp: TuneInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::TuneInfo {})
        .unwrap();
    // 2 (limit) + 1 (main pool)
    assert_eq!(resp.pool_alloc_points.len(), 3 as usize);
    let total_apoints: Uint128 = resp
        .pool_alloc_points
        .iter()
        .map(|(_, apoints)| apoints)
        .sum();
    assert_eq!(total_apoints.u128(), 318337891);
    let main_pool_contribution = resp
        .pool_alloc_points
        .iter()
        .find(|(pool, _)| pool == &pools[0]);
    assert_eq!(
        main_pool_contribution.unwrap().1,
        (total_apoints * Decimal::from_str("0.3").unwrap())
    );

    // Remove the main pool
    helper
        .update_main_pool(&mut router, "owner", None, None, true)
        .unwrap();

    router.next_block(2 * WEEK);
    helper.tune(&mut router).unwrap();

    let resp: TuneInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::TuneInfo {})
        .unwrap();
    // The main pool was removed
    assert_eq!(resp.pool_alloc_points.len(), 2 as usize);
}
