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
            let (app, delegator_helper) = proper_instantiate();

            let res = app
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
        use astroport_governance::utils::get_period;
        use astroport_nft::{
            ExecuteMsg as ExecuteMsgNFT, Extension, MintMsg, QueryMsg as QueryMsgNFT,
        };
        use cosmwasm_std::{to_binary, QueryRequest, Uint128, WasmQuery};
        use cw721::{ContractInfoResponse, NumTokensResponse, TokensResponse};
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
            let (mut app, delegator_helper) = proper_instantiate();

            // try to mint from random
            let err = app
                .execute_contract(
                    Addr::unchecked("random"),
                    delegator_helper.delegation_instance.clone(),
                    &ExecuteMsg::CreateDelegation {
                        percentage: Uint128::new(50),
                        cancel_time: 0,
                        expire_time: get_period(app.block_info().time.seconds()).unwrap() + 10u64,
                        id: "token_1".to_string(),
                    },
                    &[],
                )
                .unwrap_err();
            assert_eq!(
                "You can't delegate with zero voting power",
                err.root_cause().to_string()
            );
        }
    }
}
