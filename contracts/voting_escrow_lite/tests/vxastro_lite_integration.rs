use cosmwasm_std::{attr, Addr, StdError, Uint64};
use cw20::{Logo, LogoInfo, MarketingInfoResponse};
use cw_multi_test::{next_block, Executor};

use astroport_governance::utils::{get_lite_period, WEEK};
use astroport_governance::voting_escrow_lite::{Config, ExecuteMsg, LockInfoResponse, QueryMsg};

use crate::test_utils::{Helper, MULTIPLIER};

mod test_utils;

#[test]
fn lock_unlock_logic() {
    let mut helper = Helper::init();

    helper.mint_xastro("owner", 100);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro("user", 100);
    helper.check_xastro_balance("user", 100);

    // Try to withdraw from a non-existent lock
    let err = helper.withdraw("user").unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Lock does not exist");

    // Try to deposit more xASTRO in a position that does not already exist
    // This should create a new lock
    helper.extend_lock_amount("user", 1f32).unwrap();
    helper.check_xastro_balance("user", 99);
    helper.check_xastro_balance(helper.vxastro.as_str(), 1);

    // Current total voting power is 0
    let vp = helper.query_total_vp().unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_emissions_vp().unwrap();
    assert_eq!(vp, 1.0);

    // Try to create another voting escrow lock
    let err = helper.create_lock("user", 90f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Lock already exists, either unlock and withdraw or extend_lock to add to the lock"
    );

    // Check that 90 xASTRO were not debited
    helper.check_xastro_balance("user", 99);
    helper.check_xastro_balance(helper.vxastro.as_str(), 1);

    // Add more xASTRO to the existing position
    helper.extend_lock_amount("user", 9f32).unwrap();
    helper.check_xastro_balance("user", 90);
    helper.check_xastro_balance(helper.vxastro.as_str(), 10);

    // Try to withdraw from a non-unlocked lock
    let err = helper.withdraw("user").unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock has not been unlocked, call unlock first"
    );

    helper.unlock("user").unwrap();

    // Go in the future
    helper.app.update_block(next_block);
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(WEEK));

    // The lock has not yet expired since unlocking has a 2 week waiting time
    let err = helper.withdraw("user").unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock time has not yet expired"
    );

    // Go to the future again
    helper.app.update_block(next_block);
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(WEEK));

    // Try to add more xASTRO to an expired position
    let err = helper.extend_lock_amount("user", 1f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock expired. Withdraw and create new lock"
    );

    // Imagine the user will withdraw their expired lock in 5 weeks
    helper.app.update_block(next_block);
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(5 * WEEK));

    // Time has passed so we can withdraw
    helper.withdraw("user").unwrap();
    helper.check_xastro_balance("user", 100);
    helper.check_xastro_balance(helper.vxastro.as_str(), 0);

    // Create a new lock
    helper.extend_lock_amount("user", 50f32).unwrap();

    let vp = helper.query_total_emissions_vp().unwrap();
    assert_eq!(vp, 50.0);

    let vp = helper.query_user_emissions_vp("user").unwrap();
    assert_eq!(vp, 50.0);

    // Unlock the lock
    helper.unlock("user").unwrap();

    let vp = helper.query_total_emissions_vp().unwrap();
    assert_eq!(vp, 0.0);

    let vp = helper.query_user_emissions_vp("user").unwrap();
    assert_eq!(vp, 0.0);

    // Relock
}

