use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::incentives::{InputSchedule, RewardInfo};
use astroport::token::Logo;
use astroport::{factory, incentives};
use cosmwasm_std::{
    coin, coins, to_json_binary, Addr, BlockInfo, Coin, Decimal, Empty, IbcAcknowledgement,
    IbcEndpoint, IbcPacket, IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg,
    MemoryStorage, StdResult, Timestamp, Uint128,
};
use cw_multi_test::error::AnyResult;
use cw_multi_test::{
    no_init, App, AppBuilder, AppResponse, BankKeeper, BankSudo, DistributionKeeper, Executor,
    FailingModule, GovFailingModule, MockAddressGenerator, MockApiBech32, StakeKeeper, WasmKeeper,
};
use derivative::Derivative;

use astroport_governance::assembly::ProposalVoteOption;
use astroport_governance::emissions_controller::consts::{EPOCHS_START, EPOCH_LENGTH};
use astroport_governance::emissions_controller::msg::{ExecuteMsg, IbcAckResult, VxAstroIbcMsg};
use astroport_governance::emissions_controller::outpost::{
    OutpostInstantiateMsg, OutpostMsg, RegisteredProposal,
};
use astroport_governance::voting_escrow::{LockInfoResponse, UpdateMarketingInfo};
use astroport_governance::{emissions_controller, voting_escrow};

use crate::common::contracts::*;
use crate::common::ibc_module::IbcMockModule;
use crate::common::stargate::StargateModule;

pub type OutpostApp = App<
    BankKeeper,
    MockApiBech32,
    MemoryStorage,
    FailingModule<Empty, Empty, Empty>,
    WasmKeeper<Empty, Empty>,
    StakeKeeper,
    DistributionKeeper,
    IbcMockModule,
    GovFailingModule,
    StargateModule,
>;

fn mock_app() -> OutpostApp {
    AppBuilder::new()
        .with_ibc(IbcMockModule)
        .with_api(MockApiBech32::new("osmo"))
        .with_wasm(WasmKeeper::new().with_address_generator(MockAddressGenerator))
        .with_stargate(StargateModule)
        .with_block(BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(EPOCHS_START),
            chain_id: "cw-multitest-1".to_string(),
        })
        .build(no_init)
}

