use astroport::{
    staking::{
        ConfigResponse as StakingConfigResponse, Cw20HookMsg as StakingHookMsg,
        InstantiateMsg as StakingInstantiateMsg, QueryMsg as StakingQueryMsg,
    },
    token::InstantiateMsg as TokenInstantiateMsg,
    xastro_token::QueryMsg as XAstroQueryMsg,
};

use astroport_governance::assembly;
use astroport_governance::assembly::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, Proposal, ProposalStatus, QueryMsg,
};
use cosmwasm_std::{
    attr,
    testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR},
    to_binary, Addr, QueryRequest, StdResult, Uint128, Uint64, WasmQuery,
};
use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, MinterResponse};
use terra_multi_test::{
    next_block, AppBuilder, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock,
};

use astro_assembly::error::ContractError;

const OWNER: &str = "owner";
const USER1: &str = "user1";
const USER2: &str = "user2";

#[test]
fn proper_contract_instantiation() {
    let mut app = mock_app();

    let assembly_contract = Box::new(ContractWrapper::new_with_empty(
        astro_assembly::contract::execute,
        astro_assembly::contract::instantiate,
        astro_assembly::contract::query,
    ));

    let assembly_code = router.store_code(assembly_contract);

    let mut assembly_instantiate_msg = InstantiateMsg {
        xastro_token_addr: x_astro_token_instance.to_string(),
        staking_addr: staking_instance.to_string(),
        proposal_voting_period: 500,
        proposal_effective_delay: 50,
        proposal_expiration_period: 500,
        proposal_required_deposit: Uint128::new(1000u128),
        proposal_required_quorum: 70,
        proposal_required_threshold: 60,
    };

    let assembly_instance = router
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &assembly_instantiate_msg,
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap();
}

#[test]
fn proper_proposal_submitting() {
    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let user = Addr::unchecked(USER1);

    let (token_addr, staking_addr, xastro_addr, assembly_addr) =
        instantiate_contracts(&mut app, owner);

    mint_tokens(&mut app, &token_addr, &user, 2000);

    check_token_balance(&mut app, &token_addr, &user, 2000);

    let msg = Cw20ExecuteMsg::Send {
        contract: staking_addr.to_string(),
        msg: to_binary(&StakingHookMsg::Enter {}).unwrap(),
        amount: Uint128::from(1200u128),
    };

    app.execute_contract(user.clone(), token_addr.clone(), &msg, &[])
        .unwrap();

    check_token_balance(&mut app, &token_addr, &user, 800);
    check_token_balance(&mut app, &xastro_addr, &user, 1200);

    let submit_proposal_msg = Cw20ExecuteMsg::Send {
        contract: assembly_addr.to_string(),
        msg: to_binary(&Cw20HookMsg::SubmitProposal {
            title: String::from("Title"),
            description: String::from("description"),
            link: None,
            messages: None,
        })
        .unwrap(),
        amount: Uint128::from(999u128),
    };

    let res = app
        .execute_contract(user.clone(), xastro_addr.clone(), &submit_proposal_msg, &[])
        .unwrap_err();

    assert_eq!(res.to_string(), "Insufficient deposit!");

    let submit_proposal_msg = Cw20ExecuteMsg::Send {
        contract: assembly_addr.to_string(),
        msg: to_binary(&Cw20HookMsg::SubmitProposal {
            title: String::from("Title"),
            description: String::from("Description"),
            link: None,
            messages: None,
        })
        .unwrap(),
        amount: Uint128::from(1000u128),
    };

    let res = app
        .execute_contract(user.clone(), xastro_addr.clone(), &submit_proposal_msg, &[])
        .unwrap();

    let proposal: Proposal = app
        .wrap()
        .query_wasm_smart(
            assembly_addr.clone(),
            &QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();

    assert_eq!(proposal.proposal_id, Uint64::from(1u64));
    assert_eq!(proposal.submitter, user);
    assert_eq!(proposal.status, ProposalStatus::Active);
    assert_eq!(proposal.for_power, Uint128::zero());
    assert_eq!(proposal.against_power, Uint128::zero());
    assert_eq!(proposal.for_voters, Vec::<Addr>::new());
    assert_eq!(proposal.against_voters, Vec::<Addr>::new());
    assert_eq!(proposal.start_block, 12_345);
    assert_eq!(proposal.end_block, 12_345 + 500);
    assert_eq!(proposal.title, String::from("Title"));
    assert_eq!(proposal.description, String::from("Description"));
    assert_eq!(proposal.link, None);
    assert_eq!(proposal.messages, None);
    assert_eq!(proposal.deposit_amount, Uint128::from(1000u64))
}

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

fn instantiate_contracts(router: &mut TerraApp, owner: Addr) -> (Addr, Addr, Addr, Addr) {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let astro_token_code_id = router.store_code(astro_token_contract);

    let msg = TokenInstantiateMsg {
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

    let msg = StakingInstantiateMsg {
        owner: owner.to_string(),
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

    let msg = QueryMsg::Config {};
    let res = router
        .wrap()
        .query::<StakingConfigResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: staking_instance.to_string(),
            msg: to_binary(&msg).unwrap(),
        }))
        .unwrap();

    let x_astro_token_instance = res.share_token_addr;

    let assembly_contract = Box::new(ContractWrapper::new_with_empty(
        astro_assembly::contract::execute,
        astro_assembly::contract::instantiate,
        astro_assembly::contract::query,
    ));

    let assembly_code = router.store_code(assembly_contract);

    let assembly_instantiate_msg = InstantiateMsg {
        xastro_token_addr: x_astro_token_instance.to_string(),
        staking_addr: staking_instance.to_string(),
        proposal_voting_period: 500,
        proposal_effective_delay: 50,
        proposal_expiration_period: 500,
        proposal_required_deposit: Uint128::new(1000u128),
        proposal_required_quorum: 70,
        proposal_required_threshold: 60,
    };

    let assembly_instance = router
        .instantiate_contract(
            assembly_code,
            owner.clone(),
            &assembly_instantiate_msg,
            &[],
            "Assembly".to_string(),
            Some(owner.to_string()),
        )
        .unwrap();

    // in multitest, contract names are named in the order in which contracts are created.
    assert_eq!("contract #0", astro_token_instance);
    assert_eq!("contract #1", staking_instance);
    assert_eq!("contract #2", x_astro_token_instance);
    assert_eq!("contract #3", assembly_instance);

    (
        astro_token_instance,
        staking_instance,
        x_astro_token_instance,
        assembly_instance,
    )
}

fn mint_tokens(app: &mut TerraApp, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(Addr::unchecked(OWNER), token.to_owned(), &msg, &[])
        .unwrap();
}

fn check_token_balance(app: &mut TerraApp, token: &Addr, address: &Addr, expected: u128) {
    let msg = XAstroQueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}
