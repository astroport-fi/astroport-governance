mod test_utils;

use crate::test_utils::{mock_app, Helper, MULTIPLIER};
use astroport::token as astro;
use astroport_governance::astro_voting_escrow::{
    Cw20HookMsg, LockInfoResponse, QueryMsg, UsersResponse,
};
use astroport_voting_escrow::contract::{MAX_LOCK_TIME, WEEK};
use cosmwasm_std::{to_binary, Addr, Decimal, Uint128};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use std::str::FromStr;
use terra_multi_test::{next_block, ContractWrapper, Executor};

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
        .create_lock(router_ref, "user", WEEK - 1, 1)
        .unwrap_err();
    assert_eq!(
        res.to_string(),
        "Lock time must be within the limits (week <= lock time < 2 years)"
    );
    let res = helper
        .create_lock(router_ref, "user", MAX_LOCK_TIME + 1, 1)
        .unwrap_err();
    assert_eq!(
        res.to_string(),
        "Lock time must be within the limits (week <= lock time < 2 years)"
    );
    let res = helper
        .create_lock(router_ref, "user", WEEK, 101)
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
        .extend_lock_amount(router_ref, "user", 1)
        .unwrap_err();
    assert_eq!(res.to_string(), "Lock does not exist");

    // current total voting power is 0
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 0.0);

    // creating valid voting escrow lock
    helper
        .create_lock(router_ref, "user", WEEK * 2, 90)
        .unwrap();
    // check that 90 xASTRO were actually debited
    helper.check_xastro_balance(router_ref, "user", 10);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 90);

    // a user can have only one position in vxASTRO
    let res = helper
        .create_lock(router_ref, "user", MAX_LOCK_TIME, 1)
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
    helper.extend_lock_amount(router_ref, "user", 9).unwrap();
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
        .extend_lock_amount(router_ref, "user", 1)
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

    // time has passed so we can withdraw
    helper.withdraw(router_ref, "user").unwrap();
    helper.check_xastro_balance(router_ref, "user", 100);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 0);

    // check that the lock has disappeared
    let res = helper
        .extend_lock_amount(router_ref, "user", 1)
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
        .create_lock(router_ref, "user", WEEK * 5, 50)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 6.00961);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 6.00961);

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
        .create_lock(router_ref, "user", WEEK * 5, 100)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 12.01923);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 12.01923);
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
        .create_lock(router_ref, "user", WEEK * 10, 30)
        .unwrap();

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 7.21153);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 7.21153);

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
    assert_eq!(res, 4.32692);
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() - 3 * WEEK,
        )
        .unwrap();
    // TODO: assert_eq!(res, 5.76923);
    let res = helper
        .query_total_vp_at(
            router_ref,
            router_ref.block_info().time.seconds() - 5 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 7.21153);

    // and even in the future
    let res = helper
        .query_user_vp_at(
            router_ref,
            "user",
            router_ref.block_info().time.seconds() + WEEK,
        )
        .unwrap();
    assert_eq!(res, 2.88461);
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
        .create_lock(router_ref, "user2", WEEK * 6, 50)
        .unwrap();

    // check that we have locks from "user" and "user2"
    let res: UsersResponse = router_ref
        .wrap()
        .query_wasm_smart(helper.voting_instance.clone(), &QueryMsg::Users {})
        .unwrap();
    assert_eq!(vec!["user", "user2"], res.users);

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 3.60576);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 7.21153);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 10.81729);
    let res = helper
        .query_total_vp_at(
            router_ref,
            router_ref.block_info().time.seconds() + 4 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 3.12499);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 1.20192);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 1.20192);

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
        .create_lock(router_ref, "user", WEEK * 10, 30)
        .unwrap();

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));

    // create lock for user2
    helper
        .create_lock(router_ref, "user2", WEEK * 6, 50)
        .unwrap();
    let vp = helper.query_total_vp(router_ref).unwrap();
    // TODO: assert_eq!(vp, 10.8173);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 4));

    helper.extend_lock_amount(router_ref, "user", 70).unwrap();
    helper
        .extend_lock_time(router_ref, "user2", WEEK * 10)
        .unwrap();
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 2.40384);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 2.40384);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 4.80768);

    let res = helper
        .query_user_vp_at(
            router_ref,
            "user2",
            router_ref.block_info().time.seconds() + 4 * WEEK,
        )
        .unwrap();
    assert_eq!(res, 1.4423);
    let res = helper
        .query_total_vp_at(router_ref, router_ref.block_info().time.seconds() + WEEK)
        .unwrap();
    assert_eq!(res, 2.16346);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp, 0.0);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp, 2.16346);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp, 2.16346);
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
        .create_lock(router_ref, "user", WEEK * 2, 90)
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
    assert!(
        user_lock.boost - Decimal::from_str("0.048076").unwrap()
            < Decimal::from_str("0.000001").unwrap()
    )
}