#[test]
fn new_lock_after_unlock() {
    let mut helper = Helper::init();
    helper.mint_xastro("owner", 100);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro("user", 100);

    helper.create_lock("user", 50f32).unwrap();

    let vp = helper.query_user_vp("user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_vp().unwrap();
    assert_eq!(vp, 0.0);

    let evp = helper.query_user_emissions_vp("user").unwrap();
    assert_eq!(evp, 50.0);
    let evp = helper.query_total_emissions_vp().unwrap();
    assert_eq!(evp, 50.0);

    // Go to the future
    helper.app.update_block(next_block);

    helper.unlock("user").unwrap();
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(WEEK * 2));

    helper.withdraw("user").unwrap();
    helper.check_xastro_balance("user", 100);

    let vp = helper.query_user_vp("user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_vp().unwrap();
    assert_eq!(vp, 0.0);

    // Create a new lock in 3 weeks from now
    helper.app.update_block(next_block);
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(WEEK * 3));

    helper.create_lock("user", 100f32).unwrap();

    let vp = helper.query_user_vp("user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_vp().unwrap();
    assert_eq!(vp, 0.0);

    let evp = helper.query_user_emissions_vp("user").unwrap();
    assert_eq!(evp, 100.0);
    let evp = helper.query_total_emissions_vp().unwrap();
    assert_eq!(evp, 100.0);
}

/// Plot for this test case is generated at tests/plots/variable_decay.png
#[test]
fn emissions_voting_no_decay() {
    let mut helper = Helper::init();
    helper.mint_xastro("owner", 100);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro("user", 100);
    helper.mint_xastro("user2", 100);

    helper.create_lock("user", 30f32).unwrap();

    // Go to the future
    helper.app.update_block(next_block);
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));

    // Create lock for user2
    helper.create_lock("user2", 50f32).unwrap();
    let vp = helper.query_total_vp().unwrap();
    assert_eq!(vp, 0.0);

    let vp = helper.query_total_emissions_vp().unwrap();
    assert_eq!(vp, 80.0);

    // Go to the future
    helper.app.update_block(next_block);
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(WEEK * 4));

    helper.extend_lock_amount("user", 70f32).unwrap();

    let vp = helper.query_user_vp("user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_vp("user2").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_vp().unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_emissions_vp("user").unwrap();
    assert_eq!(vp, 100.0);
    let vp = helper.query_user_emissions_vp("user2").unwrap();
    assert_eq!(vp, 50.0);
    let vp = helper.query_total_emissions_vp().unwrap();
    assert_eq!(vp, 150.0);

    let res = helper
        .query_user_vp_at("user2", helper.app.block_info().time.seconds() + 4 * WEEK)
        .unwrap();
    assert_eq!(res, 0.0);
    let res = helper
        .query_total_vp_at(helper.app.block_info().time.seconds() + WEEK)
        .unwrap();
    assert_eq!(res, 0.0);

    let res = helper
        .query_user_emissions_vp_at("user2", helper.app.block_info().time.seconds() + 4 * WEEK)
        .unwrap();
    assert_eq!(res, 50.0);
    let res = helper
        .query_total_emissions_vp_at(helper.app.block_info().time.seconds() + WEEK)
        .unwrap();
    assert_eq!(res, 150.0);

    // Go to the future
    helper.app.update_block(next_block);
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(WEEK));
    let vp = helper.query_user_vp("user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_vp("user2").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_vp().unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_emissions_vp("user").unwrap();
    assert_eq!(vp, 100.0);
    let vp = helper.query_user_emissions_vp("user2").unwrap();
    assert_eq!(vp, 50.0);
    let vp = helper.query_total_emissions_vp().unwrap();
    assert_eq!(vp, 150.0);
}

