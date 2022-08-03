use astroport_governance::utils::EPOCH_START;
use astroport_governance::utils::WEEK;
use astroport_governance::voting_escrow_delegation::{InstantiateMsg, QueryMsg};
use astroport_tests::escrow_helper::EscrowHelper;
use cosmwasm_std::{to_binary, Addr, Empty, QueryRequest, Uint128, WasmQuery};
use cw721_base::{ExecuteMsg as ExecuteMsgNFT, Extension, MintMsg, QueryMsg as QueryMsgNFT};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};
use voting_escrow_delegation::state;

use astroport_governance::voting_escrow_delegation::ExecuteMsg;
use cw721::{ContractInfoResponse, Cw721ExecuteMsg, NumTokensResponse, TokensResponse};

pub struct DelegatorHelper {
    pub escrow_helper: EscrowHelper,
    pub delegation_instance: Addr,
    pub nft_instance: Addr,
}

const EMPTY_TOKENS: Vec<String> = vec![];
const USER: &str = "user";
const ADMIN: &str = "admin";

pub fn contract_escrow_delegation_template() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(
        voting_escrow_delegation::contract::execute,
        voting_escrow_delegation::contract::instantiate,
        voting_escrow_delegation::contract::query,
    )
    .with_reply_empty(voting_escrow_delegation::contract::reply);
    Box::new(contract)
}

pub fn contract_nft_template() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        astroport_nft::contract::execute,
        astroport_nft::contract::instantiate,
        astroport_nft::contract::query,
    );
    Box::new(contract)
}

fn instantiate_delegation(
    router: &mut App,
    escrow_addr: Addr,
    delegation_id: u64,
    nft_id: u64,
) -> (Addr, Addr) {
    let delegation_addr = router
        .instantiate_contract(
            delegation_id,
            Addr::unchecked(ADMIN.to_string()),
            &InstantiateMsg {
                owner: ADMIN.to_string(),
                nft_code_id: nft_id,
                voting_escrow_addr: escrow_addr.to_string(),
            },
            &[],
            String::from("Astroport Escrow Delegation"),
            None,
        )
        .unwrap();

    let res = router
        .wrap()
        .query::<state::Config>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegation_addr.to_string(),
            msg: to_binary(&QueryMsg::Config {}).unwrap(),
        }))
        .unwrap();

    (delegation_addr, res.nft_addr)
}

fn mock_app() -> App {
    let mut app = App::default();

    app.update_block(|bi| {
        bi.time = bi.time.plus_seconds(EPOCH_START);
        bi.height += 1;
    });

    app
}

fn proper_instantiate() -> (App, DelegatorHelper) {
    let mut router = mock_app();
    let helper = EscrowHelper::init(&mut router, Addr::unchecked(ADMIN));

    let delegation_id = router.store_code(contract_escrow_delegation_template());
    let nft_id = router.store_code(contract_nft_template());

    let (delegation_addr, nft_addr) = instantiate_delegation(
        &mut router,
        helper.escrow_instance.clone(),
        delegation_id,
        nft_id,
    );

    (
        router,
        DelegatorHelper {
            escrow_helper: helper,
            delegation_instance: delegation_addr,
            nft_instance: nft_addr,
        },
    )
}

#[test]
fn config() {
    let (router, delegator_helper) = proper_instantiate();

    let res = router
        .wrap()
        .query::<state::Config>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::Config {}).unwrap(),
        }))
        .unwrap();

    assert_eq!("admin", res.owner.to_string());
}

