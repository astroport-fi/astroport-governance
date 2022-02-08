mod test_utils;

use crate::test_utils::{mock_app, Helper, MULTIPLIER};
use astroport::token as astro;
use astroport_governance::voting_escrow::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, LockInfoResponse, QueryMsg,
};
use cosmwasm_std::{attr, to_binary, Addr, Fraction, Uint128};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use terra_multi_test::{next_block, ContractWrapper, Executor};
use voting_escrow::contract::{MAX_LOCK_TIME, WEEK};

#[test]
fn lock_unlock_logic() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    // mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user", 100);
    helper.check_xastro_balance(router_ref, "user", 100);

    // creating invalid voting escrow lock
    let res = helper
        .create_lock(router_ref, "user", WEEK - 1, 1f32)
        .unwrap_err();
    assert_eq!(
        res.to_string(),
        "Lock time must be within the limits (week <= lock time < 2 years)"
    );
    let res = helper
        .create_lock(router_ref, "user", MAX_LOCK_TIME + 1, 1f32)
        .unwrap_err();
    assert_eq!(
        res.to_string(),
        "Lock time must be within the limits (week <= lock time < 2 years)"
    );
    let res = helper
        .create_lock(router_ref, "user", WEEK, 101f32)
        .unwrap_err();
    assert_eq!(
        res.to_string(),
        format!(
            "Overflow: Cannot Sub with {} and {}",
            100 * MULTIPLIER,
            101 * MULTIPLIER
        )
    );

    // trying to increase lock's time which does not exist
    let res = helper
        .extend_lock_time(router_ref, "user", MAX_LOCK_TIME)
        .unwrap_err();
    assert_eq!(res.to_string(), "Lock does not exist");

    // trying to withdraw from non-existent lock
    let res = helper.withdraw(router_ref, "user").unwrap_err();
    assert_eq!(res.to_string(), "Lock does not exist");

    // trying to extend lock amount which does not exist
    let res = helper
        .extend_lock_amount(router_ref, "user", 1f32)
        .unwrap_err();
    assert_eq!(res.to_string(), "Lock does not exist");

    // current total voting power is 0
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 0.0);

    // creating valid voting escrow lock
    helper
        .create_lock(router_ref, "user", WEEK * 2, 90f32)
        .unwrap();
    // check that 90 xASTRO were actually debited
    helper.check_xastro_balance(router_ref, "user", 10);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 90);

    // a user can have only one position in vxASTRO
    let res = helper
        .create_lock(router_ref, "user", MAX_LOCK_TIME, 1f32)
        .unwrap_err();
    assert_eq!(res.to_string(), "Lock already exists");

    // trying to increase lock time by time less than a week
    let res = helper
        .extend_lock_time(router_ref, "user", 86400)
        .unwrap_err();
    assert_eq!(
        res.to_string(),
        "Lock time must be within the limits (week <= lock time < 2 years)"
    );

    // trying to exceed MAX_LOCK_TIME by increasing lock time
    // we locked for 2 weeks so increasing by MAX_LOCK_TIME - week is impossible
    let res = helper
        .extend_lock_time(router_ref, "user", MAX_LOCK_TIME - WEEK)
        .unwrap_err();
    assert_eq!(
        res.to_string(),
        "Lock time must be within the limits (week <= lock time < 2 years)"
    );

    // adding more xASTRO to existing lock
    helper.extend_lock_amount(router_ref, "user", 9f32).unwrap();
    helper.check_xastro_balance(router_ref, "user", 1);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 99);

    // trying to withdraw from non-expired lock
    let res = helper.withdraw(router_ref, "user").unwrap_err();
    assert_eq!(res.to_string(), "The lock time has not yet expired");

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));

    // but still the lock has not yet expired since we locked for 2 weeks
    let res = helper.withdraw(router_ref, "user").unwrap_err();
    assert_eq!(res.to_string(), "The lock time has not yet expired");

    // going to the future again
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));

    // trying to add more xASTRO to expired lock
    let res = helper
        .extend_lock_amount(router_ref, "user", 1f32)
        .unwrap_err();
    assert_eq!(
        res.to_string(),
        "The lock expired. Withdraw and create new lock"
    );
    // trying to increase lock time for expired lock
    let res = helper
        .extend_lock_time(router_ref, "user", WEEK)
        .unwrap_err();
    assert_eq!(
        res.to_string(),
        "The lock expired. Withdraw and create new lock"
    );

    // imagine the user will withdraw his expired lock in 5 weeks
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(5 * WEEK));

    // time has passed so we can withdraw
    helper.withdraw(router_ref, "user").unwrap();
    helper.check_xastro_balance(router_ref, "user", 100);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 0);

    // check that the lock has disappeared
    let res = helper
        .extend_lock_amount(router_ref, "user", 1f32)
        .unwrap_err();
    assert_eq!(res.to_string(), "Lock does not exist");
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
    let res = router
        .execute_contract(Addr::unchecked("user"), random_token, &cw20msg, &[])
        .unwrap_err();

    assert_eq!(res.to_string(), "Unauthorized");
}

