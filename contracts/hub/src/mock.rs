use std::cell::Cell;

#[cfg(test)]
use cosmwasm_std::{from_binary, Uint64};
use cosmwasm_std::{
    testing::{mock_env, mock_info, MockApi, MockQuerier, MockStorage},
    to_binary, Addr, Binary, ChannelResponse, DepsMut, Env, IbcChannel, IbcChannelConnectMsg,
    IbcChannelOpenMsg, IbcEndpoint, IbcOrder, IbcPacket, IbcQuery, MessageInfo, OwnedDeps,
    Timestamp, Uint128,
};

use cosmwasm_std::testing::MOCK_CONTRACT_ADDR;
use cosmwasm_std::{
    from_slice, Empty, Querier, QuerierResult, QueryRequest, SystemError, SystemResult, WasmQuery,
};
use cw20::BalanceResponse as Cw20BalanceResponse;

use crate::ibc::{ibc_channel_connect, ibc_channel_open, IBC_APP_VERSION};

pub const CONTRACT_PORT: &str = "ibc:wasm1234567890abcdef";
pub const REMOTE_PORT: &str = "wasm.outpost";
pub const CONNECTION_ID: &str = "connection-2";
pub const OWNER: &str = "owner";
pub const ASSEMBLY: &str = "assembly";
pub const CW20ICS20: &str = "cw20_ics20";
pub const GENERATOR_CONTROLLER: &str = "generator_controller";
pub const STAKING: &str = "staking";
pub const ASTRO_TOKEN: &str = "astro";
pub const XASTRO_TOKEN: &str = "xastro";

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
    xastro_balance: Cell<Uint128>,
    astro_balance: Cell<Uint128>,
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
                if contract_addr == STAKING {
                    match from_binary(msg).unwrap() {
                        astroport::staking::QueryMsg::Config {} => {
                            let config = astroport::staking::ConfigResponse {
                                deposit_token_addr: Addr::unchecked("astro"),
                                share_token_addr: Addr::unchecked("xastro"),
                            };

                            SystemResult::Ok(to_binary(&config).into())
                        }
                        _ => {
                            panic!("DO NOT ENTER HERE")
                        }
                    }
                } else {
                    if contract_addr == ASTRO_TOKEN {
                        // Manually increase the ASTRO balance every query
                        // to help tests
                        let response = Cw20BalanceResponse {
                            balance: self.astro_balance.get(),
                        };
                        self.astro_balance.set(
                            self.astro_balance
                                .get()
                                .checked_add(Uint128::from(100u128))
                                .unwrap(),
                        );
                        return SystemResult::Ok(to_binary(&response).into());
                    }
                    if contract_addr == XASTRO_TOKEN {
                        // Manually increase the ASTRO balance every query
                        // to help tests
                        let response = Cw20BalanceResponse {
                            balance: self.xastro_balance.get(),
                        };
                        self.xastro_balance.set(
                            self.xastro_balance
                                .get()
                                .checked_add(Uint128::from(100u128))
                                .unwrap(),
                        );
                        return SystemResult::Ok(to_binary(&response).into());
                    }
                    if contract_addr != ASSEMBLY {
                        return SystemResult::Err(SystemError::Unknown {});
                    }
                    match from_binary(msg).unwrap() {
                        astroport_governance::assembly::QueryMsg::Proposal { proposal_id } => {
                            let proposal = astroport_governance::assembly::Proposal {
                                proposal_id: Uint64::from(proposal_id),
                                submitter: Addr::unchecked("submitter"),
                                status: astroport_governance::assembly::ProposalStatus::Active,
                                for_power: Uint128::zero(),
                                outpost_against_power: Uint128::zero(),
                                against_power: Uint128::zero(),
                                outpost_for_power: Uint128::zero(),
                                for_voters: vec![],
                                against_voters: vec![],
                                start_block: 1,
                                start_time: 1571797419,
                                end_block: 5,
                                delayed_end_block: 10,
                                expiration_block: 15,
                                title: "Test title".to_string(),
                                description: "Test description".to_string(),
                                link: None,
                                messages: None,
                                deposit_amount: Uint128::one(),
                                ibc_channel: None,
                            };
                            SystemResult::Ok(to_binary(&proposal).into())
                        }
                        _ => {
                            panic!("DO NOT ENTER HERE")
                        }
                    }
                }
            }
            QueryRequest::Ibc(IbcQuery::Channel { .. }) => {
                let response = ChannelResponse {
                    channel: Some(IbcChannel::new(
                        IbcEndpoint {
                            port_id: "wasm".to_string(),
                            channel_id: "channel-1".to_string(),
                        },
                        IbcEndpoint {
                            port_id: "wasm".to_string(),
                            channel_id: "channel-1".to_string(),
                        },
                        IbcOrder::Unordered,
                        "version",
                        "connection-1",
                    )),
                };
                SystemResult::Ok(to_binary(&response).into())
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<Empty>) -> Self {
        WasmMockQuerier {
            base,
            xastro_balance: Cell::new(Uint128::zero()),
            astro_balance: Cell::new(Uint128::zero()),
        }
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
        "wasm.hub",
        "channel-3",
        "wasm.outpost",
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
pub fn mock_ibc_packet(my_channel: &str, data: Binary) -> IbcPacket {
    IbcPacket::new(
        data,
        IbcEndpoint {
            port_id: REMOTE_PORT.to_string(),
            channel_id: "channel-7".to_string(),
        },
        IbcEndpoint {
            port_id: CONTRACT_PORT.to_string(),
            channel_id: my_channel.to_string(),
        },
        3,
        Timestamp::from_seconds(1665321069).into(),
    )
}
