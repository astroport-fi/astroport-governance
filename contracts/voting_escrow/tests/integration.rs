use astroport::token as astro;
use cosmwasm_std::{attr, to_binary, Addr, Fraction, StdError, Uint128};
use cw20::{Cw20ExecuteMsg, Logo, LogoInfo, MarketingInfoResponse, MinterResponse};
use cw_multi_test::{next_block, ContractWrapper, Executor};
use voting_escrow::astroport;

use astroport_governance::utils::{get_period, MAX_LOCK_TIME, WEEK};
use astroport_governance::voting_escrow::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, LockInfoResponse, QueryMsg,
};

use crate::test_utils::{mock_app, Helper, MULTIPLIER};

mod test_utils;

#[test]
fn lock_unlock_logic() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    helper.mint_xastro(router_ref, "owner", 100);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user", 100);
    helper.check_xastro_balance(router_ref, "user", 100);

    // Create invalid vx position
    let err = helper
        .create_lock(router_ref, "user", WEEK - 1, 1f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Lock time must be within limits (week <= lock time < 2 years)"
    );
    let err = helper
        .create_lock(router_ref, "user", MAX_LOCK_TIME + 1, 1f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Lock time must be within limits (week <= lock time < 2 years)"
    );
    let err = helper
        .create_lock(router_ref, "user", WEEK, 101f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        format!(
            "Cannot Sub with {} and {}",
            100 * MULTIPLIER,
            101 * MULTIPLIER
        )
    );

    // Try to increase the lock time for a position that doesn't exist
    let err = helper
        .extend_lock_time(router_ref, "user", MAX_LOCK_TIME)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Lock does not exist");

    // Try to withdraw from a non-existent lock
    let err = helper.withdraw(router_ref, "user").unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Lock does not exist");

    // Try to deposit more xASTRO in a position that does not already exist
    let err = helper
        .extend_lock_amount(router_ref, "user", 1f32)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Lock does not exist");

    // Current total voting power is 0
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 0.0);

    // Create valid voting escrow lock
    helper
        .create_lock(router_ref, "user", WEEK * 2, 90f32)
        .unwrap();
    // Check that 90 xASTRO were actually debited
    helper.check_xastro_balance(router_ref, "user", 10);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 90);

    // A user can have a single vxASTRO position
    let err = helper
        .create_lock(router_ref, "user", MAX_LOCK_TIME, 1f32)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Lock already exists");

    // Try to increase the lock time by less than a week
    let err = helper
        .extend_lock_time(router_ref, "user", 86400)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Lock time must be within limits (week <= lock time < 2 years)"
    );

    // Try to exceed MAX_LOCK_TIME
    // We locked for 2 weeks so increasing by MAX_LOCK_TIME - week is impossible
    let err = helper
        .extend_lock_time(router_ref, "user", MAX_LOCK_TIME - WEEK)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Lock time must be within limits (week <= lock time < 2 years)"
    );

    // Add more xASTRO to the existing position
    helper.extend_lock_amount(router_ref, "user", 9f32).unwrap();
    helper.check_xastro_balance(router_ref, "user", 1);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 99);

    // Try to withdraw from a non-expired lock
    let err = helper.withdraw(router_ref, "user").unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock time has not yet expired"
    );

    // Go in the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));

    // The lock has not yet expired since we locked for 2 weeks
    let err = helper.withdraw(router_ref, "user").unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock time has not yet expired"
    );

    // Go to the future again
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));

    // Try to add more xASTRO to an expired position
    let err = helper
        .extend_lock_amount(router_ref, "user", 1f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock expired. Withdraw and create new lock"
    );
    // Try to increase the lock time for an expired position
    let err = helper
        .extend_lock_time(router_ref, "user", WEEK)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock expired. Withdraw and create new lock"
    );

    // Imagine the user will withdraw their expired lock in 5 weeks
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(5 * WEEK));

    // Time has passed so we can withdraw
    helper.withdraw(router_ref, "user").unwrap();
    helper.check_xastro_balance(router_ref, "user", 100);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 0);

    // Check that the lock has disappeared
    let err = helper
        .extend_lock_amount(router_ref, "user", 1f32)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Lock does not exist");
}

