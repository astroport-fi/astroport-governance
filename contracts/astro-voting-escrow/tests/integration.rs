use astroport::{staking as xastro, token as astro};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{attr, to_binary, Addr, QueryRequest, Timestamp, Uint128, WasmQuery};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use terra_multi_test::{AppBuilder, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock};

use astroport_governance::astro_voting_escrow::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, VotingPowerResponse,
};
use astroport_voting_escrow;
use astroport_voting_escrow::contract::WEEK;

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

fn instantiate_contracts(router: &mut TerraApp, owner: Addr) -> (Addr, Addr, Addr) {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let astro_token_code_id = router.store_code(astro_token_contract);

    let msg = astro::InstantiateMsg {
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
            &msg,
            &[],
            String::from("ASTRO"),
            None,
        )
        .unwrap();

    let staking_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_staking::contract::execute,
            astroport_staking::contract::instantiate,
            astroport_staking::contract::query,
        )
        .with_reply_empty(astroport_staking::contract::reply),
    );

    let staking_code_id = router.store_code(staking_contract);

    let msg = xastro::InstantiateMsg {
        token_code_id: astro_token_code_id,
        deposit_token_addr: astro_token_instance.to_string(),
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

    let res = router
        .wrap()
        .query::<xastro::ConfigResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: staking_instance.to_string(),
            msg: to_binary(&xastro::QueryMsg::Config {}).unwrap(),
        }))
        .unwrap();

    let voting_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_voting_escrow::contract::execute,
            astroport_voting_escrow::contract::instantiate,
            astroport_voting_escrow::contract::query,
        )
        .with_reply_empty(astroport_staking::contract::reply),
    );

    let voting_code_id = router.store_code(voting_contract);

    let msg = InstantiateMsg {
        deposit_token_addr: res.share_token_addr.to_string(),
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

    (voting_instance, astro_token_instance, staking_instance)
}

struct Minter {
    owner: Addr,
    astro_token: Addr,
    staking_instance: Addr,
}

impl Minter {
    pub fn mint_xastro(&self, router: &mut TerraApp, to: &str, amount: u64) {
        let msg = cw20::Cw20ExecuteMsg::Mint {
            recipient: String::from(to),
            amount: Uint128::from(amount),
        };
        let res = router
            .execute_contract(self.owner.clone(), self.astro_token.clone(), &msg, &[])
            .unwrap();
        assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
        assert_eq!(res.events[1].attributes[2], attr("to", String::from(to)));
        assert_eq!(
            res.events[1].attributes[3],
            attr("amount", Uint128::from(amount))
        );

        let to_addr = Addr::unchecked(to);
        let msg = Cw20ExecuteMsg::Send {
            contract: self.staking_instance.to_string(),
            msg: to_binary(&xastro::Cw20HookMsg::Enter {}).unwrap(),
            amount: Uint128::from(amount),
        };
        router
            .execute_contract(to_addr, self.astro_token.clone(), &msg, &[])
            .unwrap();
    }
}

#[test]
fn proper_initialization() {
    let mut router = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user");
    let (voting_instance, astro_token, staking_instance) =
        instantiate_contracts(&mut router, owner.clone());

    let minter = Minter {
        owner: owner.clone(),
        astro_token,
        staking_instance,
    };

    minter.mint_xastro(&mut router, "user", 100);

    let cw20msg = Cw20ReceiveMsg {
        sender: "user".to_string(),
        amount: Uint128::from(100_u128),
        msg: to_binary(&Cw20HookMsg::CreateLock {
            time: Timestamp::from_seconds(WEEK * 3),
        })
        .unwrap(),
    };
    router
        .execute_contract(
            user.clone(),
            voting_instance.clone(),
            &ExecuteMsg::Receive(cw20msg),
            &[],
        )
        .unwrap();

    let res: VotingPowerResponse = router
        .wrap()
        .query_wasm_smart(voting_instance.clone(), &QueryMsg::TotalVotingPower {})
        .unwrap();

    dbg!(res);
}
