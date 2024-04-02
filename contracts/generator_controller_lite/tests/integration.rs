use astroport::asset::AssetInfo;
use astroport::generator::PoolInfoResponse;
use cosmwasm_std::{attr, Addr, Decimal, StdResult, Uint128};
use cw_multi_test::{App, ContractWrapper, Executor};
use generator_controller_lite::astroport;
use std::str::FromStr;

use crate::astroport::asset::PairInfo;
use astroport_governance::generator_controller_lite::{
    ConfigResponse, ExecuteMsg, NetworkInfo, QueryMsg, VOTERS_MAX_LIMIT,
};
use astroport_governance::utils::{get_lite_period, LITE_VOTING_PERIOD, MAX_LOCK_TIME, WEEK};
use astroport_tests_lite::{
    controller_helper::ControllerHelper, escrow_helper::MULTIPLIER, mock_app, TerraAppExtension,
};
use generator_controller_lite::state::TuneInfo;

#[test]
fn update_configs() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner, None);

    let config = helper.query_config(&mut router).unwrap();
    assert_eq!(config.kick_voters_limit, None);

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
    assert_eq!(config.kick_voters_limit, Some(4u32));
}

#[test]
fn check_kick_holders_works() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner, None);
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

    helper.escrow_helper.mint_xastro(&mut router, "owner", 100);
    helper.escrow_helper.mint_xastro(&mut router, "user1", 100);
    // Create short lock
    helper
        .escrow_helper
        .create_lock(&mut router, "user1", WEEK, 100f32)
        .unwrap();

    helper
        .update_whitelist(
            &mut router,
            "owner",
            Some(pools.iter().map(|el| el.to_string()).collect()),
            None,
        )
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

    let ve_power = helper
        .escrow_helper
        .query_user_emissions_vp(&mut router, "user2")
        .unwrap();
    let user_info = helper.query_user_info(&mut router, "user2").unwrap();
    assert_eq!(
        get_lite_period(router.block_info().time.seconds()).unwrap(),
        user_info.vote_period.unwrap()
    );
    assert_eq!(
        ve_power,
        user_info.voting_power.u128() as f32 / MULTIPLIER as f32
    );
    let resp_votes = user_info
        .votes
        .into_iter()
        .map(|(addr, bps)| (addr, bps.into()))
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
    assert_eq!(Uint128::new(0), res.slope);
    assert_eq!(Uint128::new(40_000_000), res.vxastro_amount);

    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[1].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res.slope);
    assert_eq!(Uint128::new(70_000_000), res.vxastro_amount);

    // check if blacklisted voters limit exceeded for kick operation
    let err = helper
        .kick_holders(
            &mut router,
            "user1",
            vec!["user2".to_string(); (VOTERS_MAX_LIMIT + 1) as usize],
        )
        .unwrap_err();
    assert_eq!(
        "Exceeded voters limit for kick blacklisted/unlocked voters operation!",
        err.root_cause().to_string()
    );

    // Removes votes for user2
    helper
        .kick_holders(&mut router, "user1", vec!["user2".to_string()])
        .unwrap();

    let ve_power = helper
        .escrow_helper
        .query_user_vp(&mut router, "user2")
        .unwrap();

    let user_info = helper.query_user_info(&mut router, "user2").unwrap();
    assert_eq!(
        get_lite_period(router.block_info().time.seconds()).unwrap(),
        user_info.vote_period.unwrap()
    );
    assert_eq!(
        ve_power,
        user_info.voting_power.u128() as f32 / MULTIPLIER as f32
    );
    assert_eq!(user_info.votes, vec![]);

    // Get pool info after kick holder
    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[0].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res.slope);
    assert_eq!(Uint128::new(10_000_000), res.vxastro_amount);

    let res1 = helper
        .query_voted_pool_info_at_period(&mut router, pools[1].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res1.slope);
    assert_eq!(Uint128::new(0), res1.vxastro_amount);
}

