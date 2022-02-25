use crate::test_utils::controller_helper::ControllerHelper;
use crate::test_utils::escrow_helper::MULTIPLIER;
use crate::test_utils::{mock_app, TerraAppExtension};
use astroport_governance::generator_controller::QueryMsg;
use astroport_governance::utils::WEEK;
use cosmwasm_std::{Addr, Decimal};
use generator_controller::state::UserInfo;

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

    // TODO: query slope from escrow contract
    let ve_slope = Decimal::zero();
    let ve_power = helper
        .escrow_helper
        .query_user_vp(&mut router, "user2")
        .unwrap();
    let resp: UserInfo = router
        .wrap()
        .query_wasm_smart(
            helper.controller,
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
    assert_eq!(("pool1".to_string(), 3000), resp_votes[0]);
    assert_eq!(("pool2".to_string(), 7000), resp_votes[1]);

    router.app_next_period()
}