#[test]
fn random_token_lock() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    let random_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));
    let random_token_code_id = router.store_code(random_token_contract);

    let msg = astro::InstantiateMsg {
        name: String::from("Random token"),
        symbol: String::from("FOO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: helper.owner.to_string(),
            cap: None,
        }),
        marketing: None,
    };

    let random_token = router
        .instantiate_contract(
            random_token_code_id,
            helper.owner.clone(),
            &msg,
            &[],
            String::from("FOO"),
            None,
        )
        .unwrap();

    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: String::from("user"),
        amount: Uint128::from(100_u128),
    };

    router
        .execute_contract(helper.owner.clone(), random_token.clone(), &msg, &[])
        .unwrap();

    let cw20msg = Cw20ExecuteMsg::Send {
        contract: helper.voting_instance.to_string(),
        amount: Uint128::from(10_u128),
        msg: to_binary(&Cw20HookMsg::CreateLock { time: WEEK }).unwrap(),
    };
    let err = router
        .execute_contract(Addr::unchecked("user"), random_token, &cw20msg, &[])
        .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "Unauthorized");
}

#[test]
fn new_lock_after_lock_expired() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let helper = Helper::init(router_ref, Addr::unchecked("owner"));

    helper.mint_xastro(router_ref, "owner", 100);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user", 100);

    helper
        .create_lock(router_ref, "user", WEEK * 5, 50f32)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 53.605762);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 53.605762);

    // Go to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));

    helper.withdraw(router_ref, "user").unwrap();
    helper.check_xastro_balance(router_ref, "user", 100);

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 0.0);

    // Create a new lock in 3 weeks from now
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 3));

    helper
        .create_lock(router_ref, "user", WEEK * 5, 100f32)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 107.21153);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 107.21153);
}

/// Plot for this test case generated at tests/plots/constant_decay.png
#[test]
fn voting_constant_decay() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let helper = Helper::init(router_ref, Addr::unchecked("owner"));

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user", 100);
    helper.mint_xastro(router_ref, "user2", 50);

    helper
        .create_lock(router_ref, "user", WEEK * 10, 30f32)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 34.32692);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 34.32692);

    // Since user2 did not lock their xASTRO, the contract does not have any information
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 0.0);

    // Go to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));

    // We can check voting power in the past
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() - WEEK,
        )
        .unwrap();
    assert_eq!(res, 20.596151);
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() - 3 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 27.461536);
    let res = helper
        .query_total_vp_at(
            router_ref,
            router_ref.block_info().time.seconds() - 5 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 34.32692);

    // And we can even check voting power in the future
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() + WEEK,
        )
        .unwrap();
    assert_eq!(res, 13.730768);
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() + 5 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 0.0);

    // Create lock for user2
    helper
        .create_lock(router_ref, "user2", WEEK * 6, 50f32)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 17.16346);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 54.32692);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 71.49039);
    let res = helper
        .query_total_vp_at(
            router_ref,
            router_ref.block_info().time.seconds() + 4 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 21.541666);

    // Go to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 9.054487);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 9.054487);

    // Go to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 0.0);
}

/// Plot for this test case is generated at tests/plots/variable_decay.png
#[test]
fn voting_variable_decay() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let helper = Helper::init(router_ref, Addr::unchecked("owner"));

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "owner", 100);
    helper.mint_xastro(router_ref, "user", 100);
    helper.mint_xastro(router_ref, "user2", 100);

    helper
        .create_lock(router_ref, "user", WEEK * 10, 30f32)
        .unwrap();

    // Go to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));

    // Create lock for user2
    helper
        .create_lock(router_ref, "user2", WEEK * 6, 50f32)
        .unwrap();
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 71.49039);

    // Go to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 4));

    helper
        .extend_lock_amount(router_ref, "user", 70f32)
        .unwrap();
    helper
        .extend_lock_time(router_ref, "user2", WEEK * 8)
        .unwrap();
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 74.44231);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 57.21153);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 131.65384);

    let res = helper
        .query_user_vp_at(
            router_ref,
            "user2",
            router_ref.block_info().time.seconds() + 4 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 34.32692);
    let res = helper
        .query_total_vp_at(router_ref, router_ref.block_info().time.seconds() + WEEK)
        .unwrap();
    assert_eq!(res, 51.490376);

    // Go to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 51.490376);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 51.490376);
}

