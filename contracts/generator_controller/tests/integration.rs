use cosmwasm_std::Addr;
use terra_multi_test::Executor;

use astroport_governance::generator_controller::{ExecuteMsg, QueryMsg};
use astroport_governance::utils::{get_period, WEEK};
use generator_controller::state::{GaugeInfo, UserInfo};

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
        .create_lock(&mut router, "user1", 1 * WEEK, 100f32)
        .unwrap();
    let err = helper
        .vote(&mut router, "user1", vec![("pool1", 1000)])
        .unwrap_err();
    assert_eq!(err.to_string(), "Your lock will expire in less than a week");

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
    let resp: UserInfo = router
        .wrap()
        .query_wasm_smart(
            helper.controller.clone(),
            &QueryMsg::UserInfo {
                user: "user2".to_string(),
            },
        )
        .unwrap();
    assert_eq!(ve_slope, resp.slope);
    assert_eq!(router.block_info().time.seconds(), resp.vote_ts);
    assert_eq!(
        ve_power,
        resp.voting_power.u128() as f32 / MULTIPLIER as f32
    );
    let resp_votes = resp
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

    for (user, duration) in ve_locks {
        helper.escrow_helper.mint_xastro(&mut router, user, 1000);
        helper
            .escrow_helper
            .create_lock(&mut router, user, duration * WEEK, 100f32)
            .unwrap();
    }

    helper
        .vote(&mut router, user1, vec![("pool1", 5000), ("pool2", 5000)])
        .unwrap();
    helper
        .vote(
            &mut router,
            user2,
            vec![("pool1", 5000), ("pool3", 2000), ("pool5", 3000)],
        )
        .unwrap();
    helper
        .vote(
            &mut router,
            user3,
            vec![("pool2", 2000), ("pool3", 3000), ("pool4", 5000)],
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
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::GaugeInfo)
        .unwrap();
    assert_eq!(get_period(resp.gauge_ts), router.block_period());
    assert_eq!(resp.pool_alloc_points.len(), 5);
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
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::GaugeInfo)
        .unwrap();
    assert_eq!(get_period(resp.gauge_ts), router.block_period());
    assert_eq!(resp.pool_alloc_points.len(), limit as usize);
    let total_apoints: u64 = resp
        .pool_alloc_points
        .iter()
        .cloned()
        .map(|(_, apoints)| apoints.u64())
        .sum();
    assert_eq!(total_apoints, 10000)
}