#[test]
fn mint() {
    let (mut app, delegator_helper) = proper_instantiate();

    let resp = app
        .wrap()
        .query::<ContractInfoResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.nft_instance.to_string(),
            msg: to_binary(&QueryMsgNFT::ContractInfo {}).unwrap(),
        }))
        .unwrap();
    assert_eq!("Astroport NFT", resp.name);
    assert_eq!("ASTRO-NFT", resp.symbol);

    // try to mint from random
    let err = app
        .execute_contract(
            Addr::unchecked("random"),
            delegator_helper.nft_instance.clone(),
            &ExecuteMsgNFT::Mint(MintMsg::<Extension> {
                token_id: "token_1".to_string(),
                owner: USER.to_string(),
                token_uri: None,
                extension: None,
            }),
            &[],
        )
        .unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    // try to mint from owner
    app.execute_contract(
        delegator_helper.delegation_instance.clone(),
        delegator_helper.nft_instance.clone(),
        &ExecuteMsgNFT::Mint(MintMsg::<Extension> {
            token_id: "token_1".to_string(),
            owner: USER.to_string(),
            token_uri: None,
            extension: None,
        }),
        &[],
    )
    .unwrap();

    let resp = app
        .wrap()
        .query::<NumTokensResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.nft_instance.to_string(),
            msg: to_binary(&QueryMsgNFT::NumTokens {}).unwrap(),
        }))
        .unwrap();
    assert_eq!(1, resp.count);

    let resp = app
        .wrap()
        .query::<TokensResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.nft_instance.to_string(),
            msg: to_binary(&QueryMsgNFT::Tokens {
                owner: USER.to_string(),
                start_after: None,
                limit: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(vec!["token_1",], resp.tokens);

    // try to mint from owner for the same token ID
    let err = app
        .execute_contract(
            delegator_helper.delegation_instance.clone(),
            delegator_helper.nft_instance.clone(),
            &ExecuteMsgNFT::Mint(MintMsg::<Extension> {
                token_id: "token_1".to_string(),
                owner: USER.to_string(),
                token_uri: None,
                extension: None,
            }),
            &[],
        )
        .unwrap_err();
    assert_eq!("token_id already claimed", err.root_cause().to_string());

    // try to burn nft by token ID
    let err = app
        .execute_contract(
            delegator_helper.delegation_instance.clone(),
            delegator_helper.nft_instance.clone(),
            &ExecuteMsgNFT::<Extension>::Burn {
                token_id: "token_1".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "Generic error: Operation is not supported",
        err.root_cause().to_string()
    );

    // check if token exists
    let resp = app
        .wrap()
        .query::<TokensResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.nft_instance.to_string(),
            msg: to_binary(&QueryMsgNFT::Tokens {
                owner: USER.to_string(),
                start_after: None,
                limit: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(vec!["token_1",], resp.tokens);
}

#[test]
fn create_delegation() {
    let (mut router, delegator_helper) = proper_instantiate();
    let router_ref = &mut router;
    let nft_helper = cw721_base::helpers::Cw721Contract(delegator_helper.nft_instance.clone());

    // try to create delegation from user with zero voting power
    let err = router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(50),
                expire_time: WEEK,
                token_id: "token_1".to_string(),
                recipient: "user2".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "You can't delegate with zero voting power",
        err.root_cause().to_string()
    );

    // Mint ASTRO, stake it and mint xASTRO
    delegator_helper
        .escrow_helper
        .mint_xastro(router_ref, "user", 200);
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user", 200);

    // Create valid voting escrow lock
    delegator_helper
        .escrow_helper
        .create_lock(router_ref, "user", WEEK * 2, 100f32)
        .unwrap();
    // Check that 100 xASTRO were actually debited
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user", 100);
    delegator_helper.escrow_helper.check_xastro_balance(
        router_ref,
        delegator_helper.escrow_helper.escrow_instance.as_str(),
        100,
    );

    // Mint ASTRO, stake it and mint xASTRO
    delegator_helper
        .escrow_helper
        .mint_xastro(router_ref, "user2", 200);
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user2", 200);

    // Create valid voting escrow lock
    delegator_helper
        .escrow_helper
        .create_lock(router_ref, "user2", WEEK * 2, 100f32)
        .unwrap();
    // Check that 100 xASTRO were actually debited
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user2", 100);
    delegator_helper.escrow_helper.check_xastro_balance(
        router_ref,
        delegator_helper.escrow_helper.escrow_instance.as_str(),
        200,
    );

    // check user's adjusted balance before create a delegation
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(102_884_614), resp);

    // check user's nft tokens before create a delegation
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user", None, None)
        .unwrap();
    assert_eq!(EMPTY_TOKENS, resp.tokens);

    // check user2's adjusted balance before create a delegation
    let user_vp_before_delegation = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(102_884_614), user_vp_before_delegation);

    // check user2's nft tokens before create a delegation
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user2", None, None)
        .unwrap();
    assert_eq!(EMPTY_TOKENS, resp.tokens);

    // create delegation for one week
    router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(100),
                expire_time: WEEK,
                token_id: "token_1".to_string(),
                recipient: "user2".to_string(),
            },
            &[],
        )
        .unwrap();

    // try to create delegation with the same token ID
    let err = router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(100),
                expire_time: WEEK,
                token_id: "token_1".to_string(),
                recipient: "user2".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "A delegation with a token token_1 already exists.",
        err.root_cause().to_string()
    );

    // try create delegation without free voting power
    let err = router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(30),
                expire_time: WEEK,
                token_id: "token_2".to_string(),
                recipient: "user2".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "You have already delegated all the voting power.",
        err.root_cause().to_string()
    );

    // check user's nft tokens
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user", None, None)
        .unwrap();
    assert_eq!(EMPTY_TOKENS, resp.tokens);

    // check user's adjusted balance
    let user_vp_after_delegation = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(0), user_vp_after_delegation);

    // check user2's nft tokens
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user2", None, None)
        .unwrap();
    assert_eq!(vec!["token_1"], resp.tokens);

    // check user2's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(205_769_228), resp);

    // check user's delegated voting power
    let user_delegated_vp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::DelegatedVotingPower {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();

    // check user's user_vp_after_delegation + user_delegated_vp = user_vp_before_delegation
    assert_eq!(
        user_vp_before_delegation,
        user_delegated_vp + user_vp_after_delegation
    );

    router_ref.update_block(|block_info| {
        block_info.time = block_info.time.plus_seconds(WEEK);
        block_info.height += 1;
    });

    // check user's adjusted balance when delegation expired
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(51_442_307), resp);

    // check user2's adjusted balance when delegation expired
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(51_442_307), resp);

    // try to transfer NFT to user2
    router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.nft_instance.clone(),
            &Cw721ExecuteMsg::TransferNft {
                recipient: "user2".to_string(),
                token_id: "token_1".to_string(),
            },
            &[],
        )
        .unwrap_err();
}