#[test]
fn new_lock_after_lock_expired() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let helper = Helper::init(router_ref, Addr::unchecked("owner"));

    // mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user", 100);

    helper
        .create_lock(router_ref, "user", WEEK * 5, 50f32)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 53.60576);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 53.60576);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));

    helper.withdraw(router_ref, "user").unwrap();
    helper.check_xastro_balance(router_ref, "user", 100);

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 0.0);

    // creating a new lock in 3 weeks
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

/// Plot for this case tests/plots/constant_decay.png
#[test]
fn voting_constant_decay() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let helper = Helper::init(router_ref, Addr::unchecked("owner"));

    // mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user", 100);
    helper.mint_xastro(router_ref, "user2", 50);

    helper
        .create_lock(router_ref, "user", WEEK * 10, 30f32)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 34.32692);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 34.32692);

    // since user2 did not lock his xASTRO the contract does not have any information
    let err = helper.query_user_vp(router_ref, "user2").unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Generic error: User is not found"
    );

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));

    // we can check voting power in the past
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() - WEEK,
        )
        .unwrap();
    assert_eq!(res, 20.59615);
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() - 3 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 27.46154);
    let res = helper
        .query_total_vp_at(
            router_ref,
            router_ref.block_info().time.seconds() - 5 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 34.32692);

    // and even in the future
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() + WEEK,
        )
        .unwrap();
    assert_eq!(res, 13.73077);
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() + 5 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 0.0);

    // create lock for user2
    helper
        .create_lock(router_ref, "user2", WEEK * 6, 50f32)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 17.16346);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 54.32692);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 71.49038);
    let res = helper
        .query_total_vp_at(
            router_ref,
            router_ref.block_info().time.seconds() + 4 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 21.54167);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 9.05449);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 9.05449);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 0.0);
}

/// Plot for this case tests/plots/variable_decay.png
#[test]
fn voting_variable_decay() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let helper = Helper::init(router_ref, Addr::unchecked("owner"));

    // mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user", 100);
    helper.mint_xastro(router_ref, "user2", 100);

    helper
        .create_lock(router_ref, "user", WEEK * 10, 30f32)
        .unwrap();

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));

    // create lock for user2
    helper
        .create_lock(router_ref, "user2", WEEK * 6, 50f32)
        .unwrap();
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 71.49038);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 4));

    helper
        .extend_lock_amount(router_ref, "user", 70f32)
        .unwrap();
    helper
        .extend_lock_time(router_ref, "user2", WEEK * 8)
        .unwrap();
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 74.4423);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 18.10897);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 92.55128);

    let res = helper
        .query_user_vp_at(
            router_ref,
            "user2",
            router_ref.block_info().time.seconds() + 4 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 10.86538);
    let res = helper
        .query_total_vp_at(router_ref, router_ref.block_info().time.seconds() + WEEK)
        .unwrap();
    assert_eq!(res, 16.29808);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 16.29807);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 16.29808);
}

#[test]
fn check_queries() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    // mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user", 100);
    helper.check_xastro_balance(router_ref, "user", 100);

    // creating valid voting escrow lock
    helper
        .create_lock(router_ref, "user", WEEK * 2, 90f32)
        .unwrap();
    // check that 90 xASTRO were actually debited
    helper.check_xastro_balance(router_ref, "user", 10);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 90);

    // validating user's lock
    let cur_period = router_ref.block_info().time.seconds() / WEEK;
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
    let coeff =
        user_lock.coefficient.numerator() as f32 / user_lock.coefficient.denominator() as f32;
    if (coeff - 1.02884f32).abs() > 1e-5 {
        assert_eq!(coeff, 1.02884f32)
    }
}