#[test]
fn check_queries() {
    let mut helper = Helper::init();
    helper.mint_xastro("owner", 100);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro("user", 100);
    helper.check_xastro_balance("user", 100);

    // Create valid voting escrow lock
    helper.create_lock("user", 90f32).unwrap();
    // Check that 90 xASTRO were actually debited
    helper.check_xastro_balance("user", 10);
    helper.check_xastro_balance(helper.vxastro.as_str(), 90);

    // Validate user's lock
    let user_lock: LockInfoResponse = helper
        .app
        .wrap()
        .query_wasm_smart(
            helper.vxastro.clone(),
            &QueryMsg::LockInfo {
                user: "user".to_string(),
            },
        )
        .unwrap();
    assert_eq!(user_lock.amount.u128(), 90_u128 * MULTIPLIER as u128);
    // New locks must not have an end time
    assert_eq!(user_lock.end, None);

    // Voting power must be 0
    let total_vp_at_ts = helper
        .query_total_vp_at(helper.app.block_info().time.seconds())
        .unwrap();
    assert_eq!(total_vp_at_ts, 0.0);

    // Must always be 0
    let period = get_lite_period(helper.app.block_info().time.seconds()).unwrap();
    let total_vp_at_period = helper.query_total_vp_at_period(period).unwrap();
    assert_eq!(total_vp_at_period, 0.0);

    // Must always be 0
    let user_vp = helper
        .query_user_vp_at("user", helper.app.block_info().time.seconds())
        .unwrap();
    assert_eq!(user_vp, 0.0);

    // Must always be 0
    let user_vp = helper.query_user_vp_at_period("user", period).unwrap();
    assert_eq!(user_vp, 0.0);

    // Emissions voting power must be 90
    let total_emissions_vp_at_ts = helper
        .query_total_emissions_vp_at(helper.app.block_info().time.seconds())
        .unwrap();
    assert_eq!(total_emissions_vp_at_ts, 90.0);

    let user_emissions_vp = helper.query_user_emissions_vp("user").unwrap();
    assert_eq!(user_emissions_vp, 90.0);

    let user_emissions_vp = helper
        .query_user_emissions_vp_at("user", helper.app.block_info().time.seconds())
        .unwrap();
    assert_eq!(user_emissions_vp, 90.0);

    // Check users' locked xASTRO balance history
    helper.mint_xastro("user", 90);
    // SnapshotMap checkpoints the data at the next block
    let start_time = Uint64::from(helper.app.block_info().time.seconds() + 1);

    let balance_timestamp = helper.query_locked_balance_at("user", start_time).unwrap();
    assert_eq!(balance_timestamp, 90f32);

    helper.app.update_block(next_block);
    helper.extend_lock_amount("user", 100f32).unwrap();

    let balance_timestamp = helper.query_locked_balance_at("user", start_time).unwrap();
    assert_eq!(balance_timestamp, 90f32);

    helper.app.update_block(|bi| {
        bi.height += 100000;
        bi.time = bi.time.plus_seconds(500000);
    });

    let balance_timestamp = helper.query_locked_balance_at("user", start_time).unwrap();
    assert_eq!(balance_timestamp, 90f32);

    let balance_timestamp = helper
        .query_locked_balance_at(
            "user",
            start_time.saturating_add(Uint64::from(10u64)), // Next block adds 5 seconds
        )
        .unwrap();
    assert_eq!(balance_timestamp, 190f32);

    // The user still has 190 xASTRO locked
    let balance_timestamp = helper
        .query_locked_balance_at(
            "user",
            Uint64::from(helper.app.block_info().time.seconds()), // Next block adds 5 seconds
        )
        .unwrap();
    assert_eq!(balance_timestamp, 190f32);

    helper.app.update_block(|bi| {
        bi.height += 1;
        bi.time = bi.time.plus_seconds(WEEK * 102);
    });
    helper.unlock("user").unwrap();

    // Ensure emissions voting power is 0 after unlock
    let user_emissions_vp = helper
        .query_user_emissions_vp_at("user", helper.app.block_info().time.seconds())
        .unwrap();
    assert_eq!(user_emissions_vp, 0.0);

    // Forward until after unlock period ends
    helper.app.update_block(|bi| {
        bi.height += 1;
        bi.time = bi.time.plus_seconds(WEEK * 102);
    });
    // Withdraw
    helper.withdraw("user").unwrap();

    // Now the users' balance is zero
    // But one block before it had 190 xASTRO locked
    let balance_timestamp = helper
        .query_locked_balance_at(
            "user",
            Uint64::from(helper.app.block_info().time.seconds() + 5), // Next block adds 5 seconds
        )
        .unwrap();
    assert_eq!(balance_timestamp, 0f32);

    let balance_timestamp = helper
        .query_locked_balance_at(
            "user",
            Uint64::from(helper.app.block_info().time.seconds() - 5), // Next block adds 5 seconds
        )
        .unwrap();
    assert_eq!(balance_timestamp, 190f32);

    // add users to the blacklist
    helper
        .update_blacklist(
            vec![
                "voter1".to_string(),
                "voter2".to_string(),
                "voter3".to_string(),
                "voter4".to_string(),
                "voter5".to_string(),
                "voter6".to_string(),
                "voter7".to_string(),
                "voter8".to_string(),
            ],
            vec![],
        )
        .unwrap();

    // query all blacklisted voters
    let blacklisted_voters = helper.query_blacklisted_voters(None, None).unwrap();
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
        .query_blacklisted_voters(Some("voter9".to_string()), Some(10u32))
        .unwrap_err();
    assert_eq!(
        StdError::generic_err(
            "Querier contract error: Generic error: The voter9 address is not blacklisted"
        ),
        err
    );

    // query voters by specified parameters
    let blacklisted_voters = helper
        .query_blacklisted_voters(Some("voter2".to_string()), Some(2u32))
        .unwrap();
    assert_eq!(
        blacklisted_voters,
        vec![Addr::unchecked("voter3"), Addr::unchecked("voter4")]
    );

    // add users to the blacklist
    helper
        .update_blacklist(vec!["voter0".to_string(), "voter33".to_string()], vec![])
        .unwrap();

    // query voters by specified parameters
    let blacklisted_voters = helper
        .query_blacklisted_voters(Some("voter2".to_string()), Some(2u32))
        .unwrap();
    assert_eq!(
        blacklisted_voters,
        vec![Addr::unchecked("voter3"), Addr::unchecked("voter33")]
    );

    let blacklisted_voters = helper
        .query_blacklisted_voters(Some("voter4".to_string()), Some(10u32))
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
        .query_blacklisted_voters(Some("voter8".to_string()), Some(10u32))
        .unwrap();
    assert_eq!(blacklisted_voters, empty_blacklist);

    // check if voters are blacklisted
    let res = helper
        .check_voters_are_blacklisted(vec!["voter1".to_string(), "voter9".to_string()])
        .unwrap();
    assert_eq!("Voter is not blacklisted: voter9", res.to_string());

    let res = helper
        .check_voters_are_blacklisted(vec!["voter1".to_string(), "voter8".to_string()])
        .unwrap();
    assert_eq!("Voters are blacklisted!", res.to_string());
}