#[test]
fn check_kick_unlocked_holders_works() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner, None);
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

    helper.escrow_helper.mint_xastro(&mut router, "owner", 100);
    helper.escrow_helper.mint_xastro(&mut router, "user1", 100);
    // Create short lock
    helper
        .escrow_helper
        .create_lock(&mut router, "user1", WEEK, 100f32)
        .unwrap();

    helper
        .update_whitelist(
            &mut router,
            "owner",
            Some(pools.iter().map(|el| el.to_string()).collect()),
            None,
        )
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

    let ve_power = helper
        .escrow_helper
        .query_user_emissions_vp(&mut router, "user2")
        .unwrap();

    let user_info = helper.query_user_info(&mut router, "user2").unwrap();
    assert_eq!(
        get_lite_period(router.block_info().time.seconds()).unwrap(),
        user_info.vote_period.unwrap()
    );
    assert_eq!(
        ve_power,
        user_info.voting_power.u128() as f32 / MULTIPLIER as f32
    );

    let resp_votes = user_info
        .votes
        .into_iter()
        .map(|(addr, bps)| (addr, bps.into()))
        .collect::<Vec<_>>();
    assert_eq!(
        vec![(pools[0].to_string(), 3000), (pools[1].to_string(), 7000)],
        resp_votes
    );

    // Let's take the period for which the vote was applied.
    let current_period = router.block_period() + 1u64;
    // Get pools info before kick holder
    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[0].as_str(), current_period)
        .unwrap();

    // We should see 40_000_000 as the vxASTRO amount here because:
    // User1 voted with 10% of the 100_000_000 total voting power
    // User2 voted with 30% of the 100_000_000 total voting power
    // Total voting power is 40_000_000
    assert_eq!(Uint128::new(0), res.slope);
    assert_eq!(Uint128::new(40_000_000), res.vxastro_amount);

    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[1].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res.slope);
    assert_eq!(Uint128::new(70_000_000), res.vxastro_amount);

    // Unlock user2, which results in an immediate kick
    helper.escrow_helper.unlock(&mut router, "user2").unwrap();

    // check if blacklisted voters limit exceeded for kick operation
    let err = helper
        .kick_unlocked_holders(
            &mut router,
            "user1",
            vec!["user2".to_string(); (VOTERS_MAX_LIMIT + 1) as usize],
        )
        .unwrap_err();
    assert_eq!(
        "Exceeded voters limit for kick blacklisted/unlocked voters operation!",
        err.root_cause().to_string()
    );

    // Removes votes for user2
    // Not strictly needed as the user is kicked immediately when unlock starts
    helper
        .kick_unlocked_holders(&mut router, "user1", vec!["user2".to_string()])
        .unwrap();

    let ve_power = helper
        .escrow_helper
        .query_user_vp(&mut router, "user2")
        .unwrap();

    let user_info = helper.query_user_info(&mut router, "user2").unwrap();
    assert_eq!(
        get_lite_period(router.block_info().time.seconds()).unwrap(),
        user_info.vote_period.unwrap()
    );
    assert_eq!(
        ve_power,
        user_info.voting_power.u128() as f32 / MULTIPLIER as f32
    );
    // All votes should be removed for this user
    assert_eq!(user_info.votes, vec![]);

    // Get pool info after kick holder
    // We should see 10_000_000 as the vxASTRO amount here because:
    // User1 voted with 10% of the 100_000_000 total voting power
    // User2 voted with 30% of the 100_000_000 total voting power
    // User2 was kicked removing the 30% of the 100_000_000 total voting power
    // Total voting power is now 10_000_000
    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[0].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res.slope);
    assert_eq!(Uint128::new(10_000_000), res.vxastro_amount);

    let res1 = helper
        .query_voted_pool_info_at_period(&mut router, pools[1].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res1.slope);
    assert_eq!(Uint128::new(0), res1.vxastro_amount);
}

