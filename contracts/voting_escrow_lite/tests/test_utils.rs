#![allow(dead_code)]

use anyhow::Result;
use cosmwasm_std::{coins, Addr, BlockInfo, StdResult, Timestamp, Uint128, Uint64};
use cw20::Logo;
use cw_multi_test::{App, AppBuilder, AppResponse, ContractWrapper, Executor};

use astroport_governance::utils::EPOCH_START;
use astroport_governance::voting_escrow_lite::{
    BlacklistedVotersResponse, ExecuteMsg, InstantiateMsg, QueryMsg, UpdateMarketingInfo,
    VotingPowerResponse,
};

pub const MULTIPLIER: u128 = 1_000000;

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

    pub fn mint_xastro(&mut self, to: &str, amount: u128) {
        let amount = amount * MULTIPLIER;
        self.app
            .send_tokens(
                self.owner.clone(),
                Addr::unchecked(to),
                &coins(amount, XASTRO_DENOM),
            )
            .unwrap();
    }

    pub fn check_xastro_balance(&self, user: &str, amount: u128) {
        let amount = amount * MULTIPLIER;
        let balance = self
            .app
            .wrap()
            .query_balance(user, XASTRO_DENOM)
            .unwrap()
            .amount;
        assert_eq!(balance.u128(), amount);
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

    pub fn create_lock_u128(&mut self, user: &str, amount: u128) -> Result<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(user),
            self.vxastro.clone(),
            &ExecuteMsg::CreateLock {},
            &coins(amount, XASTRO_DENOM),
        )
    }

    pub fn extend_lock_amount(&mut self, user: &str, amount: f32) -> Result<AppResponse> {
        let amount = (amount * MULTIPLIER as f32) as u128;
        self.app.execute_contract(
            Addr::unchecked(user),
            self.vxastro.clone(),
            &ExecuteMsg::ExtendLockAmount {},
            &coins(amount, XASTRO_DENOM),
        )
    }

    pub fn relock(&mut self, user: &str) -> Result<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked("outpost"),
            self.vxastro.clone(),
            &ExecuteMsg::Relock {
                user: user.to_string(),
            },
            &[],
        )
    }

    pub fn deposit_for(&mut self, from: &str, to: &str, amount: f32) -> Result<AppResponse> {
        let amount = (amount * MULTIPLIER as f32) as u128;
        self.app.execute_contract(
            Addr::unchecked(from),
            self.vxastro.clone(),
            &ExecuteMsg::DepositFor {
                user: to.to_string(),
            },
            &coins(amount, XASTRO_DENOM),
        )
    }

    pub fn unlock(&mut self, user: &str) -> Result<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(user),
            self.vxastro.clone(),
            &ExecuteMsg::Unlock {},
            &[],
        )
    }

    pub fn withdraw(&mut self, user: &str) -> Result<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(user),
            self.vxastro.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
    }

    pub fn update_blacklist(
        &mut self,
        append_addrs: Vec<String>,
        remove_addrs: Vec<String>,
    ) -> Result<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked("owner"),
            self.vxastro.clone(),
            &ExecuteMsg::UpdateBlacklist {
                append_addrs,
                remove_addrs,
            },
            &[],
        )
    }

    pub fn update_outpost_address(&mut self, new_address: String) -> Result<AppResponse> {
        self.app.execute_contract(
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

    pub fn query_user_vp(&self, user: &str) -> StdResult<f32> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_user_emissions_vp(&self, user: &str) -> StdResult<f32> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserEmissionsVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_exact_user_vp(&self, user: &str) -> StdResult<u128> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_exact_user_emissions_vp(&self, user: &str) -> StdResult<u128> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::UserEmissionsVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_user_vp_at(&self, user: &str, time: u64) -> StdResult<f32> {
        self.app
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

    pub fn query_user_emissions_vp_at(&self, user: &str, time: u64) -> StdResult<f32> {
        self.app
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

    pub fn query_user_vp_at_period(&self, user: &str, period: u64) -> StdResult<f32> {
        self.app
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

    pub fn query_total_vp(&self) -> StdResult<f32> {
        self.app
            .wrap()
            .query_wasm_smart(self.vxastro.clone(), &QueryMsg::TotalVotingPower {})
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_emissions_vp(&self) -> StdResult<f32> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalEmissionsVotingPower {},
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_exact_total_vp(&self) -> StdResult<u128> {
        self.app
            .wrap()
            .query_wasm_smart(self.vxastro.clone(), &QueryMsg::TotalVotingPower {})
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_exact_total_emissions_vp(&self) -> StdResult<u128> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalEmissionsVotingPower {},
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128())
    }

    pub fn query_total_vp_at(&self, time: u64) -> StdResult<f32> {
        self.app
            .wrap()
            .query_wasm_smart(self.vxastro.clone(), &QueryMsg::TotalVotingPowerAt { time })
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_emissions_vp_at(&self, time: u64) -> StdResult<f32> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalEmissionsVotingPowerAt { time },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_vp_at_period(&self, period: u64) -> StdResult<f32> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalVotingPowerAtPeriod { period },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_emissions_vp_at_period(&self, timestamp: u64) -> StdResult<f32> {
        self.app
            .wrap()
            .query_wasm_smart(
                self.vxastro.clone(),
                &QueryMsg::TotalEmissionsVotingPowerAt { time: timestamp },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_locked_balance_at(&self, user: &str, timestamp: Uint64) -> StdResult<f32> {
        self.app
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
        start_after: Option<String>,
        limit: Option<u32>,
    ) -> StdResult<Vec<Addr>> {
        self.app.wrap().query_wasm_smart(
            self.vxastro.clone(),
            &QueryMsg::BlacklistedVoters { start_after, limit },
        )
    }

    pub fn check_voters_are_blacklisted(
        &self,
        voters: Vec<String>,
    ) -> StdResult<BlacklistedVotersResponse> {
        self.app.wrap().query_wasm_smart(
            self.vxastro.clone(),
            &QueryMsg::CheckVotersAreBlacklisted { voters },
        )
    }
}
