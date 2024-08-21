use astroport_governance::emissions_controller::consts::FEE_DENOM;
use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_std::{
    coins, to_json_binary, Addr, Api, BankMsg, Binary, BlockInfo, CustomMsg, CustomQuery, Empty,
    Querier, Storage,
};
use cw_multi_test::error::{anyhow, AnyResult};
use cw_multi_test::{AppResponse, CosmosRouter, MockApiBech32, Module};
use neutron_sdk::bindings::msg::{IbcFee, NeutronMsg};
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::query::min_ibc_fee::MinIbcFeeResponse;

pub struct MockNeutronModule {
    ibc_escrow: Addr,
}

impl MockNeutronModule {
    pub fn new(api: &MockApiBech32) -> Self {
        Self {
            ibc_escrow: api.addr_make("ibc_escrow"),
        }
    }
}

impl Module for MockNeutronModule {
    type ExecT = NeutronMsg;
    type QueryT = NeutronQuery;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        sender: Addr,
        msg: Self::ExecT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        match msg {
            NeutronMsg::IbcTransfer { token, .. } => {
                router.execute(
                    api,
                    storage,
                    block,
                    sender,
                    BankMsg::Send {
                        to_address: self.ibc_escrow.to_string(),
                        amount: vec![token],
                    }
                    .into(),
                )?;
            }
            _ => {}
        }

        Ok(AppResponse::default())
    }

    fn query(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        request: Self::QueryT,
    ) -> AnyResult<Binary> {
        match &request {
            NeutronQuery::MinIbcFee {} => to_json_binary(&MinIbcFeeResponse {
                min_fee: IbcFee {
                    recv_fee: vec![],
                    ack_fee: coins(100000, FEE_DENOM),
                    timeout_fee: coins(100000, FEE_DENOM),
                },
            })
            .map_err(Into::into),
            _ => Err(anyhow!("Unknown query: {request:?}")),
        }
    }

    fn sudo<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _msg: Self::SudoT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        unimplemented!()
    }
}
