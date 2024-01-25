#![allow(dead_code)]

use anyhow::Result as AnyResult;
use astroport::staking;
use cosmwasm_std::testing::MockApi;
use cosmwasm_std::{
    coins, Addr, Coin, Decimal, DepsMut, Empty, Env, GovMsg, IbcMsg, IbcQuery, MemoryStorage,
    MessageInfo, Response, StdResult, Uint128,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, BasicAppBuilder, Contract, ContractWrapper, DistributionKeeper,
    Executor, FailingModule, StakeKeeper, WasmKeeper, TOKEN_FACTORY_MODULE,
};

use astroport_governance::assembly;
use astroport_governance::assembly::{
    DELAY_INTERVAL, DEPOSIT_INTERVAL, EXPIRATION_PERIOD_INTERVAL,
    MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE, MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE,
    VOTING_PERIOD_INTERVAL,
};

use crate::common::stargate::StargateKeeper;

fn staking_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_staking::contract::execute,
            astroport_staking::contract::instantiate,
            astroport_staking::contract::query,
        )
        .with_reply_empty(astroport_staking::contract::reply),
    )
}

fn tracker_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
                unimplemented!()
            },
            astroport_tokenfactory_tracker::contract::instantiate,
            astroport_tokenfactory_tracker::query::query,
        )
        .with_sudo_empty(astroport_tokenfactory_tracker::contract::sudo),
    )
}

fn assembly_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astro_assembly::contract::execute,
        astro_assembly::contract::instantiate,
        astro_assembly::queries::query,
    ))
}

fn builder_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        builder_unlock::contract::execute,
        builder_unlock::contract::instantiate,
        builder_unlock::contract::query,
    ))
}

pub type CustomizedApp = App<
    BankKeeper,
    MockApi,
    MemoryStorage,
    FailingModule<Empty, Empty, Empty>,
    WasmKeeper<Empty, Empty>,
    StakeKeeper,
    DistributionKeeper,
    FailingModule<IbcMsg, IbcQuery, Empty>,
    FailingModule<GovMsg, Empty, Empty>,
    StargateKeeper,
>;

pub struct Helper {
    pub app: CustomizedApp,
    pub owner: Addr,
    pub staking: Addr,
    pub assembly: Addr,
    pub builder_unlock: Addr,
    pub xastro_denom: String,
}

pub const ASTRO_DENOM: &str = "factory/assembly/ASTRO";

impl Helper {
    pub fn new(owner: &Addr) -> AnyResult<Self> {
        let mut app = BasicAppBuilder::new()
            .with_stargate(StargateKeeper::default())
            .build(|router, _, storage| {
                router
                    .bank
                    .init_balance(storage, owner, coins(u128::MAX, ASTRO_DENOM))
                    .unwrap()
            });

        let staking_code_id = app.store_code(staking_contract());
        let tracker_code_id = app.store_code(tracker_contract());
        let assembly_code_id = app.store_code(assembly_contract());

        let msg = astroport::staking::InstantiateMsg {
            deposit_token_denom: ASTRO_DENOM.to_string(),
            tracking_admin: owner.to_string(),
            tracking_code_id: tracker_code_id,
            token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
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
        let staking::Config { xastro_denom, .. } = app
            .wrap()
            .query_wasm_smart(&staking, &staking::QueryMsg::Config {})
            .unwrap();

        let builder_unlock_code_id = app.store_code(builder_contract());

        let msg = astroport_governance::builder_unlock::msg::InstantiateMsg {
            owner: owner.to_string(),
            astro_denom: ASTRO_DENOM.to_string(),
            max_allocations_amount: Uint128::new(300_000_000_000000),
        };

        let builder_unlock = app
            .instantiate_contract(
                builder_unlock_code_id,
                owner.clone(),
                &msg,
                &[],
                "Builder Unlock contract".to_string(),
                Some(owner.to_string()),
            )
            .unwrap();

        let msg = assembly::InstantiateMsg {
            staking_addr: staking.to_string(),
            vxastro_token_addr: None,
            voting_escrow_delegator_addr: None,
            ibc_controller: None,
            generator_controller_addr: None,
            hub_addr: None,
            builder_unlock_addr: builder_unlock.to_string(),
            proposal_voting_period: *VOTING_PERIOD_INTERVAL.start(),
            proposal_effective_delay: *DELAY_INTERVAL.start(),
            proposal_expiration_period: *EXPIRATION_PERIOD_INTERVAL.start(),
            proposal_required_deposit: (*DEPOSIT_INTERVAL.start()).into(),
            proposal_required_quorum: MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE.to_string(),
            proposal_required_threshold: Decimal::from_atomics(
                MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE,
                2,
            )
            .unwrap()
            .to_string(),
            whitelisted_links: vec!["https://some.link/".to_string()],
        };
        let assembly = app
            .instantiate_contract(
                assembly_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("Astroport Assembly"),
                None,
            )
            .unwrap();

        Ok(Self {
            app,
            owner: owner.clone(),
            staking,
            assembly,
            builder_unlock,
            xastro_denom,
        })
    }

    pub fn give_astro(&mut self, amount: u128, recipient: &Addr) {
        self.app
            .send_tokens(
                self.owner.clone(),
                recipient.clone(),
                &coins(amount, ASTRO_DENOM),
            )
            .unwrap();
    }

    pub fn stake(&mut self, sender: &Addr, amount: u128) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.staking.clone(),
            &staking::ExecuteMsg::Enter {},
            &coins(amount, ASTRO_DENOM),
        )
    }

    pub fn unstake(&mut self, sender: &Addr, amount: u128) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.staking.clone(),
            &staking::ExecuteMsg::Leave {},
            &coins(amount, &self.xastro_denom),
        )
    }

    pub fn query_balance(&self, sender: &Addr, denom: &str) -> StdResult<Uint128> {
        self.app
            .wrap()
            .query_balance(sender, denom)
            .map(|c| c.amount)
    }

    pub fn staking_xastro_balance_at(
        &self,
        sender: &Addr,
        timestamp: Option<u64>,
    ) -> StdResult<Uint128> {
        self.app.wrap().query_wasm_smart(
            &self.staking,
            &staking::QueryMsg::BalanceAt {
                address: sender.to_string(),
                timestamp,
            },
        )
    }

    pub fn query_xastro_supply_at(&self, timestamp: Option<u64>) -> StdResult<Uint128> {
        self.app.wrap().query_wasm_smart(
            &self.staking,
            &staking::QueryMsg::TotalSupplyAt { timestamp },
        )
    }

    pub fn mint_coin(&mut self, to: &Addr, coin: Coin) {
        // .init_balance() erases previous balance thus I use such hack and create intermediate "denom admin"
        let denom_admin = Addr::unchecked(format!("{}_admin", &coin.denom));
        self.app
            .init_modules(|router, _, storage| {
                router
                    .bank
                    .init_balance(storage, &denom_admin, vec![coin.clone()])
            })
            .unwrap();

        self.app
            .send_tokens(denom_admin, to.clone(), &[coin])
            .unwrap();
    }

    pub fn next_block(&mut self, time: u64) {
        self.app.update_block(|block| {
            block.time = block.time.plus_seconds(time);
            block.height += 1
        });
    }
}
