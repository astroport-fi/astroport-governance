use anyhow::Result;
use astroport_governance::utils::EPOCH_START;
use astroport_governance::voting_escrow_delegation::Config;
use astroport_governance::voting_escrow_delegation::{InstantiateMsg, QueryMsg};
use astroport_tests::escrow_helper::EscrowHelper;
use cosmwasm_std::{to_binary, Addr, Empty, QueryRequest, StdResult, Uint128, WasmQuery};
use cw_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};

use astroport_governance::voting_escrow_delegation::ExecuteMsg;
use cw721_base::helpers::Cw721Contract;

pub struct Helper {
    pub escrow_helper: EscrowHelper,
    pub delegation_instance: Addr,
    pub nft_instance: Addr,
    pub nft_helper: Cw721Contract<Empty, Empty>,
}

impl Helper {
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
        owner: Addr,
        escrow_addr: Addr,
        delegation_id: u64,
        nft_id: u64,
    ) -> (Addr, Addr) {
        let delegation_addr = router
            .instantiate_contract(
                delegation_id,
                owner.clone(),
                &InstantiateMsg {
                    owner: owner.to_string(),
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
            .query::<Config>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: delegation_addr.to_string(),
                msg: to_binary(&QueryMsg::Config {}).unwrap(),
            }))
            .unwrap();

        (delegation_addr, res.nft_addr)
    }

    pub fn init(router: &mut App, owner: Addr) -> Self {
        let escrow_helper = EscrowHelper::init(router, owner.clone());

        let delegation_id = router.store_code(Helper::contract_escrow_delegation_template());
        let nft_id = router.store_code(Helper::contract_nft_template());

        let (delegation_addr, nft_addr) = Helper::instantiate_delegation(
            router,
            owner,
            escrow_helper.escrow_instance.clone(),
            delegation_id,
            nft_id,
        );

        let nft_helper = cw721_base::helpers::Cw721Contract(
            nft_addr.clone(),
            Default::default(),
            Default::default(),
        );

        Helper {
            escrow_helper,
            delegation_instance: delegation_addr,
            nft_instance: nft_addr,
            nft_helper,
        }
    }

    pub fn create_delegation(
        &self,
        router: &mut App,
        user: &str,
        bps: u16,
        expire_time: u64,
        token_id: String,
        recipient: String,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.delegation_instance.clone(),
            &ExecuteMsg::CreateDelegation {
                bps,
                expire_time,
                token_id,
                recipient,
            },
            &[],
        )
    }

    pub fn extend_delegation(
        &self,
        router: &mut App,
        user: &str,
        bps: u16,
        expire_time: u64,
        token_id: String,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.delegation_instance.clone(),
            &ExecuteMsg::ExtendDelegation {
                bps,
                expire_time,
                token_id,
            },
            &[],
        )
    }

    pub fn adjusted_balance(
        &self,
        router: &mut App,
        user: &str,
        timestamp: Option<u64>,
    ) -> StdResult<Uint128> {
        router
            .wrap()
            .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.delegation_instance.to_string(),
                msg: to_binary(&QueryMsg::AdjustedBalance {
                    account: user.to_string(),
                    timestamp,
                })
                .unwrap(),
            }))
    }

    pub fn delegated_balance(
        &self,
        router: &mut App,
        user: &str,
        timestamp: Option<u64>,
    ) -> StdResult<Uint128> {
        router
            .wrap()
            .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.delegation_instance.to_string(),
                msg: to_binary(&QueryMsg::DelegatedVotingPower {
                    account: user.to_string(),
                    timestamp,
                })
                .unwrap(),
            }))
    }
}

pub fn mock_app() -> App {
    let mut app = App::default();

    app.update_block(|bi| {
        bi.time = bi.time.plus_seconds(EPOCH_START);
        bi.height += 1;
    });

    app
}
