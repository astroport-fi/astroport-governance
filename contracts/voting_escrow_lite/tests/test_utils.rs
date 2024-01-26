#![allow(dead_code)]

use anyhow::Result;
use cosmwasm_std::{
    attr, coins, to_json_binary, Addr, BlockInfo, StdResult, Timestamp, Uint128, Uint64,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Logo};
use cw_multi_test::{App, AppBuilder, AppResponse, ContractWrapper, Executor};

use astroport_governance::utils::EPOCH_START;
use astroport_governance::voting_escrow_lite::{
    BlacklistedVotersResponse, ExecuteMsg, InstantiateMsg, QueryMsg, UpdateMarketingInfo,
    VotingPowerResponse,
};

pub const MULTIPLIER: u64 = 1_000000;

pub const XASTRO_DENOM: &str = "factory/assembly/xASTRO";

pub const OWNER: &str = "owner";

pub struct Helper {
    pub app: App,
    pub owner: Addr,
    pub vxastro: Addr,
    pub generator_controller: Addr,
}

impl Helper {
    pub fn init() -> Self {
        let owner = Addr::unchecked(OWNER);

        let mut app = AppBuilder::new()
            .with_block(BlockInfo {
                height: 1000,
                time: Timestamp::from_seconds(EPOCH_START),
                chain_id: "cw-multitest-1".to_string(),
            })
            .build(|router, _, storage| {
                router
                    .bank
                    .init_balance(storage, &owner, coins(u128::MAX, XASTRO_DENOM))
                    .unwrap()
            });

        let voting_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_voting_escrow_lite::execute::execute,
            astroport_voting_escrow_lite::contract::instantiate,
            astroport_voting_escrow_lite::query::query,
        ));

        let voting_code_id = app.store_code(voting_contract);

        let marketing_info = UpdateMarketingInfo {
            project: Some("Astroport".to_string()),
            description: Some("Astroport is a decentralized application for managing the supply of space resources.".to_string()),
            marketing: Some(owner.to_string()),
            logo: Some(Logo::Url("https://astroport.com/logo.png".to_string())),
        };

        let msg = InstantiateMsg {
            owner: owner.to_string(),
            guardian_addr: Some("guardian".to_string()),
            deposit_denom: XASTRO_DENOM.to_string(),
            marketing: Some(marketing_info),
            logo_urls_whitelist: vec!["https://astroport.com/".to_string()],
            generator_controller_addr: None,
            outpost_addr: None,
        };
        let vxastro = app
            .instantiate_contract(
                voting_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("vxASTRO"),
                None,
            )
            .unwrap();

        let generator_controller = Box::new(ContractWrapper::new_with_empty(
            astroport_generator_controller::contract::execute,
            astroport_generator_controller::contract::instantiate,
            astroport_generator_controller::contract::query,
        ));

        let generator_controller_id = app.store_code(generator_controller);

        let msg = astroport_governance::generator_controller_lite::InstantiateMsg {
            owner: owner.to_string(),
            assembly_addr: "assembly".to_string(),
            escrow_addr: vxastro.to_string(),
            factory_addr: "factory".to_string(),
            generator_addr: "generator".to_string(),
            hub_addr: None,
            pools_limit: 10,
            whitelisted_pools: vec![],
        };
        let generator_controller = app
            .instantiate_contract(
                generator_controller_id,
                owner.clone(),
                &msg,
                &[],
                String::from("Generator Controller Lite"),
                None,
            )
            .unwrap();

        app.execute_contract(
            owner.clone(),
            vxastro.clone(),
            &ExecuteMsg::UpdateConfig {
                new_guardian: None,
                generator_controller: Some(generator_controller.to_string()),
                outpost: None,
            },
            &[],
        )
        .unwrap();

        Self {
            app,
            owner,
            vxastro,
            generator_controller,
        }
    }

    pub fn mint_xastro(&mut self, to: &str, amount: impl Into<u128> + Copy) {
        self.app
            .send_tokens(
                self.owner.clone(),
                Addr::unchecked(to),
                &coins(amount.into(), XASTRO_DENOM),
            )
            .unwrap();
    }

    pub fn check_xastro_balance(&self, user: &str, amount: u64) {
        let amount = amount * MULTIPLIER;
        let balance = self
            .app
            .wrap()
            .query_balance(user, XASTRO_DENOM)
            .unwrap()
            .amount;
        assert_eq!(balance.u128(), amount as u128);
    }

    pub fn create_lock(&mut self, user: &str, amount: f32) -> Result<AppResponse> {
        let amount = (amount * MULTIPLIER as f32) as u128;
        self.app.execute_contract(
            Addr::unchecked(user),
            self.vxastro.clone(),
            &ExecuteMsg::CreateLock {},
            &coins(amount, XASTRO_DENOM),
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
            contract: self.vxastro.to_string(),
            amount: Uint128::from(amount),
            msg: to_json_binary(&Cw20HookMsg::CreateLock { time }).unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(user),
            self.xastro_denom.clone(),
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
            contract: self.vxastro.to_string(),
            amount: Uint128::from(amount),
            msg: to_json_binary(&Cw20HookMsg::ExtendLockAmount {}).unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(user),
            self.xastro_denom.clone(),
            &cw20msg,
            &[],
        )
    }

    pub fn relock(&self, router: &mut App, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked("outpost"),
            self.vxastro.clone(),
            &ExecuteMsg::Relock {
                user: user.to_string(),
            },
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
            contract: self.vxastro.to_string(),
            amount: Uint128::from(amount),
            msg: to_json_binary(&Cw20HookMsg::DepositFor {
                user: to.to_string(),
            })
            .unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(from),
            self.xastro_denom.clone(),
            &cw20msg,
            &[],
        )
    }

    pub fn unlock(&self, router: &mut App, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.vxastro.clone(),
            &ExecuteMsg::Unlock {},
            &[],
        )
    }

    pub fn withdraw(&self, router: &mut App, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.vxastro.clone(),
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
            self.vxastro.clone(),
            &ExecuteMsg::UpdateBlacklist {
                append_addrs,
                remove_addrs,
            },
            &[],
        )
    }

    pub fn update_outpost_address(
        &self,
        router: &mut App,
        new_address: String,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked("owner"),
            self.vxastro.clone(),
            &ExecuteMsg::UpdateConfig {
                new_guardian: None,
                generator_controller: None,
                outpost: Some(new_address),
            },
            &[],
        )
    }

    pub fn query_user_vp(&self, router: &mut App, user: &str) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_user_emissions_vp(&self, router: &mut App, user: &str) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserEmissionsVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_exact_user_vp(&self, router: &mut App, user: &str) -> StdResult<u128> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_exact_user_emissions_vp(&self, router: &mut App, user: &str) -> StdResult<u128> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserEmissionsVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_user_vp_at(&self, router: &mut App, user: &str, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserVotingPowerAt {
                    user: user.to_string(),
                    time,
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_user_emissions_vp_at(
        &self,
        router: &mut App,
        user: &str,
        time: u64,
    ) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserEmissionsVotingPowerAt {
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
                self.vxastro.clone(),
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
            .query_wasm_smart(self.vxastro.clone(), &QueryMsg::TotalVotingPower {})
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_emissions_vp(&self, router: &mut App) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalEmissionsVotingPower {},
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_exact_total_vp(&self, router: &mut App) -> StdResult<u128> {
        router
            .wrap()
            .query_wasm_smart(self.vxastro.clone(), &QueryMsg::TotalVotingPower {})
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_exact_total_emissions_vp(&self, router: &mut App) -> StdResult<u128> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalEmissionsVotingPower {},
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_total_vp_at(&self, router: &mut App, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(self.vxastro.clone(), &QueryMsg::TotalVotingPowerAt { time })
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_emissions_vp_at(&self, router: &mut App, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalEmissionsVotingPowerAt { time },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_vp_at_period(&self, router: &mut App, period: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalVotingPowerAtPeriod { period },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_emissions_vp_at_period(
        &self,
        router: &mut App,
        timestamp: u64,
    ) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalEmissionsVotingPowerAt { time: timestamp },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_locked_balance_at(
        &self,
        router: &mut App,
        user: &str,
        timestamp: Uint64,
    ) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserDepositAt {
                    user: user.to_string(),
                    timestamp,
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
            self.vxastro.clone(),
            &QueryMsg::BlacklistedVoters { start_after, limit },
        )
    }

    pub fn check_voters_are_blacklisted(
        &self,
        router: &mut App,
        voters: Vec<String>,
    ) -> StdResult<BlacklistedVotersResponse> {
        router.wrap().query_wasm_smart(
            self.vxastro.clone(),
            &QueryMsg::CheckVotersAreBlacklisted { voters },
        )
    }
}