#[test]
fn check_queries() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    helper.mint_xastro(router_ref, "owner", 100);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user", 100);
    helper.check_xastro_balance(router_ref, "user", 100);

    // Create valid voting escrow lock
    helper
        .create_lock(router_ref, "user", WEEK * 2, 90f32)
        .unwrap();
    // Check that 90 xASTRO were actually debited
    helper.check_xastro_balance(router_ref, "user", 10);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 90);

    // Validate user's lock
    let cur_period = get_period(router_ref.block_info().time.seconds()).unwrap();
    let user_lock: LockInfoResponse = router_ref
        .wrap()
        .query_wasm_smart(
            helper.voting_instance.clone(),
            &QueryMsg::LockInfo {
                user: "user".to_string(),
            },
        )
        .unwrap();
    assert_eq!(user_lock.amount.u128(), 90_u128 * MULTIPLIER as u128);
    assert_eq!(user_lock.start, cur_period);
    assert_eq!(user_lock.end, cur_period + 2);
    let coeff = user_lock.coefficient.numerator().u128() as f32
        / user_lock.coefficient.denominator().u128() as f32;
    if (coeff - 1.02884f32).abs() > 1e-5 {
        assert_eq!(coeff, 1.02884f32)
    }

    let total_vp_at_period = helper
        .query_total_vp_at_period(router_ref, cur_period)
        .unwrap();
    let total_vp_at_ts = helper
        .query_total_vp_at(router_ref, router_ref.block_info().time.seconds())
        .unwrap();
    assert_eq!(total_vp_at_period, total_vp_at_ts);

    let user_vp_at_period = helper
        .query_user_vp_at_period(router_ref, "user", cur_period)
        .unwrap();
    let user_vp = helper
        .query_user_vp_at(router_ref, "user", router_ref.block_info().time.seconds())
        .unwrap();
    assert_eq!(user_vp_at_period, user_vp);

    // Check users' locked xASTRO balance history
    helper.mint_xastro(router_ref, "user", 90);
    // SnapshotMap checkpoints the data at the next block
    let start_height = router_ref.block_info().height + 1;
    let balance = helper
        .query_locked_balance_at(router_ref, "user", start_height)
        .unwrap();
    assert_eq!(balance, 90f32);
    // Make the lockup to live longer
    helper
        .extend_lock_time(router_ref, "user", WEEK * 100)
        .unwrap();

    router_ref.update_block(next_block);
    helper
        .extend_lock_amount(router_ref, "user", 100f32)
        .unwrap();
    let balance = helper
        .query_locked_balance_at(router_ref, "user", start_height)
        .unwrap();
    assert_eq!(balance, 90f32);

    router_ref.update_block(|bi| bi.height += 100000);
    let balance = helper
        .query_locked_balance_at(router_ref, "user", start_height)
        .unwrap();
    assert_eq!(balance, 90f32);
    let balance = helper
        .query_locked_balance_at(router_ref, "user", start_height + 2)
        .unwrap();
    assert_eq!(balance, 190f32);
    // The user still has 190 xASTRO locked
    let balance = helper
        .query_locked_balance_at(router_ref, "user", router_ref.block_info().height)
        .unwrap();
    assert_eq!(balance, 190f32);

    router_ref.update_block(|bi| {
        bi.height += 1;
        bi.time = bi.time.plus_seconds(WEEK * 102);
    });
    helper.withdraw(router_ref, "user").unwrap();
    // Now the users' balance is zero
    let cur_height = router_ref.block_info().height + 1;
    let balance = helper
        .query_locked_balance_at(router_ref, "user", cur_height)
        .unwrap();
    // But one block before it had 190 xASTRO locked
    assert_eq!(balance, 0f32);
    let balance = helper
        .query_locked_balance_at(router_ref, "user", cur_height - 1)
        .unwrap();
    assert_eq!(balance, 190f32);

    // add users to the blacklist
    helper
        .update_blacklist(
            router_ref,
            Some(vec![
                "voter1".to_string(),
                "voter2".to_string(),
                "voter3".to_string(),
                "voter4".to_string(),
                "voter5".to_string(),
                "voter6".to_string(),
                "voter7".to_string(),
                "voter8".to_string(),
            ]),
            None,
        )
        .unwrap();

    // query all blacklisted voters
    let blacklisted_voters = helper
        .query_blacklisted_voters(router_ref, None, None)
        .unwrap();
    assert_eq!(
        blacklisted_voters,
        vec![
            Addr::unchecked("voter1"),
            Addr::unchecked("voter2"),
            Addr::unchecked("voter3"),
            Addr::unchecked("voter4"),
            Addr::unchecked("voter5"),
            Addr::unchecked("voter6"),
            Addr::unchecked("voter7"),
            Addr::unchecked("voter8"),
        ]
    );

    // query not blacklisted voter
    let err = helper
        .query_blacklisted_voters(router_ref, Some("voter9".to_string()), Some(10u32))
        .unwrap_err();
    assert_eq!(
        StdError::generic_err(
            "Querier contract error: Generic error: The voter9 address is not blacklisted"
        ),
        err
    );

    // query voters by specified parameters
    let blacklisted_voters = helper
        .query_blacklisted_voters(router_ref, Some("voter2".to_string()), Some(2u32))
        .unwrap();
    assert_eq!(
        blacklisted_voters,
        vec![Addr::unchecked("voter3"), Addr::unchecked("voter4")]
    );

    // add users to the blacklist
    helper
        .update_blacklist(
            router_ref,
            Some(vec!["voter0".to_string(), "voter33".to_string()]),
            None,
        )
        .unwrap();

    // query voters by specified parameters
    let blacklisted_voters = helper
        .query_blacklisted_voters(router_ref, Some("voter2".to_string()), Some(2u32))
        .unwrap();
    assert_eq!(
        blacklisted_voters,
        vec![Addr::unchecked("voter3"), Addr::unchecked("voter33")]
    );

    let blacklisted_voters = helper
        .query_blacklisted_voters(router_ref, Some("voter4".to_string()), Some(10u32))
        .unwrap();
    assert_eq!(
        blacklisted_voters,
        vec![
            Addr::unchecked("voter5"),
            Addr::unchecked("voter6"),
            Addr::unchecked("voter7"),
            Addr::unchecked("voter8"),
        ]
    );

    let empty_blacklist: Vec<Addr> = vec![];
    let blacklisted_voters = helper
        .query_blacklisted_voters(router_ref, Some("voter8".to_string()), Some(10u32))
        .unwrap();
    assert_eq!(blacklisted_voters, empty_blacklist);

    // check if voters are blacklisted
    let res = helper
        .check_voters_are_blacklisted(router_ref, vec!["voter1".to_string(), "voter9".to_string()])
        .unwrap();
    assert_eq!("Voter is not blacklisted: voter9", res.to_string());

    let res = helper
        .check_voters_are_blacklisted(router_ref, vec!["voter1".to_string(), "voter8".to_string()])
        .unwrap();
    assert_eq!("Voters are blacklisted!", res.to_string());
}

