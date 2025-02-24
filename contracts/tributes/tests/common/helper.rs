use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory;
use astroport::factory::{PairConfig, PairType};
use astroport::token::Logo;
use cosmwasm_std::{
    coin, coins, Addr, BlockInfo, Coin, Decimal, Empty, MemoryStorage, StdResult, Timestamp,
};
use cw_multi_test::error::AnyResult;
use cw_multi_test::{
    no_init, App, AppBuilder, AppResponse, BankKeeper, BankSudo, DistributionKeeper, Executor,
    FailingModule, GovFailingModule, IbcFailingModule, MockAddressGenerator, MockApiBech32,
    StakeKeeper, WasmKeeper,
};
use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::query::NeutronQuery;

use astroport_governance::emissions_controller::consts::EPOCHS_START;
use astroport_governance::emissions_controller::hub::{HubInstantiateMsg, HubMsg};
use astroport_governance::tributes::TributeFeeInfo;
use astroport_governance::voting_escrow::UpdateMarketingInfo;
use astroport_governance::{emissions_controller, tributes, voting_escrow};

use crate::common::contracts::*;
use crate::common::stargate::StargateModule;

const PREFIX: &str = "neutron";

pub type NeutronApp = App<
    BankKeeper,
    MockApiBech32,
    MemoryStorage,
    FailingModule<NeutronMsg, NeutronQuery, Empty>,
    WasmKeeper<NeutronMsg, NeutronQuery>,
    StakeKeeper,
    DistributionKeeper,
    IbcFailingModule,
    GovFailingModule,
    StargateModule,
>;

fn mock_app() -> NeutronApp {
    AppBuilder::new_custom()
        .with_custom(FailingModule::default())
        .with_api(MockApiBech32::new(PREFIX))
        .with_wasm(WasmKeeper::default().with_address_generator(MockAddressGenerator))
        .with_stargate(StargateModule)
        .with_block(BlockInfo {
            height: 1,
            time: Timestamp::from_seconds(EPOCHS_START),
            chain_id: "cw-multitest-1".to_string(),
        })
        .build(no_init)
}

pub struct Helper {
    pub app: NeutronApp,
    pub owner: Addr,
    pub astro: String,
    pub xastro: String,
    pub factory: Addr,
    pub vxastro: Addr,
    pub tributes: Addr,
    pub fee: Coin,
    pub emission_controller: Addr,
}

