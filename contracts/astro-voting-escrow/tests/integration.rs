use astroport::{staking as xastro, token as astro};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, QueryRequest, StdResult, Uint128, WasmQuery};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use terra_multi_test::{
    next_block, AppBuilder, AppResponse, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock,
};

use anyhow::Result;
use astroport_governance::astro_voting_escrow::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, UsersResponse, VotingPowerResponse,
};

use astroport_voting_escrow::contract::{MAX_LOCK_TIME, WEEK};

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

struct Helper {
    pub owner: Addr,
    pub astro_token: Addr,
    pub staking_instance: Addr,
    pub xastro_token: Addr,
    pub voting_instance: Addr,
}

impl Helper {
    pub fn init(router: &mut TerraApp, owner: Addr) -> Self {
        let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_token::contract::execute,
            astroport_token::contract::instantiate,
            astroport_token::contract::query,
        ));

        let astro_token_code_id = router.store_code(astro_token_contract);

        let msg = astro::InstantiateMsg {
            name: String::from("Astro token"),
            symbol: String::from("ASTRO"),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: owner.to_string(),
                cap: None,
            }),
        };

        let astro_token = router
            .instantiate_contract(
                astro_token_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("ASTRO"),
                None,
            )
            .unwrap();

        let staking_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_staking::contract::execute,
                astroport_staking::contract::instantiate,
                astroport_staking::contract::query,
            )
            .with_reply_empty(astroport_staking::contract::reply),
        );

        let staking_code_id = router.store_code(staking_contract);

        let msg = xastro::InstantiateMsg {
            token_code_id: astro_token_code_id,
            deposit_token_addr: astro_token.to_string(),
        };
        let staking_instance = router
            .instantiate_contract(
                staking_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("xASTRO"),
                None,
            )
            .unwrap();

        let res = router
            .wrap()
            .query::<xastro::ConfigResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: staking_instance.to_string(),
                msg: to_binary(&xastro::QueryMsg::Config {}).unwrap(),
            }))
            .unwrap();

        let voting_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_voting_escrow::contract::execute,
            astroport_voting_escrow::contract::instantiate,
            astroport_voting_escrow::contract::query,
        ));

        let voting_code_id = router.store_code(voting_contract);

        let msg = InstantiateMsg {
            deposit_token_addr: res.share_token_addr.to_string(),
        };
        let voting_instance = router
            .instantiate_contract(
                voting_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("vxASTRO"),
                None,
            )
            .unwrap();

        Self {
            owner,
            xastro_token: res.share_token_addr,
            astro_token,
            staking_instance,
            voting_instance,
        }
    }

    pub fn mint_xastro(&self, router: &mut TerraApp, to: &str, amount: u64) {
        let msg = cw20::Cw20ExecuteMsg::Mint {
            recipient: String::from(to),
            amount: Uint128::from(amount),
        };
        let res = router
            .execute_contract(self.owner.clone(), self.astro_token.clone(), &msg, &[])
            .unwrap();
        assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
        assert_eq!(res.events[1].attributes[2], attr("to", String::from(to)));
        assert_eq!(
            res.events[1].attributes[3],
            attr("amount", Uint128::from(amount))
        );

        let to_addr = Addr::unchecked(to);
        let msg = Cw20ExecuteMsg::Send {
            contract: self.staking_instance.to_string(),
            msg: to_binary(&xastro::Cw20HookMsg::Enter {}).unwrap(),
            amount: Uint128::from(amount),
        };
        router
            .execute_contract(to_addr, self.astro_token.clone(), &msg, &[])
            .unwrap();
    }

    pub fn check_xastro_balance(&self, router: &mut TerraApp, user: &str, amount: u64) {
        let res: BalanceResponse = router
            .wrap()
            .query_wasm_smart(
                self.xastro_token.clone(),
                &Cw20QueryMsg::Balance {
                    address: user.to_string(),
                },
            )
            .unwrap();
        assert_eq!(res.balance.u128(), amount as u128);
    }

    pub fn create_lock(
        &self,
        router: &mut TerraApp,
        user: &str,
        time: u64,
        amount: u128,
    ) -> Result<AppResponse> {
        let cw20msg = Cw20ExecuteMsg::Send {
            contract: self.voting_instance.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::CreateLock { time }).unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(user),
            self.xastro_token.clone(),
            &cw20msg,
            &[],
        )
    }

    pub fn extend_lock_amount(
        &self,
        router: &mut TerraApp,
        user: &str,
        amount: u128,
    ) -> Result<AppResponse> {
        let cw20msg = Cw20ExecuteMsg::Send {
            contract: self.voting_instance.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::ExtendLockAmount {}).unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(user),
            self.xastro_token.clone(),
            &cw20msg,
            &[],
        )
    }

    pub fn extend_lock_time(
        &self,
        router: &mut TerraApp,
        user: &str,
        time: u64,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.voting_instance.clone(),
            &ExecuteMsg::ExtendLockTime { time },
            &[],
        )
    }

    pub fn withdraw(&self, router: &mut TerraApp, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.voting_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
    }

    pub fn query_user_vp(
        &self,
        router: &mut TerraApp,
        user: &str,
    ) -> StdResult<VotingPowerResponse> {
        router.wrap().query_wasm_smart(
            self.voting_instance.clone(),
            &QueryMsg::UserVotingPower {
                user: user.to_string(),
            },
        )
    }

    pub fn query_total_vp(&self, router: &mut TerraApp) -> StdResult<VotingPowerResponse> {
        self.query_user_vp(router, &self.voting_instance.to_string())
    }
}

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
    assert_eq!(res.to_string(), "Overflow: Cannot Sub with 100 and 101");

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
    helper.extend_lock_amount(router_ref, "user", 10).unwrap();
    helper.check_xastro_balance(router_ref, "user", 0);
    helper.check_xastro_balance(router_ref, helper.voting_instance.as_str(), 100);

    // trying to withdraw from non-expired lock
    let res = helper.withdraw(router_ref, "user").unwrap_err();
    assert_eq!(res.to_string(), "The lock time has not yet expired");

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));

    // but still lock has not yet expired since we locked for 2 weeks
    let res = helper.withdraw(router_ref, "user").unwrap_err();
    assert_eq!(res.to_string(), "The lock time has not yet expired");

    // going to the future again
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));

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
    assert_eq!(vp.voting_power.u128(), 30);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp.voting_power.u128(), 30);

    // since user2 did not lock his xASTRO the contract does not have any information
    let err = helper.query_user_vp(router_ref, "user2").unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Querier contract error: Generic error: User is not found"
    );

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));

    // create lock for user2
    helper
        .create_lock(router_ref, "user2", WEEK * 6, 50)
        .unwrap();

    let res: UsersResponse = router_ref
        .wrap()
        .query_wasm_smart(helper.voting_instance.clone(), &QueryMsg::Users {})
        .unwrap();
    assert_eq!(vec!["user", "user2"], res.users);

    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp.voting_power.u128(), 15);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp.voting_power.u128(), 50);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp.voting_power.u128(), 65);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 5));
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp.voting_power.u128(), 0);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp.voting_power.u128(), 8);
    let vp = helper.query_total_vp(router_ref).unwrap();
    // assert_eq!(vp.voting_power.u128(), 8);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp.voting_power.u128(), 0);
    let vp = helper.query_total_vp(router_ref).unwrap();
    assert_eq!(vp.voting_power.u128(), 0);
}

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
    assert_eq!(vp.voting_power.u128(), 65);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK * 4));

    helper.extend_lock_amount(router_ref, "user", 70).unwrap();
    helper
        .extend_lock_time(router_ref, "user2", WEEK * 10)
        .unwrap();
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp.voting_power.u128(), 73);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp.voting_power.u128(), 17);
    let vp = helper.query_total_vp(router_ref).unwrap();
    // assert_eq!(vp.voting_power.u128(), 90);

    // going to the future
    router_ref.update_block(next_block);
    router_ref.update_block(|block| block.time = block.time.plus_seconds(WEEK));
    let vp = helper.query_user_vp(router_ref, "user").unwrap();
    assert_eq!(vp.voting_power.u128(), 0);
    let vp = helper.query_user_vp(router_ref, "user2").unwrap();
    assert_eq!(vp.voting_power.u128(), 16);
    let vp = helper.query_total_vp(router_ref).unwrap();
    // assert_eq!(vp.voting_power.u128(), 16);
}