#[test]
fn check_deposit_for() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    helper.mint_xastro(router_ref, "owner", 100);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user1", 100);
    helper.check_xastro_balance(router_ref, "user1", 100);
    helper.mint_xastro(router_ref, "user2", 100);
    helper.check_xastro_balance(router_ref, "user2", 100);

    // 104 weeks ~ 2 years
    helper
        .create_lock(router_ref, "user1", 104 * WEEK, 50f32)
        .unwrap();
    let vp = helper.query_user_vp(router_ref, "user1").unwrap();
    assert_eq!(124.99999, vp);
    helper
        .deposit_for(router_ref, "user2", "user1", 50f32)
        .unwrap();
    let vp = helper.query_user_vp(router_ref, "user1").unwrap();
    assert_eq!(249.99998, vp);
    helper.check_xastro_balance(router_ref, "user1", 50);
    helper.check_xastro_balance(router_ref, "user2", 50);
}

#[test]
fn check_update_owner() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut app, owner);

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
            helper.voting_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.voting_instance.clone(),
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
        helper.voting_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // Claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.voting_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        helper.voting_instance.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // Let's query the contract state
    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&helper.voting_instance, &msg)
        .unwrap();

    assert_eq!(res.owner, new_owner)
}

#[test]
fn check_blacklist() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user1", 100);
    helper.mint_xastro(router_ref, "user2", 100);
    helper.mint_xastro(router_ref, "user3", 100);

    // Try to execute with empty arrays
    let err = helper.update_blacklist(router_ref, None, None).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Append and remove arrays are empty"
    );

    // Blacklisting user2
    let res = helper
        .update_blacklist(router_ref, Some(vec!["user2".to_string()]), None)
        .unwrap();
    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "update_blacklist")
    );
    assert_eq!(
        res.events[1].attributes[2],
        attr("added_addresses", "user2")
    );

    helper
        .create_lock(router_ref, "user1", WEEK * 10, 50f32)
        .unwrap();
    // Try to create lock from a blacklisted address
    let err = helper
        .create_lock(router_ref, "user2", WEEK * 10, 100f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user2 address is blacklisted"
    );
    let err = helper
        .deposit_for(router_ref, "user2", "user3", 50f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user2 address is blacklisted"
    );

    // Since user2 is blacklisted, their xASTRO balance was left unchanged
    helper.check_xastro_balance(router_ref, "user2", 100);
    // And they did not create a lock, thus we have no information to query
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 0.0);

    // Go to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(2 * WEEK));

    // user2 is still blacklisted
    let err = helper
        .create_lock(router_ref, "user2", WEEK * 10, 100f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user2 address is blacklisted"
    );

    // Blacklisting user1 using the guardian
    let msg = ExecuteMsg::UpdateBlacklist {
        append_addrs: Some(vec!["user1".to_string()]),
        remove_addrs: None,
    };
    let res = router_ref
        .execute_contract(
            Addr::unchecked("guardian"),
            helper.voting_instance.clone(),
            &msg,
            &[],
        )
        .unwrap();
    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "update_blacklist")
    );
    assert_eq!(
        res.events[1].attributes[2],
        attr("added_addresses", "user1")
    );

    // user1 is now blacklisted
    let err = helper
        .extend_lock_time(router_ref, "user1", WEEK * 10)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user1 address is blacklisted"
    );
    let err = helper
        .extend_lock_amount(router_ref, "user1", 10f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user1 address is blacklisted"
    );
    let err = helper
        .deposit_for(router_ref, "user2", "user1", 50f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user2 address is blacklisted"
    );
    let err = helper
        .deposit_for(router_ref, "user3", "user1", 50f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user1 address is blacklisted"
    );
    // user1 doesn't have voting power now
    let vp = helper.query_user_vp(router_ref, "user1").unwrap();
    assert_eq!(vp, 0.0);
    // But they have voting power in the past
    let vp = helper
        .query_user_vp_at(
            router_ref,
            "user1",
            router_ref.block_info().time.seconds() - WEEK,
        )
        .unwrap();
    assert_eq!(vp, 51.490376);
    // Total voting power should be zero as well since there was only one vxASTRO position created by user1
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 0.0);

    // Go to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(20 * WEEK));

    // The only option available for a blacklisted user is to withdraw their funds if their lock expired
    helper.withdraw(router_ref, "user1").unwrap();

    // Remove user1 from the blacklist
    let res = helper
        .update_blacklist(router_ref, None, Some(vec!["user1".to_string()]))
        .unwrap();
    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "update_blacklist")
    );
    assert_eq!(
        res.events[1].attributes[2],
        attr("removed_addresses", "user1")
    );

    // Now user1 can create a new lock
    helper
        .create_lock(router_ref, "user1", WEEK, 10f32)
        .unwrap();
}