#[test]
fn check_deposit_for() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(router_ref, owner);

    // mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user1", 100);
    helper.check_xastro_balance(router_ref, "user1", 100);
    helper.mint_xastro(router_ref, "user2", 100);
    helper.check_xastro_balance(router_ref, "user2", 100);

    // 104 weeks ~ 2 years
    helper
        .create_lock(router_ref, "user1", 104 * WEEK, 50f32)
        .unwrap();
    let vp = helper.query_user_vp(router_ref, "user1").unwrap();
    assert_eq!(125.0, vp);
    helper
        .deposit_for(router_ref, "user2", "user1", 50f32)
        .unwrap();
    let vp = helper.query_user_vp(router_ref, "user1").unwrap();
    assert_eq!(250.0, vp);
    helper.check_xastro_balance(router_ref, "user1", 50);
    helper.check_xastro_balance(router_ref, "user2", 50);
}

#[test]
fn check_update_owner() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = Helper::init(&mut app, owner);

    let new_owner = String::from("new_owner");

    // new owner
    let msg = ExecuteMsg::ProposeNewOwner {
        new_owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // unauthorized check
    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            helper.voting_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.voting_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Ownership proposal not found"
    );

    // propose new owner
    app.execute_contract(
        Addr::unchecked("owner"),
        helper.voting_instance.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.voting_instance.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        helper.voting_instance.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // let's query the state
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

    // mint ASTRO, stake it and mint xASTRO
    helper.mint_xastro(router_ref, "user1", 100);
    helper.mint_xastro(router_ref, "user2", 100);
    helper.mint_xastro(router_ref, "user3", 100);

    let msg = ExecuteMsg::UpdateBlacklist {
        append_addrs: None,
        remove_addrs: None,
    };
    // trying to execute with empty arrays
    let err = router_ref
        .execute_contract(
            Addr::unchecked("owner"),
            helper.voting_instance.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Append and remove arrays are empty"
    );

    let msg = ExecuteMsg::UpdateBlacklist {
        append_addrs: Some(vec!["user2".to_string()]),
        remove_addrs: None,
    };
    // blacklisting user2
    let res = router_ref
        .execute_contract(
            Addr::unchecked("owner"),
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
        attr("added_addresses", "user2")
    );

    helper
        .create_lock(router_ref, "user1", WEEK * 10, 50f32)
        .unwrap();
    // trying to create lock from blacklisted address
    let err = helper
        .create_lock(router_ref, "user2", WEEK * 10, 100f32)
        .unwrap_err();
    assert_eq!(err.to_string(), "The user2 address is blacklisted");
    let err = helper
        .deposit_for(router_ref, "user2", "user3", 50f32)
        .unwrap_err();
    assert_eq!(err.to_string(), "The user2 address is blacklisted");

    // since user2 is blacklisted his xASTRO balance left unchanged
    helper.check_xastro_balance(router_ref, "user2", 100);
    // and he did not create lock in voting escrow thus we have no information
    let err = helper.query_user_vp(router_ref, "user2").unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Generic error: User is not found"
    );

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(2 * WEEK));

    // user2 is still blacklisted
    let err = helper
        .create_lock(router_ref, "user2", WEEK * 10, 100f32)
        .unwrap_err();
    assert_eq!(err.to_string(), "The user2 address is blacklisted");

    // blacklisting user1
    let msg = ExecuteMsg::UpdateBlacklist {
        append_addrs: Some(vec!["user1".to_string()]),
        remove_addrs: None,
    };
    let res = router_ref
        .execute_contract(
            Addr::unchecked("owner"),
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
    assert_eq!(err.to_string(), "The user1 address is blacklisted");
    let err = helper
        .extend_lock_amount(router_ref, "user1", 10f32)
        .unwrap_err();
    assert_eq!(err.to_string(), "The user1 address is blacklisted");
    let err = helper
        .deposit_for(router_ref, "user2", "user1", 50f32)
        .unwrap_err();
    assert_eq!(err.to_string(), "The user2 address is blacklisted");
    let err = helper
        .deposit_for(router_ref, "user3", "user1", 50f32)
        .unwrap_err();
    assert_eq!(err.to_string(), "The user1 address is blacklisted");
    // But still he has voting power
    // TODO: should we nullify his voting power?
    let vp = helper.query_user_vp(router_ref, "user1").unwrap();
    assert!(vp > 0.0);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(20 * WEEK));

    // the only option available for blacklisted user is to withdraw funds if lock expired
    helper.withdraw(router_ref, "user1").unwrap();

    // removing user1 from blacklist
    let msg = ExecuteMsg::UpdateBlacklist {
        append_addrs: None,
        remove_addrs: Some(vec!["user1".to_string()]),
    };
    let res = router_ref
        .execute_contract(
            Addr::unchecked("owner"),
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
        attr("removed_addresses", "user1")
    );

    // now user1 can create new lock
    helper
        .create_lock(router_ref, "user1", WEEK, 10f32)
        .unwrap();
}
