use crate::escrow_helper::EscrowHelper;
use anyhow::Result as AnyResult;
use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport_governance::generator_controller::{ConfigResponse, ExecuteMsg, QueryMsg};
use cosmwasm_std::{Addr, Decimal, StdResult};
use cw_multi_test::{App, AppResponse, ContractWrapper, Executor};
use generator_controller::state::{UserInfo, VotedPoolInfo};

pub struct ControllerHelper {
    pub owner: String,
    pub generator: Addr,
    pub controller: Addr,
    pub factory: Addr,
    pub escrow_helper: EscrowHelper,
}

impl ControllerHelper {
    pub fn init(router: &mut App, owner: &Addr) -> Self {
        let escrow_helper = EscrowHelper::init(router, owner.clone());

        let pair_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_pair::contract::execute,
                astroport_pair::contract::instantiate,
                astroport_pair::contract::query,
            )
            .with_reply_empty(astroport_pair::contract::reply),
        );

        let pair_code_id = router.store_code(pair_contract);

        let factory_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_factory::contract::execute,
                astroport_factory::contract::instantiate,
                astroport_factory::contract::query,
            )
            .with_reply_empty(astroport_factory::contract::reply),
        );

        let factory_code_id = router.store_code(factory_contract);

        let whitelist_code_id = store_whitelist_code(router);

        let msg = astroport::factory::InstantiateMsg {
            pair_configs: vec![PairConfig {
                code_id: pair_code_id,
                pair_type: PairType::Xyk {},
                total_fee_bps: 100,
                maker_fee_bps: 10,
                is_disabled: false,
                is_generator_disabled: false,
            }],
            token_code_id: escrow_helper.astro_token_code_id,
            fee_address: None,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id,
        };

        let factory = router
            .instantiate_contract(factory_code_id, owner.clone(), &msg, &[], "Factory", None)
            .unwrap();

        let generator_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_generator::contract::execute,
                astroport_generator::contract::instantiate,
                astroport_generator::contract::query,
            )
            .with_reply_empty(astroport_generator::contract::reply),
        );

        let generator_code_id = router.store_code(generator_contract);
        let init_msg = astroport::generator::InstantiateMsg {
            owner: owner.to_string(),
            factory: factory.to_string(),
            generator_controller: None,
            guardian: None,
            astro_token: escrow_helper.astro_token.to_string(),
            tokens_per_block: Default::default(),
            start_block: Default::default(),
            allowed_reward_proxies: vec![],
            vesting_contract: "vesting_placeholder".to_string(),
            whitelist_code_id,
            voting_escrow: None,
        };

        let generator = router
            .instantiate_contract(
                generator_code_id,
                owner.clone(),
                &init_msg,
                &[],
                String::from("Generator"),
                None,
            )
            .unwrap();

        let controller_contract = Box::new(ContractWrapper::new_with_empty(
            generator_controller::contract::execute,
            generator_controller::contract::instantiate,
            generator_controller::contract::query,
        ));

        let controller_code_id = router.store_code(controller_contract);
        let init_msg = astroport_governance::generator_controller::InstantiateMsg {
            owner: owner.to_string(),
            escrow_addr: escrow_helper.escrow_instance.to_string(),
            generator_addr: generator.to_string(),
            factory_addr: factory.to_string(),
            pools_limit: 5,
        };

        let controller = router
            .instantiate_contract(
                controller_code_id,
                owner.clone(),
                &init_msg,
                &[],
                String::from("Controller"),
                None,
            )
            .unwrap();

        // Setup controller in generator contract
        router
            .execute_contract(
                owner.clone(),
                generator.clone(),
                &astroport::generator::ExecuteMsg::UpdateConfig {
                    vesting_contract: None,
                    generator_controller: Some(controller.to_string()),
                    guardian: None,
                    checkpoint_generator_limit: None,
                    voting_escrow: None,
                },
                &[],
            )
            .unwrap();

        Self {
            owner: owner.to_string(),
            generator,
            controller,
            factory,
            escrow_helper,
        }
    }

    pub fn init_cw20_token(&self, router: &mut App, name: &str) -> AnyResult<Addr> {
        let msg = astroport::token::InstantiateMsg {
            name: name.to_string(),
            symbol: name.to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: None,
            marketing: None,
        };

        router.instantiate_contract(
            self.escrow_helper.astro_token_code_id,
            Addr::unchecked(self.owner.clone()),
            &msg,
            &[],
            name.to_string(),
            None,
        )
    }

    pub fn create_pool(&self, router: &mut App, token1: &Addr, token2: &Addr) -> AnyResult<Addr> {
        let asset_infos = [
            AssetInfo::Token {
                contract_addr: token1.clone(),
            },
            AssetInfo::Token {
                contract_addr: token2.clone(),
            },
        ];

        router.execute_contract(
            Addr::unchecked(self.owner.clone()),
            self.factory.clone(),
            &astroport::factory::ExecuteMsg::CreatePair {
                pair_type: PairType::Xyk {},
                asset_infos: asset_infos.clone(),
                init_params: None,
            },
            &[],
        )?;

        let res: PairInfo = router.wrap().query_wasm_smart(
            self.factory.clone(),
            &astroport::factory::QueryMsg::Pair { asset_infos },
        )?;

        Ok(res.liquidity_token)
    }

    pub fn create_pool_with_tokens(
        &self,
        router: &mut App,
        name1: &str,
        name2: &str,
    ) -> AnyResult<Addr> {
        let token1 = self.init_cw20_token(router, name1).unwrap();
        let token2 = self.init_cw20_token(router, name2).unwrap();

        self.create_pool(router, &token1, &token2)
    }

    pub fn vote(
        &self,
        router: &mut App,
        user: &str,
        votes: Vec<(impl Into<String>, u16)>,
    ) -> AnyResult<AppResponse> {
        let msg = ExecuteMsg::Vote {
            votes: votes
                .into_iter()
                .map(|(pool, apoints)| (pool.into(), apoints))
                .collect(),
        };

        router.execute_contract(Addr::unchecked(user), self.controller.clone(), &msg, &[])
    }

    pub fn tune(&self, router: &mut App) -> AnyResult<AppResponse> {
        router.execute_contract(
            Addr::unchecked("anyone"),
            self.controller.clone(),
            &ExecuteMsg::TunePools {},
            &[],
        )
    }

    pub fn kick_holders(
        &self,
        router: &mut App,
        user: &str,
        blacklisted_voters: Vec<String>,
    ) -> AnyResult<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.controller.clone(),
            &ExecuteMsg::KickBlacklistedVoters { blacklisted_voters },
            &[],
        )
    }

    pub fn update_blacklisted_limit(
        &self,
        router: &mut App,
        user: &str,
        blacklisted_voters_limit: Option<u32>,
    ) -> AnyResult<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.controller.clone(),
            &ExecuteMsg::UpdateConfig {
                blacklisted_voters_limit,
                main_pool: None,
                main_pool_min_alloc: None,
                remove_main_pool: None,
            },
            &[],
        )
    }

    pub fn update_main_pool(
        &self,
        router: &mut App,
        user: &str,
        main_pool: Option<&Addr>,
        main_pool_min_alloc: Option<Decimal>,
        remove_main_pool: bool,
    ) -> AnyResult<AppResponse> {
        let remove_main_pool = if remove_main_pool { Some(true) } else { None };
        router.execute_contract(
            Addr::unchecked(user),
            self.controller.clone(),
            &ExecuteMsg::UpdateConfig {
                blacklisted_voters_limit: None,
                main_pool: main_pool.map(|p| p.to_string()),
                main_pool_min_alloc,
                remove_main_pool,
            },
            &[],
        )
    }

    pub fn query_user_info(&self, router: &mut App, user: &str) -> StdResult<UserInfo> {
        router.wrap().query_wasm_smart(
            self.controller.clone(),
            &QueryMsg::UserInfo {
                user: user.to_string(),
            },
        )
    }

    pub fn query_voted_pool_info(&self, router: &mut App, pool: &str) -> StdResult<VotedPoolInfo> {
        router.wrap().query_wasm_smart(
            self.controller.clone(),
            &QueryMsg::PoolInfo {
                pool_addr: pool.to_string(),
            },
        )
    }

    pub fn query_voted_pool_info_at_period(
        &self,
        router: &mut App,
        pool: &str,
        period: u64,
    ) -> StdResult<VotedPoolInfo> {
        router.wrap().query_wasm_smart(
            self.controller.clone(),
            &QueryMsg::PoolInfoAtPeriod {
                pool_addr: pool.to_string(),
                period,
            },
        )
    }

    pub fn query_config(&self, router: &mut App) -> StdResult<ConfigResponse> {
        router
            .wrap()
            .query_wasm_smart(self.controller.clone(), &QueryMsg::Config {})
    }
}

fn store_whitelist_code(app: &mut App) -> u64 {
    let whitelist_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_whitelist::contract::execute,
        astroport_whitelist::contract::instantiate,
        astroport_whitelist::contract::query,
    ));

    app.store_code(whitelist_contract)
}
