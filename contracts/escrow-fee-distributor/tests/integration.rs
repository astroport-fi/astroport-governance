use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, QueryRequest, StdResult, Uint128, WasmQuery};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use terra_multi_test::{
    next_block, AppBuilder, AppResponse, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock,
};

use astroport_governance::escrow_fee_distributor::{ExecuteMsg, QueryMsg};
use astroport_tests::base::BaseAstroportTestPackage;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::token::InstantiateMsg as AstroTokenInstantiateMsg;

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

    let base_test_pack = BaseAstroportTestPackage::init_escrow_fee_distributor(
        router_ref,
        owner,
        voting_escrow,
        emergency_return,
    );

    let escrow_fee_distributor = base_test_pack.escrow_fee_distributor.unwrap();
}