#[test]
fn check_kick_unlocked_outpost_holders_works() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner, Some("hub".to_string()));
    let pools = vec![
        helper
            .create_pool_with_tokens(&mut router, "FOO", "BAR")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "BAR", "ADN")
            .unwrap(),
    ];

    let voter1 = "outpost_voter1".to_string();
    let voter1_power = Uint128::from(50_000_000u64);
    let voter2 = "outpost_voter2".to_string();
    let voter2_power = Uint128::from(100_000_000u64);

    helper
        .update_whitelist(
            &mut router,
            "owner",
            Some(pools.iter().map(|el| el.to_string()).collect()),
            None,
        )
        .unwrap();

    helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            voter1_power,
            vec![(pools[0].as_str(), 8000), (pools[1].as_str(), 1000)],
        )
        .unwrap();

    // Votes from user2
    helper
        .outpost_vote(
            &mut router,
            "hub",
            voter2,
            voter2_power,
            vec![(pools[0].as_str(), 2000)],
        )
        .unwrap();

    let user_info = helper
        .query_user_info(&mut router, voter1.as_ref())
        .unwrap();
    assert_eq!(
        get_lite_period(router.block_info().time.seconds()).unwrap(),
        user_info.vote_period.unwrap()
    );

    let resp_votes = user_info
        .votes
        .into_iter()
        .map(|(addr, bps)| (addr, bps.into()))
        .collect::<Vec<_>>();
    assert_eq!(
        vec![(pools[0].to_string(), 8000), (pools[1].to_string(), 1000)],
        resp_votes
    );

    // Let's take the period for which the vote was applied.
    let current_period = router.block_period() + 1u64;

    // Get pools info before kick holder
    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[0].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res.slope);
    assert_eq!(Uint128::new(60_000_000), res.vxastro_amount);

    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[1].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res.slope);
    assert_eq!(Uint128::new(5_000_000), res.vxastro_amount);

    // Check that only Hub can call this
    let err = helper
        .kick_unlocked_outpost_holders(&mut router, "not_hub", voter1.to_string())
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    helper
        .kick_unlocked_outpost_holders(&mut router, "hub", voter1.to_string())
        .unwrap();

    let user_info = helper
        .query_user_info(&mut router, voter1.as_ref())
        .unwrap();
    assert_eq!(
        get_lite_period(router.block_info().time.seconds()).unwrap(),
        user_info.vote_period.unwrap()
    );

    // Get pool info after kick holder
    let res = helper
        .query_voted_pool_info_at_period(&mut router, pools[0].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res.slope);
    assert_eq!(Uint128::new(20_000_000), res.vxastro_amount);

    // Since Outpost user 1 was kicked, their voting power should be removed for pools[1]
    let res1 = helper
        .query_voted_pool_info_at_period(&mut router, pools[1].as_str(), current_period)
        .unwrap();
    assert_eq!(Uint128::new(0), res1.slope);
    assert_eq!(Uint128::new(0), res1.vxastro_amount);
}

#[test]
fn check_kick_unlocked_outpost_holders_unauthorized() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner, Some("hub".to_string()));
    let pools = vec![
        helper
            .create_pool_with_tokens(&mut router, "FOO", "BAR")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "BAR", "ADN")
            .unwrap(),
    ];

    let voter1 = "outpost_voter1".to_string();
    let voter1_power = Uint128::from(50_000_000u64);

    helper
        .update_whitelist(
            &mut router,
            "owner",
            Some(pools.iter().map(|el| el.to_string()).collect()),
            None,
        )
        .unwrap();

    helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            voter1_power,
            vec![(pools[0].as_str(), 8000), (pools[1].as_str(), 1000)],
        )
        .unwrap();

    // Check that only Hub can call this
    let err = helper
        .kick_unlocked_outpost_holders(&mut router, "not_hub", voter1)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");
}

#[test]
fn check_vote_works() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner, None);
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

    helper.escrow_helper.mint_xastro(&mut router, "owner", 100);
    helper.escrow_helper.mint_xastro(&mut router, "user1", 100);
    // Create short lock
    helper
        .escrow_helper
        .create_lock(&mut router, "user1", WEEK, 100f32)
        .unwrap();
    let err = helper
        .vote(&mut router, "user1", vec![(pools[0].as_str(), 1000)])
        .unwrap_err();
    assert_eq!("Whitelist cannot be empty!", err.root_cause().to_string());

    let err = helper
        .update_whitelist(&mut router, "user1", Some(vec![pools[0].to_string()]), None)
        .unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    helper
        .update_whitelist(
            &mut router,
            "owner",
            Some(pools.iter().map(|el| el.to_string()).collect()),
            None,
        )
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
        "You can only run this action once in a voting period"
    );

    let ve_power = helper
        .escrow_helper
        .query_user_emissions_vp(&mut router, "user2")
        .unwrap();
    let user_info = helper.query_user_info(&mut router, "user2").unwrap();
    assert_eq!(
        get_lite_period(router.block_info().time.seconds()).unwrap(),
        user_info.vote_period.unwrap()
    );
    assert_eq!(
        ve_power,
        user_info.voting_power.u128() as f32 / MULTIPLIER as f32
    );
    let resp_votes = user_info
        .votes
        .into_iter()
        .map(|(addr, bps)| (addr, bps.into()))
        .collect::<Vec<_>>();
    assert_eq!(
        vec![(pools[0].to_string(), 3000), (pools[1].to_string(), 7000)],
        resp_votes
    );

    router.next_block(LITE_VOTING_PERIOD);
    // In the next period the user will be able to vote again
    helper
        .vote(
            &mut router,
            "user2",
            vec![(pools[0].as_str(), 500), (pools[1].as_str(), 9500)],
        )
        .unwrap();
}

