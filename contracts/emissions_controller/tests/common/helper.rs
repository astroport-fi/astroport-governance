use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::incentives::RewardInfo;
use astroport::token::{Logo, MinterResponse};
use astroport::{factory, incentives, staking};
use cosmwasm_std::{
    coin, coins, from_json, to_json_binary, Addr, BlockInfo, Coin, Decimal, Empty, IbcEndpoint,
    IbcPacket, IbcPacketReceiveMsg, MemoryStorage, StdResult, Timestamp, Uint128,
};
use cw_multi_test::error::AnyResult;
use cw_multi_test::{
    no_init, App, AppBuilder, AppResponse, BankKeeper, BankSudo, DistributionKeeper, Executor,
    GovFailingModule, MockAddressGenerator, MockApiBech32, StakeKeeper, WasmKeeper,
};
use derivative::Derivative;
use itertools::Itertools;
use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::query::NeutronQuery;

use astroport_governance::assembly::{
    ExecuteMsg, UpdateConfig, DELAY_INTERVAL, DEPOSIT_INTERVAL, EXPIRATION_PERIOD_INTERVAL,
    MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE, MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE,
    VOTING_PERIOD_INTERVAL,
};
use astroport_governance::emissions_controller::consts::EPOCHS_START;
use astroport_governance::emissions_controller::hub::{
    EmissionsState, HubInstantiateMsg, HubMsg, OutpostInfo, SimulateTuneResponse, TuneInfo,
    UserInfoResponse, VotedPoolInfo,
};
use astroport_governance::emissions_controller::msg::VxAstroIbcMsg;
use astroport_governance::voting_escrow::UpdateMarketingInfo;
use astroport_governance::{assembly, emissions_controller, voting_escrow};

use crate::common::contracts::*;
use crate::common::ibc_module::IbcMockModule;
use crate::common::neutron_module::MockNeutronModule;
use crate::common::stargate::StargateModule;

pub const PROPOSAL_REQUIRED_DEPOSIT: Uint128 = Uint128::new(*DEPOSIT_INTERVAL.start());
pub const PROPOSAL_VOTING_PERIOD: u64 = *VOTING_PERIOD_INTERVAL.start();
pub const PROPOSAL_DELAY: u64 = *DELAY_INTERVAL.start();
pub const PROPOSAL_EXPIRATION: u64 = *EXPIRATION_PERIOD_INTERVAL.start();

pub type NeutronApp = App<
    BankKeeper,
    MockApiBech32,
    MemoryStorage,
    MockNeutronModule,
    WasmKeeper<NeutronMsg, NeutronQuery>,
    StakeKeeper,
    DistributionKeeper,
    IbcMockModule,
    GovFailingModule,
    StargateModule,
>;

fn mock_ntrn_app() -> NeutronApp {
    let api = MockApiBech32::new("neutron");
    AppBuilder::new_custom()
        .with_custom(MockNeutronModule::new(&api))
        .with_api(api)
        .with_wasm(WasmKeeper::new().with_address_generator(MockAddressGenerator))
        .with_ibc(IbcMockModule)
        .with_stargate(StargateModule)
        .with_block(BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(EPOCHS_START),
            chain_id: "cw-multitest-1".to_string(),
        })
        .build(no_init)
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct ControllerHelper {
    #[derivative(Debug = "ignore")]
    pub app: NeutronApp,
    pub owner: Addr,
    pub assembly: Addr,
    pub astro: String,
    pub xastro: String,
    pub factory: Addr,
    pub staking: Addr,
    pub vxastro: Addr,
    pub whitelisting_fee: Coin,
    pub emission_controller: Addr,
    pub incentives: Addr,
}

