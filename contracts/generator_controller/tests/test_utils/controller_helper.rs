use crate::test_utils::escrow_helper::EscrowHelper;
use anyhow::Result as AnyResult;
use astroport_governance::generator_controller::{ExecuteMsg, QueryMsg};
use cosmwasm_std::{Addr, StdResult};
use generator_controller::state::{UserInfo, VotedPoolInfo};
use terra_multi_test::{AppResponse, ContractWrapper, Executor, TerraApp};

pub struct ControllerHelper {
    pub owner: String,
    pub generator: Addr,
    pub controller: Addr,
    pub escrow_helper: EscrowHelper,
}

impl ControllerHelper {
    pub fn init(router: &mut TerraApp, owner: &Addr) -> Self {
        let escrow_helper = EscrowHelper::init(router, owner.clone());

        let generator_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_generator::contract::execute,
            astroport_generator::contract::instantiate,
            astroport_generator::contract::query,
        ));

        let generator_code_id = router.store_code(generator_contract);
        let init_msg = astroport::generator::InstantiateMsg {
            owner: owner.to_string(),
            astro_token: escrow_helper.astro_token.to_string(),
            tokens_per_block: Default::default(),
            start_block: Default::default(),
            allowed_reward_proxies: vec![],
            vesting_contract: "vesting_placeholder".to_string(),
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

        Self {
            owner: owner.to_string(),
            generator,
            controller,
            escrow_helper,
        }
    }

    pub fn vote(
        &self,
        router: &mut TerraApp,
        user: &str,
        votes: Vec<(String, u16)>,
    ) -> AnyResult<AppResponse> {
        let msg = ExecuteMsg::Vote {
            votes: votes
                .into_iter()
                .map(|(pool, apoints)| (pool, apoints))
                .collect(),
        };

        router.execute_contract(Addr::unchecked(user), self.controller.clone(), &msg, &[])
    }

    pub fn gauge(&self, router: &mut TerraApp, sender: &str) -> AnyResult<AppResponse> {
        router.execute_contract(
            Addr::unchecked(sender),
            self.controller.clone(),
            &ExecuteMsg::GaugePools {},
            &[],
        )
    }

    pub fn query_user_info(&self, router: &mut TerraApp, user: &str) -> StdResult<UserInfo> {
        router.wrap().query_wasm_smart(
            self.controller.clone(),
            &QueryMsg::UserInfo {
                user: user.to_string(),
            },
        )
    }

    pub fn query_voted_pool_info(
        &self,
        router: &mut TerraApp,
        pool: &str,
    ) -> StdResult<VotedPoolInfo> {
        router.wrap().query_wasm_smart(
            self.controller.clone(),
            &QueryMsg::PoolInfo {
                pool_addr: pool.to_string(),
            },
        )
    }

    pub fn query_voted_pool_info_at_period(
        &self,
        router: &mut TerraApp,
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
}
