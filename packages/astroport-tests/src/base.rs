use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::staking;
use astroport::token::InstantiateMsg as AstroTokenInstantiateMsg;
use astroport_governance::escrow_fee_distributor::InstantiateMsg as EscrowFeeDistributorInstantiateMsg;
use astroport_governance_voting::astro_voting_escrow::InstantiateMsg as AstroVotingEscrowInstantiateMsg;
use cosmwasm_std::{attr, to_binary, Addr, QueryRequest, StdResult, Uint128, WasmQuery};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};
use terra_multi_test::{ContractWrapper, Executor, TerraApp};

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

impl BaseAstroportTestPackage {
    pub fn init_astro_token(router: &mut TerraApp, owner: Addr) -> Self {
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
                owner.clone(),
                &init_msg,
                &[],
                "Astro token",
                None,
            )
            .unwrap();

        Self {
            owner,
            astro_token: Some(ContractInfo {
                address: astro_token_instance,
                code_id: astro_token_code_id,
            }),
            escrow_fee_distributor: None,
            staking: None,
            voting_escrow: None,
        }
    }

    pub fn init_escrow_fee_distributor(
        router: &mut TerraApp,
        owner: Addr,
        voting_escrow: Addr,
        emergency_return: Addr,
    ) -> Self {
        let escrow_fee_distributor_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_escrow_fee_distributor::contract::execute,
            astroport_escrow_fee_distributor::contract::instantiate,
            astroport_escrow_fee_distributor::contract::query,
        ));

        let escrow_fee_distributor_code_id = router.store_code(escrow_fee_distributor_contract);
        let astro_token = Self::init_astro_token(router, owner.clone())
            .astro_token
            .unwrap();

        let init_msg = EscrowFeeDistributorInstantiateMsg {
            owner: owner.to_string(),
            token: astro_token.address.to_string(),
            voting_escrow: voting_escrow.to_string(),
            emergency_return: emergency_return.to_string(),
            start_time: 0,
        };

        let escrow_fee_distributor_instance = router
            .instantiate_contract(
                escrow_fee_distributor_code_id,
                owner.clone(),
                &init_msg,
                &[],
                "Astroport escrow fee distributor",
                None,
            )
            .unwrap();

        Self {
            owner,
            astro_token: Some(astro_token),
            escrow_fee_distributor: Some(ContractInfo {
                address: escrow_fee_distributor_instance,
                code_id: escrow_fee_distributor_code_id,
            }),
            staking: None,
            voting_escrow: None,
        }
    }

    pub fn init_staking(router: &mut TerraApp, owner: Addr) -> Self {
        let staking_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_staking::contract::execute,
                astroport_staking::contract::instantiate,
                astroport_staking::contract::query,
            )
            .with_reply_empty(astroport_staking::contract::reply),
        );

        let staking_code_id = router.store_code(staking_contract);

        let astro_token = Self::init_astro_token(router, owner.clone())
            .astro_token
            .unwrap();

        let msg = staking::InstantiateMsg {
            owner: owner.to_string(),
            token_code_id: astro_token.code_id,
            deposit_token_addr: astro_token.address.to_string(),
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

        Self {
            owner,
            astro_token: Some(astro_token),
            escrow_fee_distributor: None,
            staking: Some(ContractInfo {
                address: staking_instance,
                code_id: staking_code_id,
            }),
            voting_escrow: None,
        }
    }

    pub fn init_voting_escrow(router: &mut TerraApp, owner: Addr) -> Self {
        let staking = Self::init_staking(router, owner.clone()).staking.unwrap();
        let res = router
            .wrap()
            .query::<staking::ConfigResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: staking.address.to_string(),
                msg: to_binary(&staking::QueryMsg::Config {}).unwrap(),
            }))
            .unwrap();

        let voting_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_voting_escrow::contract::execute,
            astroport_voting_escrow::contract::instantiate,
            astroport_voting_escrow::contract::query,
        ));

        let voting_code_id = router.store_code(voting_contract);

        let msg = AstroVotingEscrowInstantiateMsg {
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
            astro_token: None,
            escrow_fee_distributor: None,
            voting_escrow: Some(ContractInfo {
                address: voting_instance,
                code_id: voting_code_id,
            }),
            staking: Some(staking),
        }
    }

    pub fn mint_some_astro(
        router: &mut TerraApp,
        owner: Addr,
        astro_token_instance: Addr,
        to: &str,
        amount: u128,
    ) {
        let msg = cw20::Cw20ExecuteMsg::Mint {
            recipient: String::from(to),
            amount: Uint128::from(amount),
        };
        let res = router
            .execute_contract(owner, astro_token_instance, &msg, &[])
            .unwrap();
        assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
        assert_eq!(res.events[1].attributes[2], attr("to", String::from(to)));
        assert_eq!(
            res.events[1].attributes[3],
            attr("amount", Uint128::from(100u128))
        );
    }

    pub fn check_token_balance(app: &mut TerraApp, token: &Addr, address: &Addr, expected: u128) {
        let msg = Cw20QueryMsg::Balance {
            address: address.to_string(),
        };
        let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
        assert_eq!(res.unwrap().balance, Uint128::from(expected));
    }

    pub fn allowance_token(
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
}
