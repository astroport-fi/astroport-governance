use astroport::asset::{AssetInfo, PairInfo};
use astroport::generator::PoolInfoResponse;
use cosmwasm_std::Addr;
use itertools::Itertools;
use terra_multi_test::Executor;

use astroport_governance::generator_controller::{ConfigResponse, ExecuteMsg, QueryMsg};
use astroport_governance::utils::{get_period, WEEK};
use generator_controller::state::GaugeInfo;

use crate::test_utils::controller_helper::ControllerHelper;
use crate::test_utils::escrow_helper::MULTIPLIER;
use crate::test_utils::{mock_app, TerraAppExtension};

// TODO: move this module into astroport-tests crate
#[cfg(test)]
mod test_utils;

#[test]
fn check_vote_works() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner);

    let err = helper
        .vote(&mut router, "user1", vec![("pool1", 1000)])
        .unwrap_err();
    assert_eq!(err.to_string(), "You can't vote with zero voting power");

    helper.escrow_helper.mint_xastro(&mut router, "user1", 100);
    // Create short lock
    helper
        .escrow_helper
        .create_lock(&mut router, "user1", WEEK, 100f32)
        .unwrap();
    helper
        .vote(&mut router, "user1", vec![("pool1", 1000)])
        .unwrap();

    helper.escrow_helper.mint_xastro(&mut router, "user2", 100);
    helper
        .escrow_helper
        .create_lock(&mut router, "user2", 10 * WEEK, 100f32)
        .unwrap();

    // Bps is > 10000
    let err = helper
        .vote(&mut router, "user2", vec![("pool2", 10001)])
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Basic points conversion error. 10001 > 10000"
    );

    // Bps sum is > 10000
    let err = helper
        .vote(&mut router, "user2", vec![("pool1", 3000), ("pool2", 8000)])
        .unwrap_err();
    assert_eq!(err.to_string(), "Basic points sum exceeds limit");

    // Duplicated pools
    let err = helper
        .vote(&mut router, "user2", vec![("pool1", 3000), ("pool1", 7000)])
        .unwrap_err();
    assert_eq!(err.to_string(), "Votes contain duplicated pool addresses");

    // Valid votes
    helper
        .vote(&mut router, "user2", vec![("pool1", 3000), ("pool2", 7000)])
        .unwrap();

    let err = helper
        .vote(&mut router, "user2", vec![("pool1", 7000), ("pool2", 3000)])
        .unwrap_err();
    assert_eq!(
        err.to_string(),
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
        vec![("pool1".to_string(), 3000), ("pool2".to_string(), 7000)],
        resp_votes
    );

    router.next_block(86400 * 10);
    // In 10 days user will be able to vote again
    helper
        .vote(&mut router, "user2", vec![("pool1", 500), ("pool2", 9500)])
        .unwrap();
}