#[test]
fn check_deposit_for() {
    let mut helper = Helper::init();
    helper.mint_xastro("owner", 100);

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro("user1", 100);
    helper.check_xastro_balance("user1", 100);
    helper.mint_xastro("user2", 100);
    helper.check_xastro_balance("user2", 100);

    // 104 weeks ~ 2 years
    helper.create_lock("user1", 50f32).unwrap();
    let vp = helper.query_user_vp("user1").unwrap();
    assert_eq!(0.0, vp);
    let vp = helper.query_user_emissions_vp("user1").unwrap();
    assert_eq!(50.0, vp);

    helper.deposit_for("user2", "user1", 50f32).unwrap();
    let vp = helper.query_user_vp("user1").unwrap();
    assert_eq!(0.0, vp);
    let vp = helper.query_user_emissions_vp("user1").unwrap();
    assert_eq!(100.0, vp);
    helper.check_xastro_balance("user1", 50);
    helper.check_xastro_balance("user2", 50);
}

#[test]
fn check_update_owner() {
    let mut helper = Helper::init();

    let new_owner = String::from("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        new_owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // Unauthed check
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("not_owner"),
            helper.vxastro.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.vxastro.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    helper
        .app
        .execute_contract(Addr::unchecked("owner"), helper.vxastro.clone(), &msg, &[])
        .unwrap();

    // Claim from invalid addr
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.vxastro.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim ownership
    helper
        .app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.vxastro.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap();

    // Let's query the contract state
    let msg = QueryMsg::Config {};
    let res: Config = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.vxastro, &msg)
        .unwrap();

    assert_eq!(res.owner, new_owner)
}

