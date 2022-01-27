use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, QueryRequest, StdResult, Uint128, WasmQuery};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use terra_multi_test::{
    next_block, AppBuilder, AppResponse, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock,
};

use astroport_governance::escrow_fee_distributor::{ExecuteMsg, InstantiateMsg, QueryMsg};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::token::InstantiateMsg as AstroTokenInstantiateMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ContractInfo {
    pub address: Addr,
    pub code_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct BaseAstroportTestPackages {
    pub owner: Addr,
    pub astro_token: Option<ContractInfo>,
    pub escrow_fee_distributor: Option<ContractInfo>,
}

impl BaseAstroportTestPackages {
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

        let init_msg = InstantiateMsg {
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

const OWNER1: &str = "owner1";
const USER1: &str = "user1";
const USER2: &str = "user2";
const TOKEN_INITIAL_AMOUNT: u128 = 1_000_000_000_000000;

fn mock_app() -> TerraApp {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let custom = TerraMock::luna_ust_case();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .with_custom(custom)
        .build()
}

#[test]
fn claim() {
    let mut router = mock_app();
    let router_ref = &mut router;
    let owner = Addr::unchecked("owner");
    let voting_escrow = Addr::unchecked("voting_escrow");
    let emergency_return = Addr::unchecked("emergency_return");

    let base_test_pack = BaseAstroportTestPackages::init_escrow_fee_distributor(
        router_ref,
        owner,
        voting_escrow,
        emergency_return,
    );

    let escrow_fee_distributor = base_test_pack.escrow_fee_distributor.unwrap();
}