#[test]
fn check_outpost_vote_no_hub() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner, None);
    let pools = vec![
        helper
            .create_pool_with_tokens(&mut router, "FOO", "BAR")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "BAR", "ADN")
            .unwrap(),
    ];

    let voter1 = "voter1".to_string();
    let voter1_power = Uint128::from(100_000u64);

    let err = helper
        .outpost_vote(
            &mut router,
            "not_hub",
            voter1,
            voter1_power,
            vec![(pools[0].as_str(), 1000)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Sender is not the Hub installed"
    );
}

#[test]
fn check_outpost_vote_unauthorised() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner, Some("hub".to_string()));
    let pools = vec![
        helper
            .create_pool_with_tokens(&mut router, "FOO", "BAR")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "BAR", "ADN")
            .unwrap(),
    ];

    let voter1 = "voter1".to_string();
    let voter1_power = Uint128::from(100_000u64);

    let err = helper
        .outpost_vote(
            &mut router,
            "not_hub",
            voter1,
            voter1_power,
            vec![(pools[0].as_str(), 1000)],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");
}

#[test]
fn check_outpost_vote_works() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner, Some("hub".to_string()));
    let pools = vec![
        helper
            .create_pool_with_tokens(&mut router, "FOO", "BAR")
            .unwrap(),
        helper
            .create_pool_with_tokens(&mut router, "BAR", "ADN")
            .unwrap(),
    ];

    let voter1 = "voter1".to_string();
    let voter1_power = Uint128::from(100_000u64);

    let err = helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            Uint128::zero(),
            vec![(pools[0].as_str(), 1000)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "You can't vote with zero voting power"
    );

    let err = helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            voter1_power,
            vec![(pools[0].as_str(), 1000)],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Whitelist cannot be empty!");

    helper
        .update_whitelist(
            &mut router,
            "owner",
            Some(pools.iter().map(|el| el.to_string()).collect()),
            None,
        )
        .unwrap();

    // Bps is > 10000
    let err = helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            voter1_power,
            vec![(pools[0].as_str(), 10001)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Basic points conversion error. 10001 > 10000"
    );

    let err = helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            voter1_power,
            vec![(pools[0].as_str(), 3000), (pools[1].as_str(), 8000)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Basic points sum exceeds limit"
    );

    let err = helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            voter1_power,
            vec![(pools[0].as_str(), 3000), (pools[0].as_str(), 7000)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Votes contain duplicated pool addresses"
    );

    // Valid votes
    helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            voter1_power,
            vec![(pools[0].as_str(), 3000), (pools[1].as_str(), 7000)],
        )
        .unwrap();

    let err = helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            voter1_power,
            vec![(pools[0].as_str(), 3000), (pools[1].as_str(), 7000)],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "You can only run this action once in a voting period"
    );

    let user_info = helper
        .query_user_info(&mut router, voter1.as_ref())
        .unwrap();
    assert_eq!(
        get_lite_period(router.block_info().time.seconds()).unwrap(),
        user_info.vote_period.unwrap()
    );

    let resp_votes = user_info
        .votes
        .into_iter()
        .map(|(addr, bps)| (addr, bps.into()))
        .collect::<Vec<_>>();
    assert_eq!(
        vec![(pools[0].to_string(), 3000), (pools[1].to_string(), 7000)],
        resp_votes
    );

    router.next_block(LITE_VOTING_PERIOD);
    // In the next period the user will be able to vote again
    helper
        .outpost_vote(
            &mut router,
            "hub",
            voter1.clone(),
            voter1_power,
            vec![(pools[0].as_str(), 3000), (pools[1].as_str(), 7000)],
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
                asset_infos: vec![
                    AssetInfo::Token {
                        contract_addr: test_token1,
                    },
                    AssetInfo::Token {
                        contract_addr: test_token2,
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
    let mut helper = ControllerHelper::init(&mut router, &owner_addr, None);
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

    helper
        .update_whitelist(
            &mut router,
            "owner",
            Some(pools.iter().map(|el| el.to_string()).collect()),
            None,
        )
        .unwrap();

    let err = helper
        .update_whitelist(&mut router, "owner", Some(vec![pools[0].to_string()]), None)
        .unwrap_err();
    assert_eq!("Generic error: The resulting whitelist contains duplicated pools. It's either provided 'add' list contains duplicated pools or some of the added pools are already whitelisted.", err.root_cause().to_string());

    let config_resp = helper.query_config(&mut router).unwrap();
    assert_eq!(config_resp.whitelisted_pools, pools);

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
        "Pool is not whitelisted: wasm1contract25",
        err.root_cause().to_string()
    );

    let err = helper
        .vote(
            &mut router,
            user1,
            vec![
                (pools[0].as_str(), 5000),
                (pools[1].as_str(), 2000),
                (pools[1].as_str(), 2000),
            ],
        )
        .unwrap_err();
    assert_eq!(
        "Votes contain duplicated pool addresses",
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

    // The contract was just created so we need to wait for the next period
    let err = helper.tune(&mut router).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "You can only run this action once in a voting period"
    );

    // Periods are two weeks, so this should fail as well
    router.next_block(WEEK);
    let err = helper.tune(&mut router).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "You can only run this action once in a voting period"
    );

    // This should now be the next period
    router.next_block(WEEK);
    helper.tune(&mut router).unwrap();

    let resp: TuneInfo = router
        .wrap()
        .query_wasm_smart(helper.controller.clone(), &QueryMsg::TuneInfo {})
        .unwrap();
    assert_eq!(resp.tune_period, router.block_period());
    assert_eq!(resp.pool_alloc_points.len(), pools.len());
    let total_apoints: u128 = resp
        .pool_alloc_points
        .iter()
        .cloned()
        .map(|(_, apoints)| apoints.u128())
        .sum();
    assert_eq!(total_apoints, 300_000_000);

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
    assert_eq!(resp.tune_period, router.block_period());
    assert_eq!(resp.pool_alloc_points.len(), limit as usize);
    let total_apoints: u128 = resp
        .pool_alloc_points
        .iter()
        .cloned()
        .map(|(_, apoints)| apoints.u128())
        .sum();
    assert_eq!(total_apoints, 220_000_000);

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
    let helper = ControllerHelper::init(&mut router, &owner_addr, None);
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

    // We must be able to add any pool to the whitelist as we can't validate
    // pools on other chains
    let result = helper.update_whitelist(
        &mut router,
        "owner",
        Some(vec![("random_pool".to_string())]),
        None,
    );
    assert!(result.is_ok());

    helper
        .update_whitelist(
            &mut router,
            "owner",
            Some(pools.iter().map(|el| el.to_string()).collect()),
            None,
        )
        .unwrap();

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
    let asset_infos = vec![
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
            &astroport::factory::ExecuteMsg::Deregister {
                asset_infos: asset_infos.to_vec(),
            },
            &[],
        )
        .unwrap();

    // We can vote for deregistered pool as we can't validate the information
    // from other chains
    let result = helper.vote(&mut router, user, vec![(pools[0].as_str(), 10000)]);
    assert!(result.is_ok());

    // Tune should fail as the pair is not registered in the generator
    let err = helper.tune(&mut router).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: The pair is not registered: wasm1contract10-wasm1contract11"
    );

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
    // Tune should fail as we have a token blocked in the generator
    let err = helper.tune(&mut router).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Token wasm1contract10 is blocked!"
    );

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
    assert_eq!(total_apoints, 50_000_000)
}