impl ControllerHelper {
    pub fn new() -> Self {
        let mut app = mock_ntrn_app();
        let owner = app.api().addr_make("owner");
        let astro_denom = "astro";

        let vxastro_code_id = app.store_code(vxastro_contract());
        let emissions_controller_code_id = app.store_code(emissions_controller());
        let token_code_id = app.store_code(token_contract());
        let xyk_code_id = app.store_code(pair_contract());
        let factory_code_id = app.store_code(factory_contract());
        let incentives_code_id = app.store_code(incentives_contract());
        let staking_code_id = app.store_code(staking_contract());
        let tracker_code_id = app.store_code(tracker_contract());
        let assembly_code_id = app.store_code(assembly_contract());
        let builder_code_id = app.store_code(builder_unlock_contract());

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
                    incentivization_fee_info: None,
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

        let astro_staking_amount = coins(1_000000, astro_denom);
        app.sudo(
            BankSudo::Mint {
                to_address: owner.to_string(),
                amount: astro_staking_amount.clone(),
            }
            .into(),
        )
        .unwrap();

        let msg = staking::InstantiateMsg {
            deposit_token_denom: astro_denom.to_string(),
            tracking_admin: owner.to_string(),
            tracking_code_id: tracker_code_id,
            token_factory_addr: app.api().addr_make("token_factory").to_string(),
        };
        let staking = app
            .instantiate_contract(
                staking_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("Astroport Staking"),
                None,
            )
            .unwrap();
        let xastro_denom = app
            .wrap()
            .query_wasm_smart::<staking::Config>(&staking, &staking::QueryMsg::Config {})
            .unwrap()
            .xastro_denom;

        // Lock some ASTRO in staking to get initial staking rate
        app.execute_contract(
            owner.clone(),
            staking.clone(),
            &staking::ExecuteMsg::Enter { receiver: None },
            &astro_staking_amount,
        )
        .unwrap();

        let builder_unlock_addr = app
            .instantiate_contract(
                builder_code_id,
                owner.clone(),
                &astroport_governance::builder_unlock::InstantiateMsg {
                    owner: owner.to_string(),
                    astro_denom: astro_denom.to_string(),
                    max_allocations_amount: Default::default(),
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        let assembly = app
            .instantiate_contract(
                assembly_code_id,
                owner.clone(),
                &assembly::InstantiateMsg {
                    staking_addr: staking.to_string(),
                    ibc_controller: None,
                    builder_unlock_addr: builder_unlock_addr.to_string(),
                    proposal_voting_period: PROPOSAL_VOTING_PERIOD,
                    proposal_effective_delay: PROPOSAL_DELAY,
                    proposal_expiration_period: PROPOSAL_EXPIRATION,
                    proposal_required_deposit: PROPOSAL_REQUIRED_DEPOSIT,
                    proposal_required_quorum: MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE
                        .to_string(),
                    proposal_required_threshold: Decimal::from_atomics(
                        MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE,
                        2,
                    )
                    .unwrap()
                    .to_string(),
                    whitelisted_links: vec!["https://some.link/".to_string()],
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        let whitelisting_fee = coin(1_000_000, astro_denom);
        let emission_controller = app
            .instantiate_contract(
                emissions_controller_code_id,
                owner.clone(),
                &HubInstantiateMsg {
                    owner: owner.to_string(),
                    assembly: assembly.to_string(),
                    vxastro_code_id,
                    vxastro_marketing_info: UpdateMarketingInfo {
                        project: None,
                        description: None,
                        marketing: None,
                        logo: Some(Logo::Url("".to_string())),
                    },
                    xastro_denom: xastro_denom.clone(),
                    factory: factory.to_string(),
                    astro_denom: astro_denom.to_string(),
                    pools_per_outpost: 5,
                    whitelisting_fee: whitelisting_fee.clone(),
                    fee_receiver: app.api().addr_make("fee_receiver").to_string(),
                    whitelist_threshold: Decimal::percent(1),
                    emissions_multiple: Decimal::percent(80),
                    max_astro: 1_400_000_000_000u128.into(),
                    collected_astro: 334_000_000_000u128.into(),
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        let vxastro = app
            .wrap()
            .query_wasm_smart::<emissions_controller::hub::Config>(
                &emission_controller,
                &emissions_controller::hub::QueryMsg::Config {},
            )
            .unwrap()
            .vxastro;

        app.execute_contract(
            assembly.clone(),
            assembly.clone(),
            &ExecuteMsg::UpdateConfig(Box::new(UpdateConfig {
                ibc_controller: None,
                builder_unlock_addr: None,
                proposal_voting_period: None,
                proposal_effective_delay: None,
                proposal_expiration_period: None,
                proposal_required_deposit: None,
                proposal_required_quorum: None,
                proposal_required_threshold: None,
                whitelist_remove: None,
                whitelist_add: None,
                vxastro: Some(vxastro.to_string()),
            })),
            &[],
        )
        .unwrap();

        let helper = Self {
            app,
            owner,
            xastro: xastro_denom.clone(),
            astro: astro_denom.to_string(),
            factory,
            staking,
            vxastro,
            whitelisting_fee,
            emission_controller,
            incentives,
            assembly,
        };
        dbg!(&helper);

        helper
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

    pub fn enter_staking(&mut self, user: &Addr, amount: u128) -> AnyResult<AppResponse> {
        let funds = coins(amount, &self.astro);
        self.mint_tokens(user, &funds).unwrap();
        self.app.execute_contract(
            user.clone(),
            self.staking.clone(),
            &staking::ExecuteMsg::Enter { receiver: None },
            &funds,
        )
    }

    pub fn lock(&mut self, user: &Addr, amount: u128) -> AnyResult<AppResponse> {
        let data = self.enter_staking(user, amount)?.data.unwrap();
        let mint_amount = from_json::<staking::StakingResponse>(&data)
            .unwrap()
            .xastro_amount;
        self.app.execute_contract(
            user.clone(),
            self.vxastro.clone(),
            &voting_escrow::ExecuteMsg::Lock { receiver: None },
            &coins(mint_amount.u128(), &self.xastro),
        )
    }

    pub fn withdraw(&mut self, user: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.vxastro.clone(),
            &voting_escrow::ExecuteMsg::Withdraw {},
            &[],
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

    pub fn relock(&mut self, user: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.vxastro.clone(),
            &voting_escrow::ExecuteMsg::Relock {},
            &[],
        )
    }

    pub fn timetravel(&mut self, time: u64) {
        self.app.update_block(|block| {
            block.time = block.time.plus_seconds(time);
        })
    }

    pub fn blocktravel(&mut self, blocks: u64) {
        self.app.update_block(|block| {
            block.height += blocks;
        })
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

    pub fn user_info(&self, user: &Addr, timestamp: Option<u64>) -> StdResult<UserInfoResponse> {
        self.app.wrap().query_wasm_smart(
            &self.emission_controller,
            &emissions_controller::hub::QueryMsg::UserInfo {
                user: user.to_string(),
                timestamp,
            },
        )
    }

    pub fn create_pair(&mut self, denom1: &str, denom2: &str) -> Addr {
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
            &emissions_controller::msg::ExecuteMsg::<Empty>::Vote {
                votes: votes.to_vec(),
            },
            &[],
        )
    }

    pub fn whitelist(
        &mut self,
        user: &Addr,
        pool: impl Into<String>,
        fees: &[Coin],
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::Custom(HubMsg::WhitelistPool {
                lp_token: pool.into(),
            }),
            fees,
        )
    }

    pub fn add_outpost(&mut self, prefix: &str, outpost: OutpostInfo) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            self.owner.clone(),
            self.emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::Custom(HubMsg::UpdateOutpost {
                prefix: prefix.to_string(),
                astro_denom: outpost.astro_denom,
                outpost_params: outpost.params,
                astro_pool_config: outpost.astro_pool_config,
            }),
            &[],
        )
    }

    pub fn query_voted_pool(&self, pool: &str, timestamp: Option<u64>) -> StdResult<VotedPoolInfo> {
        self.app.wrap().query_wasm_smart(
            &self.emission_controller,
            &emissions_controller::hub::QueryMsg::VotedPool {
                pool: pool.to_string(),
                timestamp,
            },
        )
    }

    pub fn query_current_emissions(&self) -> StdResult<EmissionsState> {
        self.query_tune_info(None).map(|x| x.emissions_state)
    }

    pub fn query_simulate_tune(&self) -> StdResult<SimulateTuneResponse> {
        self.app
            .wrap()
            .query_wasm_smart::<SimulateTuneResponse>(
                &self.emission_controller,
                &emissions_controller::hub::QueryMsg::SimulateTune {},
            )
            .map(|mut x| {
                x.next_pools_grouped
                    .iter_mut()
                    .for_each(|(_, array)| array.sort());
                x
            })
    }

    pub fn query_pool_vp(&self, pool: &str, timestamp: Option<u64>) -> StdResult<Uint128> {
        self.query_voted_pool(pool, timestamp)
            .map(|x| x.voting_power)
    }

    pub fn query_voted_pools(&self, limit: Option<u8>) -> StdResult<Vec<(String, VotedPoolInfo)>> {
        self.app.wrap().query_wasm_smart(
            &self.emission_controller,
            &emissions_controller::hub::QueryMsg::VotedPoolsList {
                limit,
                start_after: None,
            },
        )
    }

    pub fn query_pools_vp(&self, limit: Option<u8>) -> StdResult<Vec<(String, Uint128)>> {
        self.query_voted_pools(limit).map(|res| {
            res.into_iter()
                .sorted_by(|a, b| a.0.cmp(&b.0))
                .map(|(pool, info)| (pool, info.voting_power))
                .collect_vec()
        })
    }

    pub fn query_config(&self) -> StdResult<emissions_controller::hub::Config> {
        self.app.wrap().query_wasm_smart(
            &self.emission_controller,
            &emissions_controller::hub::QueryMsg::Config {},
        )
    }

    pub fn tune(&mut self, sender: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::Custom(HubMsg::TunePools {}),
            &[],
        )
    }

    pub fn refresh_user_votes(&mut self, sender: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::<Empty>::RefreshUserVotes {},
            &[],
        )
    }

    pub fn retry_failed_outposts(&mut self, sender: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::Custom(HubMsg::RetryFailedOutposts {}),
            &[],
        )
    }

    pub fn query_tune_info(&self, timestamp: Option<u64>) -> StdResult<TuneInfo> {
        self.app
            .wrap()
            .query_wasm_smart::<TuneInfo>(
                &self.emission_controller,
                &emissions_controller::hub::QueryMsg::TuneInfo { timestamp },
            )
            .map(|mut x| {
                x.pools_grouped
                    .iter_mut()
                    .for_each(|(_, array)| array.sort());
                x
            })
    }

    pub fn submit_proposal(&mut self, submitter: &Addr) -> AnyResult<AppResponse> {
        let deposit = coins(PROPOSAL_REQUIRED_DEPOSIT.u128(), &self.xastro);
        self.mint_tokens(submitter, &deposit).unwrap();
        self.app.execute_contract(
            submitter.clone(),
            self.assembly.clone(),
            &ExecuteMsg::SubmitProposal {
                title: "Test title".to_string(),
                description: "Test description".to_string(),
                link: None,
                messages: vec![],
                ibc_channel: None,
            },
            &deposit,
        )
    }

    pub fn register_proposal(&mut self, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            self.owner.clone(),
            self.emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::Custom(HubMsg::RegisterProposal {
                proposal_id,
            }),
            &[],
        )
    }