#[test]
fn create_multiple_delegation() {
    let (mut router, delegator_helper) = proper_instantiate();
    let router_ref = &mut router;
    let nft_helper = cw721_base::helpers::Cw721Contract(delegator_helper.nft_instance.clone());

    // Mint ASTRO, stake it and mint xASTRO
    delegator_helper
        .escrow_helper
        .mint_xastro(router_ref, "user", 200);
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user", 200);

    // Create valid voting escrow lock
    delegator_helper
        .escrow_helper
        .create_lock(router_ref, "user", WEEK * 10, 100f32)
        .unwrap();
    // Check that 100 xASTRO were actually debited
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user", 100);
    delegator_helper.escrow_helper.check_xastro_balance(
        router_ref,
        delegator_helper.escrow_helper.escrow_instance.as_str(),
        100,
    );

    // Mint ASTRO, stake it and mint xASTRO
    delegator_helper
        .escrow_helper
        .mint_xastro(router_ref, "user2", 200);
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user2", 200);

    // Create valid voting escrow lock
    delegator_helper
        .escrow_helper
        .create_lock(router_ref, "user2", WEEK * 5, 100f32)
        .unwrap();
    // Check that 100 xASTRO were actually debited
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user2", 100);
    delegator_helper.escrow_helper.check_xastro_balance(
        router_ref,
        delegator_helper.escrow_helper.escrow_instance.as_str(),
        200,
    );

    // Mint ASTRO, stake it and mint xASTRO
    delegator_helper
        .escrow_helper
        .mint_xastro(router_ref, "user3", 200);
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user3", 200);

    // Create valid voting escrow lock
    delegator_helper
        .escrow_helper
        .create_lock(router_ref, "user3", WEEK, 100f32)
        .unwrap();
    // Check that 100 xASTRO were actually debited
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user3", 100);
    delegator_helper.escrow_helper.check_xastro_balance(
        router_ref,
        delegator_helper.escrow_helper.escrow_instance.as_str(),
        300,
    );

    // try to create delegation for 1 week for user2
    router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(30),
                expire_time: WEEK,
                token_id: "token_1".to_string(),
                recipient: "user2".to_string(),
            },
            &[],
        )
        .unwrap();

    // try to create delegation for 3 weeks for user3
    router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(30),
                expire_time: WEEK * 3,
                token_id: "token_2".to_string(),
                recipient: "user3".to_string(),
            },
            &[],
        )
        .unwrap();

    // try to create delegation for 2 weeks for user1
    let err = router_ref
        .execute_contract(
            Addr::unchecked("user3"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(30),
                expire_time: WEEK * 2,
                token_id: "token_3".to_string(),
                recipient: "user".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "The delegation period must be at least a week and not more than a user lock period.",
        err.root_cause().to_string()
    );

    // try to create delegation for 1 week for user1
    router_ref
        .execute_contract(
            Addr::unchecked("user3"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(30),
                expire_time: WEEK,
                token_id: "token_3".to_string(),
                recipient: "user".to_string(),
            },
            &[],
        )
        .unwrap();

    // check the user's NFT.
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user", None, None)
        .unwrap();
    assert_eq!(vec!["token_3"], resp.tokens);

    // check user's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(86_499_999), resp);

    // check the user2's NFT.
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user2", None, None)
        .unwrap();
    assert_eq!(vec!["token_1"], resp.tokens);

    // check user2's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(141_538_456), resp);

    // check user3's nft tokens
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user3", None, None)
        .unwrap();
    assert_eq!(vec!["token_2"], resp.tokens);

    // check user3's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user3".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(95_038_457), resp);

    router_ref.update_block(|block_info| {
        block_info.time = block_info.time.plus_seconds(WEEK);
        block_info.height += 1;
    });

    // try to create delegation without free voting power
    let err = router_ref
        .execute_contract(
            Addr::unchecked("user3"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(30),
                expire_time: WEEK,
                token_id: "token_4".to_string(),
                recipient: "user2".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "You can't delegate with zero voting power",
        err.root_cause().to_string()
    );

    // check user's adjusted balance when one delegation is expired
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(86_961_535), resp);

    // check user2's adjusted balance when delegation expired
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(85_769_228), resp);

    // check user3's adjusted balance when lock is expired
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user3".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(16_019_228), resp);

    // try to transfer NFT with ID `token_1` from user1 to user3
    let err = router_ref
        .execute_contract(
            Addr::unchecked("user1"),
            delegator_helper.nft_instance.clone(),
            &Cw721ExecuteMsg::TransferNft {
                recipient: "user3".to_string(),
                token_id: "token_1".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!("Unauthorized", err.root_cause().to_string());

    // try to transfer NFT with ID `token_1` from user2 to user3
    router_ref
        .execute_contract(
            Addr::unchecked("user2"),
            delegator_helper.nft_instance.clone(),
            &Cw721ExecuteMsg::TransferNft {
                recipient: "user3".to_string(),
                token_id: "token_1".to_string(),
            },
            &[],
        )
        .unwrap();

    // check the user's NFT.
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user", None, None)
        .unwrap();
    assert_eq!(vec!["token_3"], resp.tokens);

    // check the user2's NFT.
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user2", None, None)
        .unwrap();
    assert_eq!(EMPTY_TOKENS, resp.tokens);

    // check the user3's NFT.
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user3", None, None)
        .unwrap();
    assert_eq!(vec!["token_1", "token_2"], resp.tokens);

    // check user's adjusted balance after transferred token
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(86_961_535), resp);

    // check user2's adjusted balance when delegation expired
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(85_769_228), resp);

    // check user3's adjusted balance when lock is expired and token_1 is expired
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user3".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(16_019_228), resp);

    router_ref.update_block(|block_info| {
        block_info.time = block_info.time.plus_seconds(WEEK * 8);
        block_info.height += 1;
    });

    // check the user's NFT.
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user", None, None)
        .unwrap();
    assert_eq!(vec!["token_3"], resp.tokens);

    // check the user2's NFT.
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user2", None, None)
        .unwrap();
    assert_eq!(EMPTY_TOKENS, resp.tokens);

    // check the user3's NFT.
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user3", None, None)
        .unwrap();
    assert_eq!(vec!["token_1", "token_2"], resp.tokens);

    // check user's adjusted balance after transferred token
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(11_442_307), resp);

    // check user2's adjusted balance when user2's lock and tokens are expired
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(0), resp);

    // check user3's adjusted balance when user3's lock and tokens are expired
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user3".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(0), resp);
}

