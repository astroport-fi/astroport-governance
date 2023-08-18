#[cfg(test)]
use cosmwasm_std::from_binary;
use cosmwasm_std::{
    testing::{mock_env, mock_info, MockApi, MockQuerier, MockStorage},
    to_binary, Binary, DepsMut, Env, IbcChannel, IbcChannelConnectMsg, IbcChannelOpenMsg,
    IbcEndpoint, IbcOrder, IbcPacket, IbcQuery, ListChannelsResponse, MessageInfo, OwnedDeps,
    Timestamp, Uint128,
};

use cosmwasm_std::testing::MOCK_CONTRACT_ADDR;
use cosmwasm_std::{
    from_slice, Empty, Querier, QuerierResult, QueryRequest, SystemError, SystemResult, WasmQuery,
};

use crate::ibc::{ibc_channel_connect, ibc_channel_open, IBC_APP_VERSION};

pub const CONTRACT_PORT: &str = "ibc:wasm1234567890abcdef";
pub const CONNECTION_ID: &str = "connection-2";
pub const OWNER: &str = "owner";
pub const HUB: &str = "hub";
pub const XASTRO_TOKEN: &str = "xastro";
pub const VXASTRO_TOKEN: &str = "vxastro";

/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies.
/// This uses the Astroport CustomQuerier.
#[cfg(test)]
pub fn mock_dependencies() -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(MOCK_CONTRACT_ADDR, &[])]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
        custom_query_type: Default::default(),
    }
}

/// WasmMockQuerier will respond to requests from the custom querier,
/// providing responses to the contracts
pub struct WasmMockQuerier {
    base: MockQuerier<Empty>,
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                if contract_addr == XASTRO_TOKEN {
                    match from_binary(msg).unwrap() {
                        astroport::xastro_outpost_token::QueryMsg::BalanceAt {
                            address: _,
                            timestamp: _,
                        } => {
                            let balance = astroport::token::BalanceResponse {
                                balance: Uint128::from(1000u128),
                            };
                            SystemResult::Ok(to_binary(&balance).into())
                        }
                        _ => {
                            panic!("DO NOT ENTER HERE")
                        }
                    }
                } else {
                    match from_binary(msg).unwrap() {
                        astroport_governance::voting_escrow_lite::QueryMsg::UserDepositAt {
                            user:_,
                            timestamp:_,
                        } => {
                            let balance = astroport::token::BalanceResponse {
                                balance: Uint128::zero(),
                            };
                            SystemResult::Ok(to_binary(&balance).into())
                        }
                       astroport_governance::voting_escrow_lite::QueryMsg::UserEmissionsVotingPower {
                            user:_,
                        } => {
                            let balance = astroport_governance::voting_escrow_lite::VotingPowerResponse {
                                voting_power: Uint128::from(1000u128),
                            };
                            SystemResult::Ok(to_binary(&balance).into())
                        }
                        _ => {
                            panic!("DO NOT ENTER HERE")
                        }
                    }
                }
            }
            QueryRequest::Ibc(IbcQuery::ListChannels { .. }) => {
                let response = ListChannelsResponse {
                    channels: vec![
                        IbcChannel::new(
                            IbcEndpoint {
                                port_id: "wasm".to_string(),
                                channel_id: "channel-3".to_string(),
                            },
                            IbcEndpoint {
                                port_id: "wasm".to_string(),
                                channel_id: "channel-1".to_string(),
                            },
                            IbcOrder::Unordered,
                            "version",
                            "connection-1",
                        ),
                        IbcChannel::new(
                            IbcEndpoint {
                                port_id: "wasm".to_string(),
                                channel_id: "channel-15".to_string(),
                            },
                            IbcEndpoint {
                                port_id: "wasm".to_string(),
                                channel_id: "channel-1".to_string(),
                            },
                            IbcOrder::Unordered,
                            "version",
                            "connection-1",
                        ),
                    ],
                };
                SystemResult::Ok(to_binary(&response).into())
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<Empty>) -> Self {
        WasmMockQuerier { base }
    }
}

/// Mock the dependencies for unit tests
pub fn mock_all(
    sender: &str,
) -> (
    OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    Env,
    MessageInfo,
) {
    let deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info(sender, &[]);
    (deps, env, info)
}

/// Mock an IBC channel
pub fn mock_channel(
    our_port: &str,
    our_channel_id: &str,
    counter_port: &str,
    counter_channel: &str,
    ibc_order: IbcOrder,
    ibc_version: &str,
) -> IbcChannel {
    IbcChannel::new(
        IbcEndpoint {
            port_id: our_port.into(),
            channel_id: our_channel_id.into(),
        },
        IbcEndpoint {
            port_id: counter_port.into(),
            channel_id: counter_channel.into(),
        },
        ibc_order,
        ibc_version.to_string(),
        CONNECTION_ID,
    )
}

/// Set up a valid channel for use in tests
pub fn setup_channel(mut deps: DepsMut, env: Env) {
    let channel = mock_channel(
        "wasm.outpost",
        "channel-3",
        "wasm.hub",
        "channel-7",
        IbcOrder::Unordered,
        IBC_APP_VERSION,
    );
    let open_msg = IbcChannelOpenMsg::new_init(channel.clone());
    ibc_channel_open(deps.branch(), env.clone(), open_msg).unwrap();
    let connect_msg = IbcChannelConnectMsg::new_ack(channel, IBC_APP_VERSION);
    ibc_channel_connect(deps, env, connect_msg).unwrap();
}

/// Construct a mock IBC packet
pub fn mock_ibc_packet(remote_port: &str, my_channel: &str, data: Binary) -> IbcPacket {
    IbcPacket::new(
        data,
        IbcEndpoint {
            port_id: remote_port.to_string(),
            channel_id: "channel-3".to_string(),
        },
        IbcEndpoint {
            port_id: CONTRACT_PORT.to_string(),
            channel_id: my_channel.to_string(),
        },
        3,
        Timestamp::from_seconds(1665321069).into(),
    )
}