#[test]
fn check_residual() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);
    let lock_duration = 104;
    let users_num = 1000;
    let lock_amount = 100_000_000;

    helper.mint_xastro(router_ref, "owner", 100);

    for i in 1..(users_num / 2) {
        let user = &format!("user{}", i);
        helper.mint_xastro(router_ref, user, 100);
        helper
            .create_lock_u128(router_ref, user, WEEK * lock_duration, lock_amount)
            .unwrap();
    }

    let mut sum = 0;
    for i in 1..=users_num {
        let user = &format!("user{}", i);
        sum += helper.query_exact_user_vp(router_ref, user).unwrap();
    }

    assert_eq!(sum, helper.query_exact_total_vp(router_ref).unwrap());

    router_ref.update_block(|bi| {
        bi.height += 1;
        bi.time = bi.time.plus_seconds(WEEK);
    });

    for i in (users_num / 2)..users_num {
        let user = &format!("user{}", i);
        helper.mint_xastro(router_ref, user, 1000000);
        helper
            .create_lock_u128(router_ref, user, WEEK * lock_duration, lock_amount)
            .unwrap();
    }

    for _ in 1..104 {
        sum = 0;
        for i in 1..=users_num {
            let user = &format!("user{}", i);
            sum += helper.query_exact_user_vp(router_ref, user).unwrap();
        }

        let ve_vp = helper.query_exact_total_vp(router_ref).unwrap();
        let diff = (sum as f64 - ve_vp as f64).abs();
        assert_eq!(diff, 0.0, "diff: {}, sum: {}, ve_vp: {}", diff, sum, ve_vp);

        router_ref.update_block(|bi| {
            bi.height += 1;
            bi.time = bi.time.plus_seconds(WEEK);
        });
    }
}