#[test]
fn check_blacklist() {
    let mut helper = Helper::init();

    // Mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro("user1", 100);
    helper.mint_xastro("user2", 100);
    helper.mint_xastro("user3", 100);

    // Try to execute with empty arrays
    let err = helper.update_blacklist(vec![], vec![]).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Append and remove arrays are empty"
    );

    // Blacklisting user2
    let res = helper
        .update_blacklist(vec!["user2".to_string()], vec![])
        .unwrap();
    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "update_blacklist")
    );
    assert_eq!(
        res.events[1].attributes[2],
        attr("added_addresses", "user2")
    );

    helper.create_lock("user1", 50f32).unwrap();
    // Try to create lock from a blacklisted address
    let err = helper.create_lock("user2", 100f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user2 address is blacklisted"
    );
    let err = helper.deposit_for("user2", "user3", 50f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user2 address is blacklisted"
    );

    // Since user2 is blacklisted, their xASTRO balance was left unchanged
    helper.check_xastro_balance("user2", 100);
    // And they did not create a lock, thus we have no information to query
    let vp = helper.query_user_vp("user2").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_emissions_vp("user2").unwrap();
    assert_eq!(vp, 0.0);

    // Go to the future
    helper.app.update_block(next_block);
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(2 * WEEK));

    // user2 is still blacklisted
    let err = helper.create_lock("user2", 100f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user2 address is blacklisted"
    );

    // Blacklisting user1 using the guardian
    let msg = ExecuteMsg::UpdateBlacklist {
        append_addrs: vec!["user1".to_string()],
        remove_addrs: vec![],
    };
    let res = helper
        .app
        .execute_contract(
            Addr::unchecked("guardian"),
            helper.vxastro.clone(),
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

    let err = helper.extend_lock_amount("user1", 10f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user1 address is blacklisted"
    );
    let err = helper.deposit_for("user2", "user1", 50f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user2 address is blacklisted"
    );
    let err = helper.deposit_for("user3", "user1", 50f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The user1 address is blacklisted"
    );
    // user1 doesn't have voting power now
    let vp = helper.query_user_vp("user1").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_emissions_vp("user1").unwrap();
    assert_eq!(vp, 0.0);
    // Voting
    let vp = helper
        .query_user_vp_at("user1", helper.app.block_info().time.seconds() - WEEK)
        .unwrap();
    assert_eq!(vp, 0f32);
    // Total voting power should be zero as well since there was only one vxASTRO position created by user1
    let vp = helper.query_total_vp().unwrap();
    assert_eq!(vp, 0.0);
    // Total emissions voting power should be zero as well since there was only one vxASTRO position created by user1
    let vp = helper.query_total_emissions_vp().unwrap();
    assert_eq!(vp, 0.0);

    // The only option available for a blacklisted user is to unlock and withdraw their funds
    helper.unlock("user1").unwrap();

    // Go to the future
    helper.app.update_block(next_block);
    helper
        .app
        .update_block(|block| block.time = block.time.plus_seconds(20 * WEEK));

    // The only option available for a blacklisted user is to withdraw their funds
    helper.withdraw("user1").unwrap();

    // Remove user1 from the blacklist
    let res = helper
        .update_blacklist(vec![], vec!["user1".to_string()])
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
    helper.create_lock("user1", 10f32).unwrap();
}

#[test]
fn check_residual() {
    let mut helper = Helper::init();
    let users_num = 1000;
    let lock_amount = 100_000_000;

    helper.mint_xastro("owner", 100);

    for i in 1..(users_num / 2) {
        let user = &format!("user{}", i);
        helper.mint_xastro(user, 100);
        helper.create_lock_u128(user, lock_amount).unwrap();
    }

    let mut sum = 0;
    for i in 1..=users_num {
        let user = &format!("user{}", i);
        sum += helper.query_exact_user_vp(user).unwrap();
    }

    assert_eq!(sum, helper.query_exact_total_vp().unwrap());

    let mut sum = 0;
    for i in 1..=users_num {
        let user = &format!("user{}", i);
        sum += helper.query_exact_user_emissions_vp(user).unwrap();
    }

    assert_eq!(sum, helper.query_exact_total_emissions_vp().unwrap());

    helper.app.update_block(|bi| {
        bi.height += 1;
        bi.time = bi.time.plus_seconds(WEEK);
    });

    for i in (users_num / 2)..users_num {
        let user = &format!("user{}", i);
        helper.mint_xastro(user, 1000000);
        helper.create_lock_u128(user, lock_amount).unwrap();
    }

    for _ in 1..104 {
        sum = 0;
        for i in 1..=users_num {
            let user = &format!("user{}", i);
            sum += helper.query_exact_user_vp(user).unwrap();
        }

        let ve_vp = helper.query_exact_total_vp().unwrap();
        let diff = (sum as f64 - ve_vp as f64).abs();
        assert_eq!(diff, 0.0, "diff: {}, sum: {}, ve_vp: {}", diff, sum, ve_vp);

        helper.app.update_block(|bi| {
            bi.height += 1;
            bi.time = bi.time.plus_seconds(WEEK);
        });
    }

    for _ in 1..104 {
        sum = 0;
        for i in 1..=users_num {
            let user = &format!("user{}", i);
            sum += helper.query_exact_user_emissions_vp(user).unwrap();
        }

        let ve_vp = helper.query_exact_total_emissions_vp().unwrap();
        let diff = (sum as f64 - ve_vp as f64).abs();
        assert_eq!(diff, 0.0, "diff: {}, sum: {}, ve_vp: {}", diff, sum, ve_vp);

        helper.app.update_block(|bi| {
            bi.height += 1;
            bi.time = bi.time.plus_seconds(WEEK);
        });
    }
}

