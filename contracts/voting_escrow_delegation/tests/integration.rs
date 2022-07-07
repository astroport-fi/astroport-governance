#[cfg(test)]
mod tests {
    use cosmwasm_std::{Addr, Coin, Empty, Uint128};
    use cw_multi_test::{App, AppBuilder, Contract, ContractWrapper, Executor};
    use voting_escrow_delegation::msg::InstantiateMsg;

    pub fn contract_template() -> Box<dyn Contract<Empty>> {
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
    const NATIVE_DENOM: &str = "denom";

    fn mock_app() -> App {
        AppBuilder::new().build(|router, _, storage| {
            router
                .bank
                .init_balance(
                    storage,
                    &Addr::unchecked(USER),
                    vec![Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::new(1),
                    }],
                )
                .unwrap();
        })
    }

    fn proper_instantiate() -> (App, Addr, u64) {
        let mut app = mock_app();
        let cw_template_id = app.store_code(contract_template());
        let cw_nft_template_id = app.store_code(contract_nft_template());

        let msg = InstantiateMsg {
            owner: ADMIN.to_string(),
            nft_token_code_id: cw_nft_template_id,
            voting_escrow_addr: "voting_escrow_addr".to_string(),
        };
        let cw_template_contract_addr = app
            .instantiate_contract(
                cw_template_id,
                Addr::unchecked(ADMIN),
                &msg,
                &[],
                "test",
                None,
            )
            .unwrap();

        (app, cw_template_contract_addr, cw_nft_template_id)
    }

    mod queries {
        use super::*;
        use astroport_nft::QueryMsg::ContractInfo;
        use astroport_nft::{
            helpers as nft_helpers, ExecuteMsg as ExecuteMsgNFT, Extension, MintMsg,
            QueryMsg as QueryMsgNFT,
        };
        use cosmwasm_std::{to_binary, QuerierWrapper, QueryRequest, WasmQuery};
        use cw721::{ContractInfoResponse, NumTokensResponse, TokensResponse};
        use std::borrow::Borrow;
        use voting_escrow_delegation::msg::{ExecuteMsg, QueryMsg};
        use voting_escrow_delegation::state::Config;

        #[test]
        fn config() {
            let (app, cw_template_contract, _) = proper_instantiate();

            let msg = QueryMsg::Config {};
            let res = app
                .wrap()
                .query::<Config>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: cw_template_contract.to_string(),
                    msg: to_binary(&msg).unwrap(),
                }))
                .unwrap();

            assert_eq!("voting_escrow_addr", res.voting_escrow_addr.to_string());
            assert_eq!("admin", res.owner.to_string());
            assert_eq!("contract1", res.nft_token_addr.to_string())
        }

        #[test]
        fn mint() {
            let (mut app, cw_template_contract, cw_nft_template_id_inside) = proper_instantiate();
            let cw_nft_template_id = app.store_code(contract_nft_template());
            let cw_nft_template_id2 = app.store_code(contract_nft_template());
            let cw_nft_template_id3 = app.store_code(contract_nft_template());

            let msg = QueryMsg::Config {};
            let res = app
                .wrap()
                .query::<Config>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: cw_template_contract.to_string(),
                    msg: to_binary(&msg).unwrap(),
                }))
                .unwrap();

            app.execute_contract(
                cw_template_contract.clone(),
                res.nft_token_addr.clone(),
                &ExecuteMsgNFT::Mint(MintMsg::<Extension> {
                    token_id: cw_nft_template_id.to_string(),
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
                    contract_addr: res.nft_token_addr.to_string(),
                    msg: to_binary(&QueryMsgNFT::NumTokens {}).unwrap(),
                }))
                .unwrap();
            assert_eq!(1, resp.count);
            let resp = app
                .wrap()
                .query::<ContractInfoResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: res.nft_token_addr.to_string(),
                    msg: to_binary(&QueryMsgNFT::ContractInfo {}).unwrap(),
                }))
                .unwrap();
            assert_eq!("Astroport NFT", resp.name);
            assert_eq!("ASTRO-NFT", resp.symbol);

            let resp = app
                .wrap()
                .query::<TokensResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: res.nft_token_addr.to_string(),
                    msg: to_binary(&QueryMsgNFT::Tokens {
                        owner: USER.to_string(),
                        start_after: None,
                        limit: None,
                    })
                    .unwrap(),
                }))
                .unwrap();
            assert_eq!(vec!["3"], resp.tokens);

            app.execute_contract(
                cw_template_contract.clone(),
                res.nft_token_addr.clone(),
                &ExecuteMsgNFT::Mint(MintMsg::<Extension> {
                    token_id: cw_nft_template_id2.to_string(),
                    owner: USER.to_string(),
                    token_uri: None,
                    extension: None,
                }),
                &[],
            )
            .unwrap();

            app.execute_contract(
                cw_template_contract.clone(),
                res.nft_token_addr.clone(),
                &ExecuteMsgNFT::Mint(MintMsg::<Extension> {
                    token_id: cw_nft_template_id_inside.to_string(),
                    owner: USER.to_string(),
                    token_uri: None,
                    extension: None,
                }),
                &[],
            )
            .unwrap();

            app.execute_contract(
                cw_template_contract.clone(),
                res.nft_token_addr.clone(),
                &ExecuteMsgNFT::Mint(MintMsg::<Extension> {
                    token_id: cw_nft_template_id3.to_string(),
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
                    contract_addr: res.nft_token_addr.to_string(),
                    msg: to_binary(&QueryMsgNFT::NumTokens {}).unwrap(),
                }))
                .unwrap();
            assert_eq!(4, resp.count);

            let resp = app
                .wrap()
                .query::<TokensResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: res.nft_token_addr.to_string(),
                    msg: to_binary(&QueryMsgNFT::Tokens {
                        owner: USER.to_string(),
                        start_after: None,
                        limit: None,
                    })
                    .unwrap(),
                }))
                .unwrap();
            assert_eq!(vec!["2", "3", "4", "5"], resp.tokens);
        }
    }
}
