use cosmwasm_std::{
    Addr, Binary, Coin, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdResult, Uint128,
};
use cw20::Logo;
use cw_multi_test::error::AnyResult;
use cw_multi_test::{AppResponse, BankSudo, BasicApp, Contract, ContractWrapper, Executor};

use astroport_governance::emissions_controller;
use astroport_governance::voting_escrow::{
    ExecuteMsg, InstantiateMsg, LockInfoResponse, QueryMsg, UpdateMarketingInfo,
};

fn vxastro_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_voting_escrow::contract::execute,
        astroport_voting_escrow::contract::instantiate,
        astroport_voting_escrow::contract::query,
    ))
}

fn mock_emissions_controller() -> Box<dyn Contract<Empty>> {
    fn instantiate(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: Empty,
    ) -> StdResult<Response> {
        Ok(Response::default())
    }
    fn execute(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: emissions_controller::msg::ExecuteMsg<Empty>,
    ) -> StdResult<Response> {
        Ok(Response::default())
    }

    fn query(_deps: Deps, _env: Env, _msg: Empty) -> StdResult<Binary> {
        unimplemented!()
    }

    Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
}

pub struct EscrowHelper {
    pub app: BasicApp,
    pub owner: Addr,
    pub xastro_denom: String,
    pub vxastro_contract: Addr,
    pub emissions_controller: Addr,
}

impl EscrowHelper {
    pub fn new(xastro_denom: &str) -> Self {
        let mut app = BasicApp::default();
        let owner = Addr::unchecked("owner");

        let vxastro_code_id = app.store_code(vxastro_contract());
        let emissions_controller_code_id = app.store_code(mock_emissions_controller());
        let mocked_emission_controller = app
            .instantiate_contract(
                emissions_controller_code_id,
                owner.clone(),
                &Empty {},
                &[],
                "label",
                None,
            )
            .unwrap();
        let vxastro_contract = app
            .instantiate_contract(
                vxastro_code_id,
                owner.clone(),
                &InstantiateMsg {
                    deposit_denom: xastro_denom.to_string(),
                    emissions_controller: mocked_emission_controller.to_string(),
                    marketing: Some(UpdateMarketingInfo {
                        project: None,
                        description: None,
                        marketing: Some(owner.to_string()),
                        logo: Some(Logo::Url("https://example.com".to_string())),
                    }),
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        Self {
            app,
            owner,
            xastro_denom: xastro_denom.to_string(),
            vxastro_contract,
            emissions_controller: mocked_emission_controller,
        }
    }

    pub fn mint_tokens(&mut self, user: &Addr, coins: &[Coin]) -> AnyResult<AppResponse> {
        self.app.sudo(
            BankSudo::Mint {
                to_address: user.to_string(),
                amount: coins.to_vec(),
            }
            .into(),
        )
    }

    pub fn lock(&mut self, user: &Addr, coins: &[Coin]) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.vxastro_contract.clone(),
            &ExecuteMsg::Lock { receiver: None },
            coins,
        )
    }

    pub fn unlock(&mut self, user: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.vxastro_contract.clone(),
            &ExecuteMsg::Unlock {},
            &[],
        )
    }

    pub fn relock(&mut self, user: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.vxastro_contract.clone(),
            &ExecuteMsg::Relock {},
            &[],
        )
    }

    pub fn confirm_unlock(&mut self, user: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            self.emissions_controller.clone(),
            self.vxastro_contract.clone(),
            &ExecuteMsg::ConfirmUnlock {
                user: user.to_string(),
            },
            &[],
        )
    }

    pub fn withdraw(&mut self, user: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.vxastro_contract.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
    }

    pub fn timetravel(&mut self, time: u64) {
        self.app.update_block(|block| {
            block.time = block.time.plus_seconds(time);
        })
    }

    pub fn user_vp(&self, user: &Addr, timestamp: Option<u64>) -> StdResult<Uint128> {
        self.app.wrap().query_wasm_smart(
            &self.vxastro_contract,
            &QueryMsg::UserVotingPower {
                user: user.to_string(),
                timestamp,
            },
        )
    }

    pub fn total_vp(&self, timestamp: Option<u64>) -> StdResult<Uint128> {
        self.app.wrap().query_wasm_smart(
            &self.vxastro_contract,
            &QueryMsg::TotalVotingPower { timestamp },
        )
    }

    pub fn lock_info(&self, user: &Addr) -> StdResult<LockInfoResponse> {
        self.app.wrap().query_wasm_smart(
            &self.vxastro_contract,
            &QueryMsg::LockInfo {
                user: user.to_string(),
            },
        )
    }
}