#[test]
fn extend_delegation() {
    let (mut router, delegator_helper) = proper_instantiate();
    let router_ref = &mut router;
    let nft_helper = cw721_base::helpers::Cw721Contract(delegator_helper.nft_instance.clone());

    // Mint ASTRO, stake it and mint xASTRO
    delegator_helper
        .escrow_helper
        .mint_xastro(router_ref, "user", 100);
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user", 100);

    // Create valid voting escrow lock
    delegator_helper
        .escrow_helper
        .create_lock(router_ref, "user", WEEK * 5, 100f32)
        .unwrap();
    // Check that 90 xASTRO were actually debited
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user", 0);
    delegator_helper.escrow_helper.check_xastro_balance(
        router_ref,
        delegator_helper.escrow_helper.escrow_instance.as_str(),
        100,
    );

    // Mint ASTRO, stake it and mint xASTRO
    delegator_helper
        .escrow_helper
        .mint_xastro(router_ref, "user2", 100);
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user2", 100);

    // Create valid voting escrow lock
    delegator_helper
        .escrow_helper
        .create_lock(router_ref, "user2", WEEK * 2, 100f32)
        .unwrap();
    // Check that 90 xASTRO were actually debited
    delegator_helper
        .escrow_helper
        .check_xastro_balance(router_ref, "user2", 0);
    delegator_helper.escrow_helper.check_xastro_balance(
        router_ref,
        delegator_helper.escrow_helper.escrow_instance.as_str(),
        200,
    );

    // try to create delegation to user2
    router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                percentage: Uint128::new(100),
                expire_time: WEEK,
                token_id: "token_1".to_string(),
                recipient: "user2".to_string(),
            },
            &[],
        )
        .unwrap();

    // check user's nft token
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user", None, None)
        .unwrap();
    assert_eq!(EMPTY_TOKENS, resp.tokens);

    // check user2's nft token
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user2", None, None)
        .unwrap();
    assert_eq!(vec!["token_1"], resp.tokens);

    // check user's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(0), resp);

    // check user2's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(210_096_149), resp);

    router_ref.update_block(|block_info| {
        block_info.time = block_info.time.plus_seconds(WEEK);
        block_info.height += 1;
    });

    // check user's nft token
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user", None, None)
        .unwrap();
    assert_eq!(EMPTY_TOKENS, resp.tokens);

    // check user2's nft token
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user2", None, None)
        .unwrap();
    assert_eq!(vec!["token_1"], resp.tokens);

    // check user's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(85_769_228), resp);

    // check user2's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(51_442_307), resp);

    // try to extend delegation period
    router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::ExtendDelegation {
                percentage: Uint128::new(90),
                expire_time: WEEK * 3,
                token_id: "token_1".to_string(),
            },
            &[],
        )
        .unwrap();

    // check user's nft token
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user", None, None)
        .unwrap();
    assert_eq!(EMPTY_TOKENS, resp.tokens);

    // check user2's nft token
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user2", None, None)
        .unwrap();
    assert_eq!(vec!["token_1"], resp.tokens);

    // check user's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(8_576_924), resp);

    // check user2's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(128_634_611), resp);

    router_ref.update_block(|block_info| {
        block_info.time = block_info.time.plus_seconds(WEEK * 3);
        block_info.height += 1;
    });

    // check user's nft token
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user", None, None)
        .unwrap();
    assert_eq!(EMPTY_TOKENS, resp.tokens);

    // check user2's nft token
    let resp = nft_helper
        .tokens(&router_ref.wrap().into(), "user2", None, None)
        .unwrap();
    assert_eq!(vec!["token_1"], resp.tokens);

    // check user's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(21_442_307), resp);

    // check user2's adjusted balance
    let resp = router_ref
        .wrap()
        .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: delegator_helper.delegation_instance.to_string(),
            msg: to_binary(&QueryMsg::AdjustedBalance {
                account: "user2".to_string(),
                timestamp: None,
            })
            .unwrap(),
        }))
        .unwrap();
    assert_eq!(Uint128::new(0), resp);

    // try to extend delegation period
    let err = router_ref
        .execute_contract(
            Addr::unchecked("user"),
            delegator_helper.delegation_instance.clone(),
            &ExecuteMsg::ExtendDelegation {
                percentage: Uint128::new(90),
                expire_time: WEEK * 3,
                token_id: "token_1".to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        "The delegation period must be at least a week and not more than a user lock period.",
        err.root_cause().to_string()
    );
}
