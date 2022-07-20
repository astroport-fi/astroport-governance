#[cfg(test)]
mod tests {
    use astroport_governance::utils::EPOCH_START;
    use astroport_tests::escrow_helper::EscrowHelper;
    use cosmwasm_std::{to_binary, Addr, Empty, QueryRequest, WasmQuery};
    use cw_multi_test::{App, Contract, ContractWrapper, Executor};
    use voting_escrow_delegation::{msg, state};

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
            astroport_nft::entry::execute,
            astroport_nft::entry::instantiate,
            astroport_nft::entry::query,
        );
        Box::new(contract)
    }

    const USER: &str = "user";
    const ADMIN: &str = "admin";

    pub struct DelegatorHelper {
        pub escrow_helper: EscrowHelper,
        pub delegation_instance: Addr,
        pub nft_instance: Addr,
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
                &msg::InstantiateMsg {
                    owner: ADMIN.to_string(),
                    nft_token_code_id: nft_id,
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
                msg: to_binary(&msg::QueryMsg::Config {}).unwrap(),
            }))
            .unwrap();

        (delegation_addr, res.nft_token_addr)
    }

    fn mock_app() -> App {
        let mut app = App::new(|_router, _, _| {});

        app.update_block(|bi| {
            bi.time = bi.time.plus_seconds(EPOCH_START);
            bi.height += 1;
            bi.chain_id = "cosm-wasm-test".to_string();
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

    mod queries {
        use super::*;
        use cosmwasm_std::{to_binary, QueryRequest, WasmQuery};

        #[test]
        fn config() {
            let (router, delegator_helper) = proper_instantiate();

            let res = router
                .wrap()
                .query::<state::Config>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: delegator_helper.delegation_instance.to_string(),
                    msg: to_binary(&msg::QueryMsg::Config {}).unwrap(),
                }))
                .unwrap();

            assert_eq!("admin", res.owner.to_string());
        }
    }

    mod executes {
        use super::*;
        use astroport_governance::utils::WEEK;
        use astroport_nft::{
            ExecuteMsg as ExecuteMsgNFT, Extension, MintMsg, QueryMsg as QueryMsgNFT,
        };

        use cosmwasm_std::{to_binary, QueryRequest, Uint128, WasmQuery};
        use cw721::{ContractInfoResponse, NumTokensResponse, TokensResponse};
        use cw_multi_test::next_block;
        use voting_escrow_delegation::msg::{ExecuteMsg, QueryMsg};

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
        }

        #[test]
        fn create_delegation() {
            let (mut router, delegator_helper) = proper_instantiate();
            let router_ref = &mut router;
            let nft_helper =
                astroport_nft::helpers::Cw721Contract(delegator_helper.nft_instance.clone());

            // try to mint from user
            let err = router_ref
                .execute_contract(
                    Addr::unchecked("user"),
                    delegator_helper.delegation_instance.clone(),
                    &ExecuteMsg::CreateDelegation {
                        percent: Uint128::new(50),
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
                .mint_xastro(router_ref, "user", 100);
            delegator_helper
                .escrow_helper
                .check_xastro_balance(router_ref, "user", 100);

            // Create valid voting escrow lock
            delegator_helper
                .escrow_helper
                .create_lock(router_ref, "user", WEEK * 2, 90f32)
                .unwrap();
            // Check that 90 xASTRO were actually debited
            delegator_helper
                .escrow_helper
                .check_xastro_balance(router_ref, "user", 10);
            delegator_helper.escrow_helper.check_xastro_balance(
                router_ref,
                delegator_helper.escrow_helper.escrow_instance.as_str(),
                90,
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
                .create_lock(router_ref, "user2", WEEK * 2, 90f32)
                .unwrap();
            // Check that 90 xASTRO were actually debited
            delegator_helper
                .escrow_helper
                .check_xastro_balance(router_ref, "user2", 10);
            delegator_helper.escrow_helper.check_xastro_balance(
                router_ref,
                delegator_helper.escrow_helper.escrow_instance.as_str(),
                180,
            );

            // try to mint from user
            router_ref
                .execute_contract(
                    Addr::unchecked("user"),
                    delegator_helper.delegation_instance.clone(),
                    &ExecuteMsg::CreateDelegation {
                        percent: Uint128::new(100),
                        expire_time: WEEK,
                        token_id: "token_1".to_string(),
                        recipient: "user2".to_string(),
                    },
                    &[],
                )
                .unwrap();

            // try to mint from user
            let err = router_ref
                .execute_contract(
                    Addr::unchecked("user"),
                    delegator_helper.delegation_instance.clone(),
                    &ExecuteMsg::CreateDelegation {
                        percent: Uint128::new(100),
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

            // try to mint from user
            let err = router_ref
                .execute_contract(
                    Addr::unchecked("user"),
                    delegator_helper.delegation_instance.clone(),
                    &ExecuteMsg::CreateDelegation {
                        percent: Uint128::new(30),
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

            // try to mint from user
            router_ref
                .execute_contract(
                    Addr::unchecked("user"),
                    delegator_helper.delegation_instance.clone(),
                    &ExecuteMsg::ExtendDelegation {
                        percentage: Uint128::new(50),
                        expire_time: WEEK * 2,
                        token_id: "token_1".to_string(),
                        recipient: "user2".to_string(),
                    },
                    &[],
                )
                .unwrap();

            // try to mint from user
            router_ref
                .execute_contract(
                    Addr::unchecked("user"),
                    delegator_helper.delegation_instance.clone(),
                    &ExecuteMsg::CreateDelegation {
                        percent: Uint128::new(100),
                        expire_time: WEEK,
                        token_id: "token_4".to_string(),
                        recipient: "user2".to_string(),
                    },
                    &[],
                )
                .unwrap();

            let empty_tokens: Vec<String> = vec![];
            // check user's nft token
            let resp = nft_helper
                .tokens(&router_ref.wrap().into(), "user", None, None)
                .unwrap();
            assert_eq!(empty_tokens, resp.tokens);

            // check user's nft token
            let resp = router_ref
                .wrap()
                .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: delegator_helper.delegation_instance.to_string(),
                    msg: to_binary(&QueryMsg::AdjustedBalance {
                        account: "user".to_string(),
                    })
                    .unwrap(),
                }))
                .unwrap();
            assert_eq!(Uint128::new(0), resp);

            // check user's nft token
            let resp = nft_helper
                .tokens(&router_ref.wrap().into(), "user", None, None)
                .unwrap();
            assert_eq!(empty_tokens, resp.tokens);

            // check user2's nft token
            let resp = nft_helper
                .tokens(&router_ref.wrap().into(), "user2", None, None)
                .unwrap();
            assert_eq!(vec!["token_1", "token_4"], resp.tokens);

            // check user's adjusted balance
            let resp = router_ref
                .wrap()
                .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: delegator_helper.delegation_instance.to_string(),
                    msg: to_binary(&QueryMsg::AdjustedBalance {
                        account: "user".to_string(),
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
                    })
                    .unwrap(),
                }))
                .unwrap();
            assert_eq!(Uint128::new(185_192_304), resp);

            router_ref.update_block(next_block);
            router_ref
                .update_block(|block_info| block_info.time = block_info.time.plus_seconds(WEEK));

            // check user's adjusted balance
            let resp = router_ref
                .wrap()
                .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: delegator_helper.delegation_instance.to_string(),
                    msg: to_binary(&QueryMsg::AdjustedBalance {
                        account: "user".to_string(),
                    })
                    .unwrap(),
                }))
                .unwrap();
            assert_eq!(Uint128::new(23_149_038), resp);

            // check user2's adjusted balance
            let resp = router_ref
                .wrap()
                .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: delegator_helper.delegation_instance.to_string(),
                    msg: to_binary(&QueryMsg::AdjustedBalance {
                        account: "user2".to_string(),
                    })
                    .unwrap(),
                }))
                .unwrap();
            assert_eq!(Uint128::new(69_447_114), resp);

            // trytransfer NFT to user2
            // router_ref
            //     .execute_contract(
            //         Addr::unchecked("user"),
            //         delegator_helper.nft_instance.clone(),
            //         &Cw721ExecuteMsg::TransferNft {
            //             recipient: "user2".to_string(),
            //             token_id: "token_1".to_string(),
            //         },
            //         &[],
            //     )
            //     .unwrap_err();
        }
    }
}
