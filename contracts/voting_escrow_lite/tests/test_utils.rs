use anyhow::Result;
use astroport::{staking as xastro, token as astro};
use astroport_governance::utils::EPOCH_START;
use astroport_governance::voting_escrow_lite::{
    BlacklistedVotersResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg,
    UpdateMarketingInfo, VotingPowerResponse,
};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{
    attr, to_json_binary, Addr, QueryRequest, StdResult, Timestamp, Uint128, Uint64, WasmQuery,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Logo, MinterResponse};
use cw_multi_test::{App, AppBuilder, AppResponse, BankKeeper, ContractWrapper, Executor};
use voting_escrow_lite::astroport;

pub const MULTIPLIER: u64 = 1000000;

pub struct Helper {
    pub owner: Addr,
    pub astro_token: Addr,
    pub staking_instance: Addr,
    pub xastro_token: Addr,
    pub voting_instance: Addr,
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
            marketing: None,
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
                msg: to_json_binary(&xastro::QueryMsg::Config {}).unwrap(),
            }))
            .unwrap();

        let generator_controller = Box::new(ContractWrapper::new_with_empty(
            astroport_generator_controller::contract::execute,
            astroport_generator_controller::contract::instantiate,
            astroport_generator_controller::contract::query,
        ));

        let generator_controller_id = router.store_code(generator_controller);

        let msg = astroport_governance::generator_controller_lite::InstantiateMsg {
            owner: owner.to_string(),
            assembly_addr: "assembly".to_string(),
            escrow_addr: "contract4".to_string(),
            factory_addr: "factory".to_string(),
            generator_addr: "generator".to_string(),
            hub_addr: None,
            pools_limit: 10,
            whitelisted_pools: vec![],
        };
        let generator_controller_instance = router
            .instantiate_contract(
                generator_controller_id,
                owner.clone(),
                &msg,
                &[],
                String::from("Generator Controller Lite"),
                None,
            )
            .unwrap();

        let voting_contract = Box::new(ContractWrapper::new_with_empty(
            voting_escrow_lite::execute::execute,
            voting_escrow_lite::contract::instantiate,
            voting_escrow_lite::query::query,
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
            logo_urls_whitelist: vec!["https://astroport.com/".to_string()],
            generator_controller_addr: Some(generator_controller_instance.to_string()),
            outpost_addr: None,
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
            msg: to_json_binary(&xastro::Cw20HookMsg::Enter {}).unwrap(),
            amount: Uint128::from(amount),
        };
        router
            .execute_contract(to_addr, self.astro_token.clone(), &msg, &[])
            .unwrap();
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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
            msg: to_json_binary(&Cw20HookMsg::CreateLock { time }).unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(user),
            self.xastro_token.clone(),
            &cw20msg,
            &[],
        )
    }

    #[allow(dead_code)]
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
            msg: to_json_binary(&Cw20HookMsg::CreateLock { time }).unwrap(),
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
            msg: to_json_binary(&Cw20HookMsg::ExtendLockAmount {}).unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(user),
            self.xastro_token.clone(),
            &cw20msg,
            &[],
        )
    }

    #[allow(dead_code)]
    pub fn relock(&self, router: &mut App, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked("outpost"),
            self.voting_instance.clone(),
            &ExecuteMsg::Relock {
                user: user.to_string(),
            },
            &[],
        )
    }

    #[allow(dead_code)]
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
            msg: to_json_binary(&Cw20HookMsg::DepositFor {
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

    #[allow(dead_code)]
    pub fn unlock(&self, router: &mut App, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.voting_instance.clone(),
            &ExecuteMsg::Unlock {},
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

    #[allow(dead_code)]
    pub fn update_outpost_address(
        &self,
        router: &mut App,
        new_address: String,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked("owner"),
            self.voting_instance.clone(),
            &ExecuteMsg::UpdateConfig {
                new_guardian: None,
                generator_controller: None,
                outpost: Some(new_address),
            },
            &[],
        )
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn query_user_emissions_vp(&self, router: &mut App, user: &str) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::UserEmissionsVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn query_exact_user_emissions_vp(&self, router: &mut App, user: &str) -> StdResult<u128> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::UserEmissionsVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn query_user_emissions_vp_at(
        &self,
        router: &mut App,
        user: &str,
        time: u64,
    ) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::UserEmissionsVotingPowerAt {
                    user: user.to_string(),
                    time,
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn query_total_vp(&self, router: &mut App) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(self.voting_instance.clone(), &QueryMsg::TotalVotingPower {})
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    #[allow(dead_code)]
    pub fn query_total_emissions_vp(&self, router: &mut App) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::TotalEmissionsVotingPower {},
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    #[allow(dead_code)]
    pub fn query_exact_total_vp(&self, router: &mut App) -> StdResult<u128> {
        router
            .wrap()
            .query_wasm_smart(self.voting_instance.clone(), &QueryMsg::TotalVotingPower {})
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    #[allow(dead_code)]
    pub fn query_exact_total_emissions_vp(&self, router: &mut App) -> StdResult<u128> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::TotalEmissionsVotingPower {},
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    #[allow(dead_code)]
    pub fn query_total_vp_at(&self, router: &mut App, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::TotalVotingPowerAt { time },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_emissions_vp_at(&self, router: &mut App, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::TotalEmissionsVotingPowerAt { time },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    #[allow(dead_code)]
    pub fn query_total_vp_at_period(&self, router: &mut App, period: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::TotalVotingPowerAtPeriod { period },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    #[allow(dead_code)]
    pub fn query_total_emissions_vp_at_period(
        &self,
        router: &mut App,
        timestamp: u64,
    ) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::TotalEmissionsVotingPowerAt { time: timestamp },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    #[allow(dead_code)]
    pub fn query_locked_balance_at(
        &self,
        router: &mut App,
        user: &str,
        timestamp: Uint64,
    ) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_instance.clone(),
                &QueryMsg::UserDepositAt {
                    user: user.to_string(),
                    timestamp,
                },
            )
            .map(|vp: Uint128| vp.u128() as f32 / MULTIPLIER as f32)
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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
