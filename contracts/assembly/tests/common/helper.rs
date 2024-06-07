#![allow(dead_code)]

use anyhow::Result as AnyResult;
use astroport::staking;
use astroport::token::Logo;
use cosmwasm_std::testing::MockApi;
use cosmwasm_std::{
    coin, coins, Addr, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Empty, Env,
    GovMsg, IbcMsg, IbcQuery, MemoryStorage, MessageInfo, Response, StdResult, Uint128, WasmMsg,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, BasicAppBuilder, Contract, ContractWrapper, DistributionKeeper,
    Executor, FailingModule, StakeKeeper, WasmKeeper, TOKEN_FACTORY_MODULE,
};

use astroport_governance::assembly::{
    ExecuteMsg, InstantiateMsg, Proposal, ProposalVoteOption, ProposalVoterResponse,
    ProposalVotesResponse, QueryMsg, UpdateConfig, DELAY_INTERVAL, DEPOSIT_INTERVAL,
    EXPIRATION_PERIOD_INTERVAL, MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE,
    MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE, VOTING_PERIOD_INTERVAL,
};
use astroport_governance::builder_unlock::{CreateAllocationParams, Schedule};
use astroport_governance::voting_escrow::UpdateMarketingInfo;
use astroport_governance::{emissions_controller, voting_escrow};

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

fn vxastro_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_voting_escrow::contract::execute,
        astroport_voting_escrow::contract::instantiate,
        astroport_voting_escrow::contract::query,
    ))
}

fn builder_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        builder_unlock::contract::execute,
        builder_unlock::contract::instantiate,
        builder_unlock::query::query,
    ))
}

pub fn noop_contract() -> Box<dyn Contract<Empty>> {
    fn noop_execute(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: Empty,
    ) -> StdResult<Response> {
        Ok(Response::new())
    }

    fn noop_query(_deps: Deps, _env: Env, _msg: Empty) -> StdResult<Binary> {
        Ok(Default::default())
    }

    Box::new(ContractWrapper::new_with_empty(
        noop_execute,
        noop_execute,
        noop_query,
    ))
}

fn mock_emissions_controller() -> Box<dyn Contract<Empty>> {
    fn instantiate(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: Empty,
    ) -> StdResult<Response> {
        Ok(Response::default())
    }
    fn execute(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: emissions_controller::msg::ExecuteMsg<Empty>,
    ) -> StdResult<Response> {
        Ok(Response::default())
    }

    fn query(_deps: Deps, _env: Env, _msg: Empty) -> StdResult<Binary> {
        unimplemented!()
    }

    Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
}

pub const PROPOSAL_REQUIRED_DEPOSIT: Uint128 = Uint128::new(*DEPOSIT_INTERVAL.start());
pub const PROPOSAL_VOTING_PERIOD: u64 = *VOTING_PERIOD_INTERVAL.start();
pub const PROPOSAL_DELAY: u64 = *DELAY_INTERVAL.start();
pub const PROPOSAL_EXPIRATION: u64 = *EXPIRATION_PERIOD_INTERVAL.start();