/// Normalize current timestamp to the beginning of the current epoch (Monday).
pub fn get_epoch_start(timestamp: u64) -> u64 {
    let rem = timestamp % EPOCHS_START;
    if rem % EPOCH_LENGTH == 0 {
        // Hit at the beginning of the current epoch
        timestamp
    } else {
        // Hit somewhere in the middle
        EPOCHS_START + rem / EPOCH_LENGTH * EPOCH_LENGTH
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct ControllerHelper {
    #[derivative(Debug = "ignore")]
    pub app: OutpostApp,
    pub owner: Addr,
    pub astro: String,
    pub xastro: String,
    pub factory: Addr,
    pub vxastro: Addr,
    pub emission_controller: Addr,
    pub incentives: Addr,
}

impl ControllerHelper {
    pub fn new() -> Self {
        let mut app = mock_app();
        let owner = app.api().addr_make("owner");
        let astro_denom = "astro";
        let xastro_denom = "xastro";

        let vxastro_code_id = app.store_code(vxastro_contract());
        let emissions_controller_code_id = app.store_code(emissions_controller());
        let token_code_id = app.store_code(token_contract());
        let xyk_code_id = app.store_code(pair_contract());
        let factory_code_id = app.store_code(factory_contract());
        let incentives_code_id = app.store_code(incentives_contract());

        let factory = app
            .instantiate_contract(
                factory_code_id,
                owner.clone(),
                &factory::InstantiateMsg {
                    pair_configs: vec![PairConfig {
                        code_id: xyk_code_id,
                        pair_type: PairType::Xyk {},
                        total_fee_bps: 0,
                        maker_fee_bps: 0,
                        is_disabled: false,
                        is_generator_disabled: false,
                        permissioned: false,
                    }],
                    token_code_id,
                    fee_address: None,
                    generator_address: None,
                    owner: owner.to_string(),
                    whitelist_code_id: 0,
                    coin_registry_address: app.api().addr_make("coin_registry").to_string(),
                    tracker_config: None,
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        let incentives = app
            .instantiate_contract(
                incentives_code_id,
                owner.clone(),
                &incentives::InstantiateMsg {
                    owner: owner.to_string(),
                    factory: factory.to_string(),
                    astro_token: AssetInfo::native(astro_denom),
                    vesting_contract: app.api().addr_make("vesting").to_string(),
                    incentivization_fee_info: Some(incentives::IncentivizationFeeInfo {
                        fee_receiver: app.api().addr_make("maker"),
                        fee: coin(250_000000, astro_denom),
                    }),
                    guardian: None,
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        app.execute_contract(
            owner.clone(),
            factory.clone(),
            &factory::ExecuteMsg::UpdateConfig {
                token_code_id: None,
                fee_address: None,
                generator_address: Some(incentives.to_string()),
                whitelist_code_id: None,
                coin_registry_address: None,
            },
            &[],
        )
        .unwrap();

        let emission_controller = app
            .instantiate_contract(
                emissions_controller_code_id,
                owner.clone(),
                &OutpostInstantiateMsg {
                    owner: owner.to_string(),
                    astro_denom: astro_denom.to_string(),
                    vxastro_code_id,
                    vxastro_marketing_info: UpdateMarketingInfo {
                        project: None,
                        description: None,
                        marketing: None,
                        logo: Logo::Url("".to_string()),
                    },
                    xastro_denom: xastro_denom.to_string(),
                    factory: factory.to_string(),
                    hub_emissions_controller: "emissions_controller".to_string(),
                    ics20_channel: "channel-2".to_string(),
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        let vxastro = app
            .wrap()
            .query_wasm_smart::<emissions_controller::outpost::Config>(
                &emission_controller,
                &emissions_controller::outpost::QueryMsg::Config {},
            )
            .unwrap()
            .vxastro;

        Self {
            app,
            owner,
            xastro: xastro_denom.to_string(),
            astro: astro_denom.to_string(),
            factory,
            vxastro,
            emission_controller,
            incentives,
        }
    }

    pub fn mint_tokens(&mut self, user: &Addr, coins: &[Coin]) -> AnyResult<AppResponse> {
        self.app.sudo(
            BankSudo::Mint {
                to_address: user.to_string(),
                amount: coins.to_vec(),
            }
            .into(),
        )
    }

    pub fn lock(&mut self, user: &Addr, amount: u128) -> AnyResult<AppResponse> {
        let funds = coins(amount, &self.xastro);
        self.mint_tokens(user, &funds).unwrap();
        self.app.execute_contract(
            user.clone(),
            self.vxastro.clone(),
            &voting_escrow::ExecuteMsg::Lock { receiver: None },
            &funds,
        )
    }

    pub fn unlock(&mut self, user: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.vxastro.clone(),
            &voting_escrow::ExecuteMsg::Unlock {},
            &[],
        )
    }

    pub fn user_vp(&self, user: &Addr, timestamp: Option<u64>) -> StdResult<Uint128> {
        self.app.wrap().query_wasm_smart(
            &self.vxastro,
            &voting_escrow::QueryMsg::UserVotingPower {
                user: user.to_string(),
                timestamp,
            },
        )
    }

    pub fn create_pair(&mut self, denom1: &str, denom2: &str) -> String {
        let asset_infos = vec![AssetInfo::native(denom1), AssetInfo::native(denom2)];
        self.app
            .execute_contract(
                self.owner.clone(),
                self.factory.clone(),
                &factory::ExecuteMsg::CreatePair {
                    pair_type: PairType::Xyk {},
                    asset_infos: asset_infos.clone(),
                    init_params: None,
                },
                &[],
            )
            .unwrap();

        self.app
            .wrap()
            .query_wasm_smart::<PairInfo>(&self.factory, &factory::QueryMsg::Pair { asset_infos })
            .unwrap()
            .liquidity_token
    }

    pub fn vote(&mut self, user: &Addr, votes: &[(String, Decimal)]) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.emission_controller.clone(),
            &ExecuteMsg::<Empty>::Vote {
                votes: votes.to_vec(),
            },
            &[],
        )
    }

    pub fn set_emissions(
        &mut self,
        sender: &Addr,
        schedules: &[(&str, InputSchedule)],
        funds: &[Coin],
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::<OutpostMsg>::Custom(
                OutpostMsg::SetEmissions {
                    schedules: schedules
                        .iter()
                        .map(|(pool, schedule)| (pool.to_string(), schedule.clone()))
                        .collect(),
                },
            ),
            funds,
        )
    }

    pub fn permissioned_set_emissions(
        &mut self,
        user: &Addr,
        schedules: &[(&str, InputSchedule)],
        funds: &[Coin],
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::<OutpostMsg>::Custom(
                OutpostMsg::PermissionedSetEmissions {
                    schedules: schedules
                        .iter()
                        .map(|(pool, schedule)| (pool.to_string(), schedule.clone()))
                        .collect(),
                },
            ),
            funds,
        )
    }

    pub fn query_config(&self) -> StdResult<emissions_controller::outpost::Config> {
        self.app.wrap().query_wasm_smart(
            &self.emission_controller,
            &emissions_controller::outpost::QueryMsg::Config {},
        )
    }

    pub fn timetravel(&mut self, time: u64) {
        self.app.update_block(|block| {
            block.time = block.time.plus_seconds(time);
        })
    }

    pub fn withdraw(&mut self, user: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.vxastro.clone(),
            &voting_escrow::ExecuteMsg::Withdraw {},
            &[],
        )
    }

    pub fn query_rewards(&self, pool: impl Into<String>) -> StdResult<Vec<RewardInfo>> {
        self.app
            .wrap()
            .query_wasm_smart::<incentives::PoolInfoResponse>(
                &self.incentives,
                &incentives::QueryMsg::PoolInfo {
                    lp_token: pool.into(),
                },
            )
            .map(|x| x.rewards)
    }

    pub fn lock_info(&self, user: &Addr, timestamp: Option<u64>) -> StdResult<LockInfoResponse> {
        self.app.wrap().query_wasm_smart(
            &self.vxastro,
            &voting_escrow::QueryMsg::LockInfo {
                user: user.to_string(),
                timestamp,
            },
        )
    }

    pub fn refresh_user(&mut self, user: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.emission_controller.clone(),
            &ExecuteMsg::<Empty>::RefreshUserVotes {},
            &[],
        )
    }

    pub fn query_ibc_status(
        &self,
        user: &Addr,
    ) -> StdResult<emissions_controller::outpost::UserIbcStatus> {
        self.app.wrap().query_wasm_smart(
            &self.emission_controller,
            &emissions_controller::outpost::QueryMsg::QueryUserIbcStatus {
                user: user.to_string(),
            },
        )
    }

    pub fn set_voting_channel(&mut self) {
        self.update_config(
            &self.owner.clone(),
            Some("channel-1".to_string()),
            None,
            None,
        )
        .unwrap();
    }

    pub fn mock_ibc_ack(
        &mut self,
        ibc_msg: VxAstroIbcMsg,
        error: Option<&str>,
    ) -> AnyResult<AppResponse> {
        let ack_result = if let Some(err) = error {
            IbcAckResult::Error(err.to_string())
        } else {
            IbcAckResult::Ok(b"null".into())
        };
        let packet = IbcPacketAckMsg::new(
            IbcAcknowledgement::encode_json(&ack_result).unwrap(),
            IbcPacket::new(
                to_json_binary(&ibc_msg).unwrap(),
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: "".to_string(),
                },
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: "".to_string(),
                },
                0,
                Timestamp::from_seconds(0).into(),
            ),
            Addr::unchecked("relayer"),
        );
        self.app
            .wasm_sudo(self.emission_controller.clone(), &TestSudoMsg::Ack(packet))
    }

    pub fn mock_ibc_timeout(&mut self, ibc_msg: VxAstroIbcMsg) -> AnyResult<AppResponse> {
        let packet = IbcPacketTimeoutMsg::new(
            IbcPacket::new(
                to_json_binary(&ibc_msg).unwrap(),
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: "".to_string(),
                },
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: "".to_string(),
                },
                0,
                Timestamp::from_seconds(0).into(),
            ),
            Addr::unchecked("relayer"),
        );
        self.app.wasm_sudo(
            self.emission_controller.clone(),
            &TestSudoMsg::Timeout(packet),
        )
    }

    pub fn mock_packet_receive(
        &mut self,
        ibc_msg: VxAstroIbcMsg,
        dst_channel: &str,
    ) -> AnyResult<AppResponse> {
        let packet = IbcPacketReceiveMsg::new(
            IbcPacket::new(
                to_json_binary(&ibc_msg).unwrap(),
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: "".to_string(),
                },
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: dst_channel.to_string(),
                },
                0,
                Timestamp::from_seconds(0).into(),
            ),
            Addr::unchecked("relayer"),
        );
        self.app.wasm_sudo(
            self.emission_controller.clone(),
            &TestSudoMsg::IbcRecv(packet),
        )
    }

    pub fn update_config(
        &mut self,
        sender: &Addr,
        voting_ibc_channel: Option<String>,
        hub_emissions_controller: Option<String>,
        ics20_channel: Option<String>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.emission_controller.clone(),
            &ExecuteMsg::Custom(OutpostMsg::UpdateConfig {
                voting_ibc_channel,
                hub_emissions_controller,
                ics20_channel,
            }),
            &[],
        )
    }

    pub fn cast_vote(&mut self, user: &Addr, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.emission_controller.clone(),
            &ExecuteMsg::Custom(OutpostMsg::CastVote {
                proposal_id,
                vote: ProposalVoteOption::For,
            }),
            &[],
        )
    }

    pub fn is_prop_registered(&self, proposal_id: u64) -> bool {
        self.app
            .wrap()
            .query_wasm_smart::<Vec<RegisteredProposal>>(
                &self.emission_controller,
                &emissions_controller::outpost::QueryMsg::QueryRegisteredProposals {
                    limit: Some(100),
                    start_after: None,
                },
            )
            .map(|proposals| proposals.iter().any(|p| p.id == proposal_id))
            .unwrap()
    }
}