#[test]
fn total_vp_multiple_slope_subtraction() {
    let mut helper = Helper::init();

    helper.mint_xastro("user1", 1000);
    helper.create_lock("user1", 100f32).unwrap();
    let total = helper.query_total_vp().unwrap();
    assert_eq!(total, 0.0);
    let total = helper.query_total_emissions_vp().unwrap();
    assert_eq!(total, 100.0);

    helper
        .app
        .update_block(|bi| bi.time = bi.time.plus_seconds(2 * WEEK));
    // Slope changes have been applied
    let total = helper.query_total_vp().unwrap();
    assert_eq!(total, 0.0);
    let total = helper.query_total_emissions_vp().unwrap();
    assert_eq!(total, 100.0);

    helper.unlock("user1").unwrap();

    // Try to manipulate over expired lock 3 weeks later
    helper
        .app
        .update_block(|bi| bi.time = bi.time.plus_seconds(3 * WEEK));

    let err = helper.extend_lock_amount("user1", 100f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "The lock expired. Withdraw and create new lock"
    );

    let err = helper.create_lock("user1", 100f32).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Lock already exists, either unlock and withdraw or extend_lock to add to the lock"
    );

    let total = helper.query_total_vp().unwrap();
    assert_eq!(total, 0f32);
    let total = helper.query_total_emissions_vp().unwrap();
    assert_eq!(total, 0f32);
}

#[test]
fn marketing_info() {
    let mut helper = Helper::init();

    let err = helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.vxastro.clone(),
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

    let err = helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.vxastro.clone(),
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

    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.vxastro.clone(),
            &ExecuteMsg::SetLogoUrlsWhitelist {
                whitelist: vec!["example.com/".to_string()],
            },
            &[],
        )
        .unwrap();

    let err = helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.vxastro.clone(),
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

    let err = helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.vxastro.clone(),
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

    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.vxastro.clone(),
            &ExecuteMsg::UpdateMarketing {
                project: Some("Some project".to_string()),
                description: Some("Some description".to_string()),
                marketing: None,
            },
            &[],
        )
        .unwrap();

    let config: Config = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.vxastro, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.logo_urls_whitelist, vec!["example.com/".to_string()]);
    let marketing_info: MarketingInfoResponse = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.vxastro, &QueryMsg::MarketingInfo {})
        .unwrap();
    assert_eq!(marketing_info.project, Some("Some project".to_string()));
    assert_eq!(
        marketing_info.description,
        Some("Some description".to_string())
    );

    let err = helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.vxastro.clone(),
            &ExecuteMsg::UploadLogo(Logo::Url("https://some-website.com/logo.svg".to_string())),
            &[],
        )
        .unwrap_err();
    assert_eq!(
        &err.root_cause().to_string(),
        "Marketing info validation error: Logo link is not whitelisted: https://some-website.com/logo.svg",
    );

    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.vxastro.clone(),
            &ExecuteMsg::UploadLogo(Logo::Url("example.com/logo.svg".to_string())),
            &[],
        )
        .unwrap();

    let marketing_info: MarketingInfoResponse = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.vxastro, &QueryMsg::MarketingInfo {})
        .unwrap();
    assert_eq!(
        marketing_info.logo.unwrap(),
        LogoInfo::Url("example.com/logo.svg".to_string())
    );
}