impl Helper {
    pub fn new() -> Self {
        let mut app = mock_app();
        let owner = app.api().addr_make("owner");
        let astro_denom = "astro";

        let vxastro_code_id = app.store_code(vxastro_contract());
        let emissions_controller_code_id = app.store_code(emissions_controller());
        let xyk_code_id = app.store_code(pair_contract());
        let factory_code_id = app.store_code(factory_contract());
        let incentives_code_id = app.store_code(incentives_contract());
        let tributes_code_id = app.store_code(tributes_contract());

        let mocked_incentives = app
            .instantiate_contract(
                incentives_code_id,
                owner.clone(),
                &Empty {},
                &[],
                "label",
                None,
            )
            .unwrap();

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
                    token_code_id: 111, // deprecated
                    fee_address: None,
                    generator_address: Some(mocked_incentives.to_string()),
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

        let whitelisting_fee = coin(1_000000, astro_denom);
        let xastro_denom = format!("factory/{owner}/xASTRO");

        // Mocked initial xASTRO supply
        app.sudo(
            BankSudo::Mint {
                to_address: owner.to_string(),
                amount: coins(1_000000, &xastro_denom),
            }
            .into(),
        )
        .unwrap();

        let emission_controller = app
            .instantiate_contract(
                emissions_controller_code_id,
                owner.clone(),
                &HubInstantiateMsg {
                    owner: owner.to_string(),
                    assembly: app.api().addr_make("assembly").to_string(),
                    vxastro_code_id,
                    vxastro_marketing_info: UpdateMarketingInfo {
                        project: None,
                        description: None,
                        marketing: None,
                        logo: Logo::Url("".to_string()),
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
                    ema: 300_000_000_000u128.into(),
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        // Add dummy "hub" outpost
        app.execute_contract(
            owner.clone(),
            emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::Custom(HubMsg::UpdateOutpost {
                prefix: PREFIX.to_string(),
                astro_denom: astro_denom.to_string(),
                outpost_params: None,
                astro_pool_config: None,
            }),
            &[],
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

        let tributes = app
            .instantiate_contract(
                tributes_code_id,
                owner.clone(),
                &tributes::InstantiateMsg {
                    owner: owner.to_string(),
                    emissions_controller: emission_controller.to_string(),
                    tribute_fee_info: TributeFeeInfo {
                        fee: whitelisting_fee.clone(),
                        fee_collector: app.api().addr_make("tributes_fee_receiver"),
                    },
                    rewards_limit: 10,
                    token_transfer_gas_limit: 600_000,
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        Self {
            app,
            owner,
            astro: astro_denom.to_string(),
            xastro: xastro_denom,
            factory,
            vxastro,
            tributes,
            fee: whitelisting_fee,
            emission_controller,
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

    pub fn timetravel(&mut self, time: u64) {
        self.app.update_block(|block| {
            block.time = block.time.plus_seconds(time);
        })
    }

    pub fn lock(&mut self, user: &Addr, amount: u128) -> AnyResult<AppResponse> {
        let funds = coins(amount, &self.xastro);
        self.mint_tokens(user, &funds)?;
        self.app.execute_contract(
            user.clone(),
            self.vxastro.clone(),
            &voting_escrow::ExecuteMsg::Lock { receiver: None },
            &funds,
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
            &emissions_controller::msg::ExecuteMsg::<Empty>::Vote {
                votes: votes.to_vec(),
            },
            &[],
        )
    }

    pub fn whitelist(&mut self, pool: impl Into<String>) -> AnyResult<AppResponse> {
        let fee = [self.fee.clone()];
        self.mint_tokens(&self.owner.clone(), &fee)?;
        self.app.execute_contract(
            self.owner.clone(),
            self.emission_controller.clone(),
            &emissions_controller::msg::ExecuteMsg::Custom(HubMsg::WhitelistPool {
                lp_token: pool.into(),
            }),
            &fee,
        )
    }

    pub fn query_config(&self) -> StdResult<tributes::Config> {
        self.app.wrap().query_wasm_smart(
            &self.tributes,
            &emissions_controller::hub::QueryMsg::Config {},
        )
    }

    pub fn add_tribute(
        &mut self,
        sender: &Addr,
        lp_token: &str,
        tribute: &Asset,
        funds: &[Coin],
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.tributes.clone(),
            &tributes::ExecuteMsg::AddTribute {
                lp_token: lp_token.to_string(),
                asset: tribute.clone(),
            },
            funds,
        )
    }

    pub fn query_pool_tributes(
        &self,
        lp_token: &str,
        timestamp: Option<u64>,
    ) -> StdResult<Vec<Asset>> {
        self.app.wrap().query_wasm_smart(
            &self.tributes,
            &tributes::QueryMsg::QueryPoolTributes {
                epoch_ts: timestamp,
                lp_token: lp_token.to_string(),
            },
        )
    }

    pub fn query_all_epoch_tributes(
        &self,
        timestamp: Option<u64>,
        start_after: Option<(String, AssetInfo)>,
    ) -> StdResult<Vec<(String, Asset)>> {
        self.app.wrap().query_wasm_smart(
            &self.tributes,
            &tributes::QueryMsg::QueryAllEpochTributes {
                epoch_ts: timestamp,
                start_after,
                limit: None,
            },
        )
    }

    pub fn simulate_claim(&self, address: impl Into<String>) -> StdResult<Vec<Asset>> {
        self.app.wrap().query_wasm_smart(
            &self.tributes,
            &tributes::QueryMsg::SimulateClaim {
                address: address.into(),
            },
        )
    }

    pub fn claim(&mut self, sender: &Addr, receiver: Option<String>) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.tributes.clone(),
            &tributes::ExecuteMsg::Claim { receiver },
            &[],
        )
    }

    pub fn update_config(
        &mut self,
        sender: &Addr,
        tribute_fee_info: Option<TributeFeeInfo>,
        rewards_limit: Option<u8>,
        token_transfer_gas_limit: Option<u64>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.tributes.clone(),
            &tributes::ExecuteMsg::UpdateConfig {
                tribute_fee_info,
                rewards_limit,
                token_transfer_gas_limit,
            },
            &[],
        )
    }

    pub fn remove_tribute(
        &mut self,
        sender: &Addr,
        lp_token: &str,
        asset_info: &AssetInfo,
        receiver: impl Into<String>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.tributes.clone(),
            &tributes::ExecuteMsg::RemoveTribute {
                lp_token: lp_token.to_string(),
                asset_info: asset_info.clone(),
                receiver: receiver.into(),
            },
            &[],
        )
    }
}
