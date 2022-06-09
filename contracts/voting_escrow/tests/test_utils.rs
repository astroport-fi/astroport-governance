use anyhow::Result;
use astroport::{staking as xastro, token as astro};
use astroport_governance::escrow_fee_distributor::InstantiateMsg as FeeDistributorInstantiateMsg;
use astroport_governance::utils::EPOCH_START;
use astroport_governance::voting_escrow::{
    BlacklistedVotersResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg,
    UpdateMarketingInfo, VotingPowerResponse,
};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{
    attr, to_binary, Addr, Decimal, QueryRequest, StdResult, Timestamp, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Logo, MinterResponse};
use cw_multi_test::{App, AppBuilder, AppResponse, BankKeeper, ContractWrapper, Executor};
use std::str::FromStr;
use voting_escrow::astroport;

pub const MULTIPLIER: u64 = 1000000;

pub struct Helper {
    pub owner: Addr,
    pub astro_token: Addr,
    pub staking_instance: Addr,
    pub xastro_token: Addr,
    pub voting_instance: Addr,
    pub fee_distributor_instance: Addr,
}

impl Helper {
    pub fn init(router: &mut App, owner: Addr) -> Self {
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
            marketing: None,
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
            owner: owner.to_string(),
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
            voting_escrow::contract::execute,
            voting_escrow::contract::instantiate,
            voting_escrow::contract::query,
        ));

        let voting_code_id = router.store_code(voting_contract);

        let marketing_info = UpdateMarketingInfo {
            project: Some("Astroport".to_string()),
            description: Some("Astroport is a decentralized application for managing the supply of space resources.".to_string()),
            marketing: Some(owner.to_string()),
            logo: Some(Logo::Url("https://astroport.com/logo.png".to_string())),
        };

        let msg = InstantiateMsg {
            owner: owner.to_string(),
            guardian_addr: Some("guardian".to_string()),
            deposit_token_addr: res.share_token_addr.to_string(),
            marketing: Some(marketing_info),
            max_exit_penalty: Decimal::from_str("0.75").unwrap(),
            slashed_fund_receiver: None,
            logo_urls_whitelist: vec!["https://astroport.com/".to_string()],
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

        let fee_distributor_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_escrow_fee_distributor::contract::execute,
            astroport_escrow_fee_distributor::contract::instantiate,
            astroport_escrow_fee_distributor::contract::query,
        ));

        let fee_distributor_code_id = router.store_code(fee_distributor_contract);

        let msg = FeeDistributorInstantiateMsg {
            owner: owner.to_string(),
            astro_token: astro_token.to_string(),
            voting_escrow_addr: voting_instance.to_string(),
            claim_many_limit: None,
            is_claim_disabled: None,
        };

        let fee_distributor_instance = router
            .instantiate_contract(
                fee_distributor_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("Fee distributor"),
                None,
            )
            .unwrap();

        Self {
            owner,
            xastro_token: res.share_token_addr,
            astro_token,
            staking_instance,
            voting_instance,
            fee_distributor_instance,
        }
    }

    pub fn mint_xastro(&self, router: &mut App, to: &str, amount: u64) {
        let amount = amount * MULTIPLIER;
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

    pub fn check_xastro_balance(&self, router: &mut App, user: &str, amount: u64) {
        let amount = amount * MULTIPLIER;
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

    pub fn check_astro_balance(&self, router: &mut App, user: &str, amount: u64) {
        let amount = amount * MULTIPLIER;
        let res: BalanceResponse = router
            .wrap()
            .query_wasm_smart(
                self.astro_token.clone(),
                &Cw20QueryMsg::Balance {
                    address: user.to_string(),
                },
            )
            .unwrap();
        assert_eq!(res.balance.u128(), amount as u128);
    }

    pub fn create_lock(
        &self,
        router: &mut App,
        user: &str,
        time: u64,
        amount: f32,
    ) -> Result<AppResponse> {
        let amount = (amount * MULTIPLIER as f32) as u64;
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

    pub fn create_lock_u128(
        &self,
        router: &mut App,
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
        router: &mut App,
        user: &str,
        amount: f32,
    ) -> Result<AppResponse> {
        let amount = (amount * MULTIPLIER as f32) as u64;
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

    pub fn deposit_for(
        &self,
        router: &mut App,
        from: &str,
        to: &str,
        amount: f32,
    ) -> Result<AppResponse> {
        let amount = (amount * MULTIPLIER as f32) as u64;
        let cw20msg = Cw20ExecuteMsg::Send {
            contract: self.voting_instance.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::DepositFor {
                user: to.to_string(),
            })
            .unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(from),
            self.xastro_token.clone(),
            &cw20msg,
            &[],
        )
    }

    pub fn extend_lock_time(&self, router: &mut App, user: &str, time: u64) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.voting_instance.clone(),
            &ExecuteMsg::ExtendLockTime { time },
            &[],
        )
    }

    pub fn withdraw(&self, router: &mut App, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.voting_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
    }

    pub fn withdraw_early(&self, router: &mut App, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.voting_instance.clone(),
            &ExecuteMsg::WithdrawEarly {},
            &[],
        )
    }

    pub fn configure_early_withdrawal(
        &self,
        router: &mut App,
        max_penalty: &str,
        slashed_fund_receiver: &str,
    ) -> Result<AppResponse> {
        router.execute_contract(
            self.owner.clone(),
            self.voting_instance.clone(),
            &ExecuteMsg::ConfigureEarlyWithdrawal {
                max_penalty: Some(Decimal::from_str(max_penalty).unwrap()),
                slashed_fund_receiver: Some(slashed_fund_receiver.to_string()),
            },
            &[],
        )
    }

    pub fn update_blacklist(
        &self,
        router: &mut App,
        append_addrs: Option<Vec<String>>,
        remove_addrs: Option<Vec<String>>,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked("owner"),
            self.voting_instance.clone(),
            &ExecuteMsg::UpdateBlacklist {
                append_addrs,
                remove_addrs,
            },
            &[],
        )
    }

    pub fn query_user_vp(&self, router: &mut App, user: &str) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::UserVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_exact_user_vp(&self, router: &mut App, user: &str) -> StdResult<u128> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::UserVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_user_vp_at(&self, router: &mut App, user: &str, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::UserVotingPowerAt {
                    user: user.to_string(),
                    time,
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_user_vp_at_period(
        &self,
        router: &mut App,
        user: &str,
        period: u64,
    ) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::UserVotingPowerAtPeriod {
                    user: user.to_string(),
                    period,
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_vp(&self, router: &mut App) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(self.voting_instance.clone(), &QueryMsg::TotalVotingPower {})
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_exact_total_vp(&self, router: &mut App) -> StdResult<u128> {
        router
            .wrap()
            .query_wasm_smart(self.voting_instance.clone(), &QueryMsg::TotalVotingPower {})
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_total_vp_at(&self, router: &mut App, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::TotalVotingPowerAt { time },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_vp_at_period(&self, router: &mut App, period: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::TotalVotingPowerAtPeriod { period },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_early_withdraw_amount(&self, router: &mut App, user: &str) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::EarlyWithdrawAmount {
                    user: user.to_string(),
                },
            )
            .map(|amount: Uint128| amount.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_locked_balance_at(
        &self,
        router: &mut App,
        user: &str,
        height: u64,
    ) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::UserDepositAtHeight {
                    user: user.to_string(),
                    height,
                },
            )
            .map(|vp: Uint128| vp.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_blacklisted_voters(
        &self,
        router: &mut App,
        start_after: Option<String>,
        limit: Option<u32>,
    ) -> StdResult<Vec<Addr>> {
        router.wrap().query_wasm_smart(
            self.voting_instance.clone(),
            &QueryMsg::BlacklistedVoters { start_after, limit },
        )
    }

    pub fn check_voters_are_blacklisted(
        &self,
        router: &mut App,
        voters: Vec<String>,
    ) -> StdResult<BlacklistedVotersResponse> {
        router.wrap().query_wasm_smart(
            self.voting_instance.clone(),
            &QueryMsg::CheckVotersAreBlacklisted { voters },
        )
    }
}

pub fn mock_app() -> App {
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(EPOCH_START);
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .build(|_, _, _| {})
}