    pub fn mock_packet_receive(&mut self, ibc_msg: VxAstroIbcMsg) -> AnyResult<AppResponse> {
        let packet = IbcPacketReceiveMsg::new(
            IbcPacket::new(
                to_json_binary(&ibc_msg).unwrap(),
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: "".to_string(),
                },
                IbcEndpoint {
                    port_id: "".to_string(),
                    channel_id: "channel-1".to_string(),
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

    pub fn reset_astro_reward(&mut self, lp_token: &Addr) -> AnyResult<AppResponse> {
        // Mocking LP provide and depositing to incentives contract
        // NOTE:
        // it doesn't really provide assets to the pair
        // but this is fine in the context of emissions controller
        self.app
            .wrap()
            .query_wasm_smart(lp_token, &cw20_base::msg::QueryMsg::Minter {})
            .map_err(Into::into)
            .and_then(|info: MinterResponse| {
                self.app.execute_contract(
                    Addr::unchecked(info.minter),
                    lp_token.clone(),
                    &cw20_base::msg::ExecuteMsg::Mint {
                        recipient: self.owner.to_string(),
                        amount: 10000u128.into(),
                    },
                    &[],
                )
            })
            .and_then(|_| {
                self.app.execute_contract(
                    self.owner.clone(),
                    lp_token.clone(),
                    &cw20_base::msg::ExecuteMsg::Send {
                        contract: self.incentives.to_string(),
                        amount: 10000u128.into(),
                        msg: to_json_binary(&incentives::Cw20Msg::Deposit { recipient: None })
                            .unwrap(),
                    },
                    &[],
                )
            })
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
}