#[test]
fn check_gauging() {
    let mut router = mock_app();
    let owner = "owner";
    let owner_addr = Addr::unchecked(owner);
    let helper = ControllerHelper::init(&mut router, &owner_addr);
    let user1 = "user1";
    let user2 = "user2";
    let user3 = "user3";
    let ve_locks = vec![(user1, 10), (user2, 5), (user3, 50)];

    let foo_token = helper.init_cw20_token(&mut router, "FOO").unwrap();
    let bar_token = helper.init_cw20_token(&mut router, "BAR").unwrap();
    let adn_token = helper.init_cw20_token(&mut router, "ADN").unwrap();
    let tokens = [foo_token, bar_token, adn_token];
    let pairs: Vec<_> = tokens
        .iter()
        .cartesian_product(tokens.clone())
        .filter_map(|(token1, token2)| helper.create_pool(&mut router, token1, &token2).ok())
        .collect();

    for (user, duration) in ve_locks {
        helper.escrow_helper.mint_xastro(&mut router, user, 1000);
        helper
            .escrow_helper
            .create_lock(&mut router, user, duration * WEEK, 100f32)
            .unwrap();
    }

    helper
        .vote(
            &mut router,
            user1,
            vec![(pairs[0].as_str(), 5000), (pairs[1].as_str(), 5000)],
        )
        .unwrap();
    helper
        .vote(
            &mut router,
            user2,
            vec![
                (pairs[0].as_str(), 5000),
                (pairs[1].as_str(), 2000),
                (pairs[2].as_str(), 3000),
            ],
        )
        .unwrap();
    helper
        .vote(
            &mut router,
            user3,
            vec![
                (pairs[0].as_str(), 2000),
                (pairs[1].as_str(), 3000),
                (pairs[2].as_str(), 5000),
            ],
        )
        .unwrap();

    // The contract was just created so we need to wait for 2 weeks
    let err = helper.gauge(&mut router, owner).unwrap_err();
    assert_eq!(
        err.to_string(),
        "You can only run this action every 14 days"
    );

    router.next_block(WEEK);
    let err = helper.gauge(&mut router, owner).unwrap_err();
    assert_eq!(
        err.to_string(),
        "You can only run this action every 14 days"
    );

    router.next_block(WEEK);
    let err = helper.gauge(&mut router, "somebody").unwrap_err();
    assert_eq!(err.to_string(), "Unauthorized");

    helper.gauge(&mut router, owner).unwrap();

    let resp: GaugeInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::GaugeInfo {})
        .unwrap();
    assert_eq!(get_period(resp.gauge_ts).unwrap(), router.block_period());
    assert_eq!(resp.pool_alloc_points.len(), pairs.len());
    let total_apoints: u64 = resp
        .pool_alloc_points
        .iter()
        .cloned()
        .map(|(_, apoints)| apoints.u64())
        .sum();
    assert_eq!(total_apoints, 10000);

    router.next_block(2 * WEEK);
    // Reduce pools limit 5 -> 2 (5 is initial limit in integration tests)
    let limit = 2u64;
    let err = router
        .execute_contract(
            Addr::unchecked("somebody"),
            helper.controller.clone(),
            &ExecuteMsg::ChangePoolLimit { limit },
            &[],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Unauthorized");

    router
        .execute_contract(
            owner_addr.clone(),
            helper.controller.clone(),
            &ExecuteMsg::ChangePoolLimit { limit },
            &[],
        )
        .unwrap();

    helper.gauge(&mut router, owner).unwrap();

    let resp: GaugeInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::GaugeInfo {})
        .unwrap();
    assert_eq!(get_period(resp.gauge_ts).unwrap(), router.block_period());
    assert_eq!(resp.pool_alloc_points.len(), limit as usize);
    let total_apoints: u64 = resp
        .pool_alloc_points
        .iter()
        .cloned()
        .map(|(_, apoints)| apoints.u64())
        .sum();
    assert_eq!(total_apoints, 10000);

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
    let pair_resp: PairInfo = router
        .wrap()
        .query_wasm_smart(pairs[2].clone(), &astroport::pair::QueryMsg::Pair {})
        .unwrap();
    let generator_resp: PoolInfoResponse = router
        .wrap()
        .query_wasm_smart(
            helper.generator.clone(),
            &astroport::generator::QueryMsg::PoolInfo {
                lp_token: pair_resp.liquidity_token.to_string(),
            },
        )
        .unwrap();
    assert_eq!(generator_resp.alloc_point.u64(), 0)
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
    let tokens = [&foo_token, &bar_token, &adn_token];
    let pairs: Vec<_> = tokens
        .iter()
        .cartesian_product(tokens.clone())
        .filter_map(|(&token1, token2)| helper.create_pool(&mut router, token1, &token2).ok())
        .collect();

    helper.escrow_helper.mint_xastro(&mut router, user, 1000);
    helper
        .escrow_helper
        .create_lock(&mut router, user, 10 * WEEK, 100f32)
        .unwrap();

    helper
        .vote(
            &mut router,
            user,
            vec![("random_pool", 5000), (pairs[0].as_str(), 5000)],
        )
        .unwrap();

    router.next_block(2 * WEEK);

    helper.gauge(&mut router, owner).unwrap();
    let resp: GaugeInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::GaugeInfo {})
        .unwrap();
    // There was only one valid pool during voting
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

    // Vote for deregistered pool
    helper
        .vote(&mut router, user, vec![(pairs[0].as_str(), 10000)])
        .unwrap();
    let err = helper.gauge(&mut router, owner).unwrap_err();
    assert_eq!(err.to_string(), "There are no pools to gauge");

    router.next_block(2 * WEEK);

    // Blocking FOO token so pair[0] and pair[1] become blocked as well
    let foo_asset_info = AssetInfo::Token {
        contract_addr: foo_token.clone(),
    };
    router
        .execute_contract(
            owner_addr.clone(),
            helper.generator.clone(),
            &astroport::generator::ExecuteMsg::UpdateTokensBlockedlist {
                add: Some(vec![foo_asset_info]),
                remove: None,
            },
            &[],
        )
        .unwrap();

    // Voting for 2 blocked pools and one valid pool
    helper
        .vote(
            &mut router,
            user,
            vec![
                (pairs[0].as_str(), 1000),
                (pairs[1].as_str(), 1000),
                (pairs[2].as_str(), 8000),
            ],
        )
        .unwrap();

    let resp: GaugeInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::GaugeInfo {})
        .unwrap();
    // Only one pool is eligible to receive alloc points
    assert_eq!(resp.pool_alloc_points.len(), 1);
    let total_apoints: u64 = resp
        .pool_alloc_points
        .iter()
        .cloned()
        .map(|(_, apoints)| apoints.u64())
        .sum();
    assert_eq!(total_apoints, 10000)
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
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

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
        err.to_string(),
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
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

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