#[test]
fn check_update_owner() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut app, &owner, None);

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
    let helper = ControllerHelper::init(&mut router, &owner_addr, None);
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

    helper.escrow_helper.mint_xastro(&mut router, "owner", 100);

    for user in ["user1", "user2"] {
        helper.escrow_helper.mint_xastro(&mut router, user, 100);
        helper
            .escrow_helper
            .create_lock(&mut router, user, MAX_LOCK_TIME, 100f32)
            .unwrap();
    }

    helper
        .update_whitelist(
            &mut router,
            "owner",
            Some(pools.iter().map(|el| el.to_string()).collect()),
            None,
        )
        .unwrap();

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
    assert_eq!(main_pool_info.vxastro_amount.u128(), 10_000_000);

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
        "wasm1contract13 is the main pool. Voting or whitelisting the main pool is prohibited."
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
    assert_eq!(resp.pool_alloc_points.len(), 3_usize);
    let total_apoints: Uint128 = resp
        .pool_alloc_points
        .iter()
        .map(|(_, apoints)| apoints)
        .sum();
    assert_eq!(total_apoints.u128(), 128571428);
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
    assert_eq!(resp.pool_alloc_points.len(), 2_usize);
}

#[test]
fn check_add_network() {
    let mut router = mock_app();
    let owner_addr = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner_addr, None);

    // Attempt to duplicate the native/home network
    let add_network = NetworkInfo {
        address_prefix: "unknown".to_string(),
        generator_address: Addr::unchecked("wasm1contract"),
        ibc_channel: None,
    };

    // Test success
    let result = helper.update_networks(&mut router, "owner", Some(vec![add_network]), None);
    assert!(result.is_err());

    let add_network = NetworkInfo {
        address_prefix: "unknown".to_string(),
        generator_address: Addr::unchecked("wasmx1contract"),
        ibc_channel: None,
    };

    // Test success
    let result = helper.update_networks(&mut router, "owner", Some(vec![add_network]), None);
    assert!(result.is_ok());

    let add_network = NetworkInfo {
        address_prefix: "unknown".to_string(),
        generator_address: Addr::unchecked("wasm1contract"),
        ibc_channel: None,
    };

    // Test for duplicate
    let err = helper
        .update_networks(&mut router, "owner", Some(vec![add_network]), None)
        .unwrap_err();
    assert_eq!(
        "Generic error: The resulting whitelist contains duplicated prefixes. It's either provided 'add' list contains duplicated prefixes or some of the added prefixes are already whitelisted.",
        err.root_cause().to_string()
    );
}

