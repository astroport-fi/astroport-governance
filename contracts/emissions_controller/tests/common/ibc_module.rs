use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_std::{
    to_json_binary, Addr, Api, Binary, BlockInfo, ChannelResponse, CustomMsg, CustomQuery, Empty,
    IbcChannel, IbcEndpoint, IbcMsg, IbcOrder, IbcQuery, Querier, Storage,
};
use cw_multi_test::error::{anyhow, AnyResult};
use cw_multi_test::{AppResponse, CosmosRouter, Ibc, Module};

pub struct IbcMockModule;

impl Ibc for IbcMockModule {}

impl Module for IbcMockModule {
    type ExecT = IbcMsg;
    type QueryT = IbcQuery;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _sender: Addr,
        _msg: Self::ExecT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
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
