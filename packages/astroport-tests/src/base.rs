use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::token::InstantiateMsg as AstroTokenInstantiateMsg;
use astroport_governance::escrow_fee_distributor::{
    ExecuteMsg, InstantiateMsg as EscrowFeeDistributorInstantiateMsg, QueryMsg,
};
use cosmwasm_std::Addr;
use cw20::MinterResponse;
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
            owner: owner.clone(),
            astro_token: Some(astro_token),
            escrow_fee_distributor: Some(ContractInfo {
                address: escrow_fee_distributor_instance,
                code_id: escrow_fee_distributor_code_id,
            }),
        }
    }
}