#[test]
fn total_vp_multiple_slope_subtraction() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    helper.mint_xastro(router_ref, "user1", 1000);
    helper
        .create_lock(router_ref, "user1", 2 * WEEK, 100f32)
        .unwrap();
    let total = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(total, 102.88461);

    router_ref.update_block(|bi| bi.time = bi.time.plus_seconds(2 * WEEK));
    // Slope changes have been applied
    let total = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(total, 0.0);

    // Try to manipulate over expired lock 2 weeks later
    router_ref.update_block(|bi| bi.time = bi.time.plus_seconds(2 * WEEK));
    let err = helper
        .extend_lock_amount(router_ref, "user1", 100f32)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock expired. Withdraw and create new lock"
    );
    let err = helper
        .create_lock(router_ref, "user1", 2 * WEEK, 100f32)
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Lock already exists");
    let err = helper
        .extend_lock_time(router_ref, "user1", 2 * WEEK)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock expired. Withdraw and create new lock"
    );
    let total = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(total, 0f32);
}

#[test]
fn marketing_info() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    let err = router_ref
        .execute_contract(
            helper.owner.clone(),
            helper.voting_instance.clone(),
            &ExecuteMsg::SetLogoUrlsWhitelist {
                whitelist: vec![
                    "@hello-test-url .com/".to_string(),
                    "example.com/".to_string(),
                ],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.root_cause().to_string(),
        "Generic error: Link contains invalid characters: @hello-test-url .com/"
    );

    let err = router_ref
        .execute_contract(
            helper.owner.clone(),
            helper.voting_instance.clone(),
            &ExecuteMsg::SetLogoUrlsWhitelist {
                whitelist: vec!["example.com".to_string()],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.root_cause().to_string(),
        "Marketing info validation error: Whitelist link should end with '/': example.com"
    );

    router_ref
        .execute_contract(
            helper.owner.clone(),
            helper.voting_instance.clone(),
            &ExecuteMsg::SetLogoUrlsWhitelist {
                whitelist: vec!["example.com/".to_string()],
            },
            &[],
        )
        .unwrap();

    let err = router_ref
        .execute_contract(
            helper.owner.clone(),
            helper.voting_instance.clone(),
            &ExecuteMsg::UpdateMarketing {
                project: Some("<script>alert('test')</script>".to_string()),
                description: None,
                marketing: None,
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        &err.root_cause().to_string(),
        "Marketing info validation error: project contains invalid characters: <script>alert('test')</script>"
    );

    let err = router_ref
        .execute_contract(
            helper.owner.clone(),
            helper.voting_instance.clone(),
            &ExecuteMsg::UpdateMarketing {
                project: None,
                description: Some("<script>alert('test')</script>".to_string()),
                marketing: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.root_cause().to_string(),
        "Marketing info validation error: description contains invalid characters: <script>alert('test')</script>"
    );

    router_ref
        .execute_contract(
            helper.owner.clone(),
            helper.voting_instance.clone(),
            &ExecuteMsg::UpdateMarketing {
                project: Some("Some project".to_string()),
                description: Some("Some description".to_string()),
                marketing: None,
            },
            &[],
        )
        .unwrap();

    let config: ConfigResponse = router_ref
        .wrap()
        .query_wasm_smart(&helper.voting_instance, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.logo_urls_whitelist, vec!["example.com/".to_string()]);
    let marketing_info: MarketingInfoResponse = router_ref
        .wrap()
        .query_wasm_smart(&helper.voting_instance, &QueryMsg::MarketingInfo {})
        .unwrap();
    assert_eq!(marketing_info.project, Some("Some project".to_string()));
    assert_eq!(
        marketing_info.description,
        Some("Some description".to_string())
    );

    let err = router_ref
        .execute_contract(
            helper.owner.clone(),
            helper.voting_instance.clone(),
            &ExecuteMsg::UploadLogo(Logo::Url("https://some-website.com/logo.svg".to_string())),
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.root_cause().to_string(),
        "Marketing info validation error: Logo link is not whitelisted: https://some-website.com/logo.svg",
    );

    router_ref
        .execute_contract(
            helper.owner.clone(),
            helper.voting_instance.clone(),
            &ExecuteMsg::UploadLogo(Logo::Url("example.com/logo.svg".to_string())),
            &[],
        )
        .unwrap();

    let marketing_info: MarketingInfoResponse = router_ref
        .wrap()
        .query_wasm_smart(&helper.voting_instance, &QueryMsg::MarketingInfo {})
        .unwrap();
    assert_eq!(
        marketing_info.logo.unwrap(),
        LogoInfo::Url("example.com/logo.svg".to_string())
    );
}