#[test]
fn check_remove_network() {
    let mut router = mock_app();
    let owner_addr = Addr::unchecked("owner");
    let helper = ControllerHelper::init(&mut router, &owner_addr, None);

    let add_network = NetworkInfo {
        address_prefix: "unknown".to_string(),
        generator_address: Addr::unchecked("wasmx1contract"),
        ibc_channel: None,
    };

    // Add network
    helper
        .update_networks(&mut router, "owner", Some(vec![add_network]), None)
        .unwrap();

    // Test remove invalid network
    helper
        .update_networks(&mut router, "owner", None, Some(vec!["testx".to_string()]))
        .unwrap();

    // We'll still have the default and the added network
    let config = helper.query_config(&mut router).unwrap();
    let prefixes: Vec<String> = config
        .whitelisted_networks
        .into_iter()
        .map(|network_info| network_info.address_prefix)
        .collect();
    assert_eq!(prefixes, vec!["wasm".to_string(), "wasmx".to_string()]);

    // Test remove native/home network, this should not succeed
    let err = helper
        .update_networks(&mut router, "owner", None, Some(vec!["wasm".to_string()]))
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Cannot remove the native network with prefix wasm".to_string()
    );

    // Attempt to remove the network we added, should pass
    helper
        .update_networks(&mut router, "owner", None, Some(vec!["wasmx".to_string()]))
        .unwrap();

    // We'll still have the default and the added network
    let config = helper.query_config(&mut router).unwrap();
    let prefixes: Vec<String> = config
        .whitelisted_networks
        .into_iter()
        .map(|network_info| network_info.address_prefix)
        .collect();
    assert_eq!(prefixes, vec!["wasm".to_string()]);
}
