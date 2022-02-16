use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::staking;
use astroport::token::InstantiateMsg as AstroTokenInstantiateMsg;
use astroport_governance::escrow_fee_distributor::InstantiateMsg as EscrowFeeDistributorInstantiateMsg;
use astroport_governance::voting_escrow::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg as AstroVotingEscrowInstantiateMsg, QueryMsg,
    VotingPowerResponse,
};
use cosmwasm_std::{attr, to_binary, Addr, QueryRequest, StdResult, Uint128, WasmQuery};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use terra_multi_test::{AppResponse, ContractWrapper, Executor, TerraApp};

use anyhow::Result;

pub const MULTIPLIER: u64 = 1_000_000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ContractInfo {
    pub address: Addr,
    pub code_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct BaseAstroportTestPackage {
    pub owner: Addr,
    pub astro_token: Option<ContractInfo>,
    pub escrow_fee_distributor: Option<ContractInfo>,
    pub staking: Option<ContractInfo>,
    pub voting_escrow: Option<ContractInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct BaseAstroportTestInitMessage {
    pub owner: Addr,
    pub emergency_return: Addr,
    pub start_time: Option<u64>,
}

impl BaseAstroportTestPackage {
    pub fn init_all(router: &mut TerraApp, msg: BaseAstroportTestInitMessage) -> Self {
        let mut base_pack = BaseAstroportTestPackage {
            owner: msg.owner.clone(),
            astro_token: None,
            escrow_fee_distributor: None,
            staking: None,
            voting_escrow: None,
        };

        base_pack.init_astro_token(router, msg.owner.clone());
        base_pack.init_staking(router, msg.owner.clone());
        base_pack.init_voting_escrow(router, msg.owner.clone());
        base_pack.init_escrow_fee_distributor(
            router,
            msg.owner.clone(),
            msg.emergency_return,
            msg.start_time,
        );
        base_pack
    }

    fn init_astro_token(&mut self, router: &mut TerraApp, owner: Addr) {
        let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_token::contract::execute,
            astroport_token::contract::instantiate,
            astroport_token::contract::query,
        ));

        let astro_token_code_id = router.store_code(astro_token_contract);

        let init_msg = AstroTokenInstantiateMsg {
            name: String::from("Astro token"),
            symbol: String::from("ASTRO"),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: owner.to_string(),
                cap: None,
            }),
        };

        let astro_token_instance = router
            .instantiate_contract(
                astro_token_code_id,
                owner,
                &init_msg,
                &[],
                "Astro token",
                None,
            )
            .unwrap();

        self.astro_token = Some(ContractInfo {
            address: astro_token_instance,
            code_id: astro_token_code_id,
        })
    }

    fn init_staking(&mut self, router: &mut TerraApp, owner: Addr) {
        let staking_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_staking::contract::execute,
                astroport_staking::contract::instantiate,
                astroport_staking::contract::query,
            )
            .with_reply_empty(astroport_staking::contract::reply),
        );

        let staking_code_id = router.store_code(staking_contract);

        let msg = staking::InstantiateMsg {
            owner: owner.to_string(),
            token_code_id: self.astro_token.clone().unwrap().code_id,
            deposit_token_addr: self.astro_token.clone().unwrap().address.to_string(),
        };

        let staking_instance = router
            .instantiate_contract(
                staking_code_id,
                owner,
                &msg,
                &[],
                String::from("xASTRO"),
                None,
            )
            .unwrap();

        self.staking = Some(ContractInfo {
            address: staking_instance,
            code_id: staking_code_id,
        })
    }

    pub fn get_staking_xastro(&self, router: &TerraApp) -> Addr {
        let res = router
            .wrap()
            .query::<staking::ConfigResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.staking.clone().unwrap().address.to_string(),
                msg: to_binary(&staking::QueryMsg::Config {}).unwrap(),
            }))
            .unwrap();

        res.share_token_addr
    }

    fn init_voting_escrow(&mut self, router: &mut TerraApp, owner: Addr) {
        let voting_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_voting_escrow::contract::execute,
            astroport_voting_escrow::contract::instantiate,
            astroport_voting_escrow::contract::query,
        ));

        let voting_code_id = router.store_code(voting_contract);

        let msg = AstroVotingEscrowInstantiateMsg {
            guardian_addr: "guardian".to_string(),
            marketing: None,
            owner: owner.to_string(),
            deposit_token_addr: self.get_staking_xastro(router).to_string(),
        };

        let voting_instance = router
            .instantiate_contract(
                voting_code_id,
                owner,
                &msg,
                &[],
                String::from("vxASTRO"),
                None,
            )
            .unwrap();

        self.voting_escrow = Some(ContractInfo {
            address: voting_instance,
            code_id: voting_code_id,
        })
    }

    pub fn init_escrow_fee_distributor(
        &mut self,
        router: &mut TerraApp,
        owner: Addr,
        emergency_return: Addr,
        start_time: Option<u64>,
    ) {
        let escrow_fee_distributor_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_escrow_fee_distributor::contract::execute,
            astroport_escrow_fee_distributor::contract::instantiate,
            astroport_escrow_fee_distributor::contract::query,
        ));

        let escrow_fee_distributor_code_id = router.store_code(escrow_fee_distributor_contract);

        let init_msg = EscrowFeeDistributorInstantiateMsg {
            owner: owner.to_string(),
            astro_token: self.astro_token.clone().unwrap().address.to_string(),
            voting_escrow_addr: self.voting_escrow.clone().unwrap().address.to_string(),
            emergency_return_addr: emergency_return.to_string(),
            start_time: start_time.unwrap_or_default(),
        };

        let escrow_fee_distributor_instance = router
            .instantiate_contract(
                escrow_fee_distributor_code_id,
                owner,
                &init_msg,
                &[],
                "Astroport escrow fee distributor",
                None,
            )
            .unwrap();

        self.escrow_fee_distributor = Some(ContractInfo {
            address: escrow_fee_distributor_instance,
            code_id: escrow_fee_distributor_code_id,
        })
    }

    pub fn create_lock(
        &self,
        router: &mut TerraApp,
        user: Addr,
        time: u64,
        amount: u64,
    ) -> Result<AppResponse> {
        let amount = amount * MULTIPLIER;
        let cw20msg = Cw20ExecuteMsg::Send {
            contract: self.voting_escrow.clone().unwrap().address.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::CreateLock { time }).unwrap(),
        };

        router.execute_contract(user, self.get_staking_xastro(router), &cw20msg, &[])
    }

    pub fn extend_lock_amount(
        &mut self,
        router: &mut TerraApp,
        user: &str,
        amount: u64,
    ) -> Result<AppResponse> {
        let amount = amount * MULTIPLIER;
        let cw20msg = Cw20ExecuteMsg::Send {
            contract: self.voting_escrow.clone().unwrap().address.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::ExtendLockAmount {}).unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(user),
            self.get_staking_xastro(router),
            &cw20msg,
            &[],
        )
    }

    pub fn extend_lock_time(
        &mut self,
        router: &mut TerraApp,
        user: &str,
        time: u64,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.voting_escrow.clone().unwrap().address,
            &ExecuteMsg::ExtendLockTime { time },
            &[],
        )
    }

    pub fn withdraw(&self, router: &mut TerraApp, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.voting_escrow.clone().unwrap().address,
            &ExecuteMsg::Withdraw {},
            &[],
        )
    }

    pub fn query_user_vp(&self, router: &mut TerraApp, user: Addr) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_escrow.clone().unwrap().address,
                &QueryMsg::UserVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_user_vp_at(&self, router: &mut TerraApp, user: Addr, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_escrow.clone().unwrap().address,
                &QueryMsg::UserVotingPowerAt {
                    user: user.to_string(),
                    time,
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_vp(&self, router: &mut TerraApp) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_escrow.clone().unwrap().address,
                &QueryMsg::TotalVotingPower {},
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_vp_at(&self, router: &mut TerraApp, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.voting_escrow.clone().unwrap().address,
                &QueryMsg::TotalVotingPowerAt { time },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }
}

pub fn mint(router: &mut TerraApp, owner: Addr, token_instance: Addr, to: &Addr, amount: u128) {
    let amount = amount * MULTIPLIER as u128;
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: to.to_string(),
        amount: Uint128::from(amount),
    };

    let res = router
        .execute_contract(owner, token_instance, &msg, &[])
        .unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[1].attributes[2], attr("to", String::from(to)));
    assert_eq!(
        res.events[1].attributes[3],
        attr("amount", Uint128::from(amount))
    );
}

pub fn check_balance(app: &mut TerraApp, token_addr: &Addr, contract_addr: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: contract_addr.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token_addr, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}

pub fn increase_allowance(
    router: &mut TerraApp,
    owner: Addr,
    spender: Addr,
    token: Addr,
    amount: Uint128,
) {
    let msg = cw20::Cw20ExecuteMsg::IncreaseAllowance {
        spender: spender.to_string(),
        amount,
        expires: None,
    };

    let res = router
        .execute_contract(owner.clone(), token, &msg, &[])
        .unwrap();

    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "increase_allowance")
    );
    assert_eq!(
        res.events[1].attributes[2],
        attr("owner", owner.to_string())
    );
    assert_eq!(
        res.events[1].attributes[3],
        attr("spender", spender.to_string())
    );
    assert_eq!(res.events[1].attributes[4], attr("amount", amount));
}
