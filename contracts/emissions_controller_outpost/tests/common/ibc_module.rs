use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_std::{
    attr, from_json, to_json_binary, Addr, Api, BankMsg, Binary, BlockInfo, ChannelResponse,
    ContractResult, CustomMsg, CustomQuery, Empty, Event, IbcChannel, IbcEndpoint, IbcMsg,
    IbcOrder, IbcQuery, Querier, QuerierResult, QuerierWrapper, QueryRequest, Storage, SystemError,
    SystemResult,
};
use cw_multi_test::error::{anyhow, AnyResult};
use cw_multi_test::{AppResponse, CosmosRouter, Ibc, Module};

use astroport_governance::utils::check_contract_supports_channel;

pub struct RouterQuerier<'a, ExecC, QueryC> {
    router: &'a dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
    api: &'a dyn Api,
    storage: &'a dyn Storage,
    block_info: &'a BlockInfo,
}

impl<'a, ExecC, QueryC> RouterQuerier<'a, ExecC, QueryC> {
    pub fn new(
        router: &'a dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        api: &'a dyn Api,
        storage: &'a dyn Storage,
        block_info: &'a BlockInfo,
    ) -> Self {
        Self {
            router,
            api,
            storage,
            block_info,
        }
    }
}

impl<'a, ExecC, QueryC> Querier for RouterQuerier<'a, ExecC, QueryC>
where
    ExecC: CustomMsg + DeserializeOwned + 'static,
    QueryC: CustomQuery + DeserializeOwned + 'static,
{
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        let request: QueryRequest<QueryC> = match from_json(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        let contract_result: ContractResult<Binary> = self
            .router
            .query(self.api, self.storage, self.block_info, request)
            .into();
        SystemResult::Ok(contract_result)
    }
}

pub struct IbcMockModule;

impl Ibc for IbcMockModule {}

impl Module for IbcMockModule {
    type ExecT = IbcMsg;
    type QueryT = IbcQuery;
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
            IbcMsg::SendPacket { channel_id, .. } => {
                let querier = RouterQuerier::new(router, api, storage, block);
                let querier = QuerierWrapper::new(&querier);
                check_contract_supports_channel(querier, &sender, &channel_id)
                    .map(|_| AppResponse::default())
                    .map_err(Into::into)
            }
            IbcMsg::Transfer {
                channel_id,
                to_address,
                amount,
                timeout,
            } => {
                // Very simplified IBC transfer processing given cosmwasm-multitest constraints
                let ibc_event = Event::new("transfer").add_attributes([
                    attr(
                        "packet_timeout_timestamp",
                        timeout.timestamp().unwrap().seconds().to_string(),
                    ),
                    attr("packet_src_port", "transfer"),
                    attr("packet_src_channel", channel_id),
                    attr("to_address", to_address),
                    attr("amount", amount.to_string()),
                ]);
                let mut response = router.execute(
                    api,
                    storage,
                    block,
                    sender.clone(),
                    BankMsg::Burn {
                        amount: vec![amount],
                    }
                    .into(),
                )?;
                response.events.push(ibc_event);
                Ok(response)
            }
            _ => unimplemented!("Execute {msg:?} not supported"),
        }
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
            IbcQuery::Channel { channel_id, .. } if channel_id == "channel-1" => {
                to_json_binary(&ChannelResponse {
                    channel: Some(IbcChannel::new(
                        IbcEndpoint {
                            port_id: "".to_string(),
                            channel_id: "channel-1".to_string(),
                        },
                        IbcEndpoint {
                            port_id: "".to_string(),
                            channel_id: "".to_string(),
                        },
                        IbcOrder::Unordered,
                        "",
                        "",
                    )),
                })
                .map_err(Into::into)
            }
            IbcQuery::Channel { .. } => {
                to_json_binary(&ChannelResponse { channel: None }).map_err(Into::into)
            }
            _ => Err(anyhow!("Query {request:?} not supported")),
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