pub fn default_init_msg(staking: &Addr, builder_unlock: &Addr) -> InstantiateMsg {
    InstantiateMsg {
        staking_addr: staking.to_string(),
        ibc_controller: None,
        builder_unlock_addr: builder_unlock.to_string(),
        proposal_voting_period: PROPOSAL_VOTING_PERIOD,
        proposal_effective_delay: PROPOSAL_DELAY,
        proposal_expiration_period: PROPOSAL_EXPIRATION,
        proposal_required_deposit: PROPOSAL_REQUIRED_DEPOSIT,
        proposal_required_quorum: MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE.to_string(),
        proposal_required_threshold: Decimal::from_atomics(
            MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE,
            2,
        )
        .unwrap()
        .to_string(),
        whitelisted_links: vec!["https://some.link/".to_string()],
    }
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
    pub vxastro: Addr,
    pub assembly_code_id: u64,
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
        let vxastro_code_id = app.store_code(vxastro_contract());
        let emissions_controller_code_id = app.store_code(mock_emissions_controller());

        let msg = staking::InstantiateMsg {
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

        let mocked_emission_controller = app
            .instantiate_contract(
                emissions_controller_code_id,
                owner.clone(),
                &Empty {},
                &[],
                "label",
                None,
            )
            .unwrap();
        let vxastro = app
            .instantiate_contract(
                vxastro_code_id,
                owner.clone(),
                &voting_escrow::InstantiateMsg {
                    deposit_denom: xastro_denom.to_string(),
                    emissions_controller: mocked_emission_controller.to_string(),
                    marketing: UpdateMarketingInfo {
                        project: None,
                        description: None,
                        marketing: Some(owner.to_string()),
                        logo: Some(Logo::Url("https://example.com".to_string())),
                    },
                },
                &[],
                "label",
                None,
            )
            .unwrap();

        let builder_unlock_code_id = app.store_code(builder_contract());

        let msg = astroport_governance::builder_unlock::InstantiateMsg {
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

        let assembly = app
            .instantiate_contract(
                assembly_code_id,
                owner.clone(),
                &default_init_msg(&staking, &builder_unlock),
                &[],
                String::from("Astroport Assembly"),
                Some(owner.to_string()),
            )
            .unwrap();

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

        app.execute(
            owner.clone(),
            WasmMsg::UpdateAdmin {
                contract_addr: assembly.to_string(),
                admin: assembly.to_string(),
            }
            .into(),
        )
        .unwrap();

        Ok(Self {
            app,
            owner: owner.clone(),
            staking,
            assembly,
            builder_unlock,
            xastro_denom,
            vxastro,
            assembly_code_id,
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
            &staking::ExecuteMsg::Enter { receiver: None },
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

    pub fn get_xastro(&mut self, recipient: &Addr, amount: impl Into<u128> + Copy) -> AppResponse {
        self.give_astro(amount.into(), recipient);
        self.stake(recipient, amount.into()).unwrap()
    }

    pub fn get_vxastro(&mut self, recipient: &Addr, amount: impl Into<u128> + Copy) -> AppResponse {
        let amount = amount.into();
        self.get_xastro(recipient, amount);
        self.app
            .execute_contract(
                recipient.clone(),
                self.vxastro.clone(),
                &voting_escrow::ExecuteMsg::Lock { receiver: None },
                &coins(amount, &self.xastro_denom),
            )
            .unwrap()
    }

    pub fn create_builder_allocation(&mut self, recipient: &Addr, amount: u128) {
        self.app
            .execute_contract(
                self.owner.clone(),
                self.builder_unlock.clone(),
                &astroport_governance::builder_unlock::ExecuteMsg::CreateAllocations {
                    allocations: vec![(
                        recipient.to_string(),
                        CreateAllocationParams {
                            amount: amount.into(),
                            unlock_schedule: Schedule {
                                duration: 10,
                                ..Default::default()
                            },
                        },
                    )],
                },
                &coins(amount, ASTRO_DENOM),
            )
            .unwrap();
    }

    pub fn submit_proposal(&mut self, submitter: &Addr, messages: Vec<CosmosMsg>) {
        self.app
            .execute_contract(
                submitter.clone(),
                self.assembly.clone(),
                &ExecuteMsg::SubmitProposal {
                    title: "Test title".to_string(),
                    description: "Test description".to_string(),
                    link: None,
                    messages,
                    ibc_channel: None,
                },
                &coins(PROPOSAL_REQUIRED_DEPOSIT.u128(), &self.xastro_denom),
            )
            .unwrap();
    }

    pub fn submit_sample_proposal(&mut self, submitter: &Addr) {
        let assembly = self.assembly.clone();
        self.mint_coin(&assembly, coin(1, "some_coin"));
        self.submit_proposal(
            submitter,
            vec![BankMsg::Send {
                to_address: "receiver".to_string(),
                amount: coins(1, "some_coin"),
            }
            .into()],
        );
    }

    pub fn end_proposal(&mut self, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked("permissionless"),
            self.assembly.clone(),
            &ExecuteMsg::EndProposal { proposal_id },
            &[],
        )
    }

    pub fn execute_proposal(&mut self, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked("permissionless"),
            self.assembly.clone(),
            &ExecuteMsg::ExecuteProposal { proposal_id },
            &[],
        )
    }

    pub fn query_balance(&self, addr: impl Into<String>, denom: &str) -> StdResult<Uint128> {
        self.app.wrap().query_balance(addr, denom).map(|c| c.amount)
    }

    pub fn query_xastro_bal_at(&self, user: &Addr, timestamp: Option<u64>) -> Uint128 {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.staking,
                &staking::QueryMsg::BalanceAt {
                    address: user.to_string(),
                    timestamp,
                },
            )
            .unwrap()
    }

    pub fn user_vp(&self, address: &Addr, proposal_id: u64) -> Uint128 {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.assembly,
                &QueryMsg::UserVotingPower {
                    user: address.to_string(),
                    proposal_id,
                },
            )
            .unwrap()
    }

    pub fn proposal(&self, proposal_id: u64) -> Proposal {
        self.app
            .wrap()
            .query_wasm_smart(&self.assembly, &QueryMsg::Proposal { proposal_id })
            .unwrap()
    }

    pub fn proposal_votes(&self, proposal_id: u64) -> ProposalVotesResponse {
        self.app
            .wrap()
            .query_wasm_smart(&self.assembly, &QueryMsg::ProposalVotes { proposal_id })
            .unwrap()
    }

    pub fn proposal_voters(&self, proposal_id: u64) -> Vec<ProposalVoterResponse> {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.assembly,
                &QueryMsg::ProposalVoters {
                    proposal_id,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap()
    }

    pub fn cast_vote(
        &mut self,
        proposal_id: u64,
        sender: &Addr,
        option: ProposalVoteOption,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.assembly.clone(),
            &ExecuteMsg::CastVote {
                proposal_id,
                vote: option,
            },
            &[],
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

    pub fn create_allocations(&mut self, allocations: Vec<(String, CreateAllocationParams)>) {
        let amount = allocations
            .iter()
            .map(|params| params.1.amount.u128())
            .sum();

        self.app
            .execute_contract(
                Addr::unchecked("owner"),
                self.builder_unlock.clone(),
                &astroport_governance::builder_unlock::ExecuteMsg::CreateAllocations {
                    allocations,
                },
                &coins(amount, ASTRO_DENOM),
            )
            .unwrap();
    }

    pub fn proposal_total_vp(&self, proposal_id: u64) -> StdResult<Uint128> {
        self.app
            .wrap()
            .query_wasm_smart(&self.assembly, &QueryMsg::TotalVotingPower { proposal_id })
    }

    pub fn next_block(&mut self, time: u64) {
        self.app.update_block(|block| {
            block.time = block.time.plus_seconds(time);
            block.height += 1
        });
    }

    pub fn next_block_height(&mut self, height: u64) {
        self.app.update_block(|block| {
            block.time = block.time.plus_seconds(5 * height);
            block.height += height
        });
    }
}
