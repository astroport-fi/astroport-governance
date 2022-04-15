use std::cmp::Ordering;
use std::collections::HashMap;

use anyhow::Result;
use cosmwasm_std::{Addr, Uint128};
use itertools::Itertools;
use proptest::prelude::*;
use terra_multi_test::{AppResponse, Executor, TerraApp};

use astroport_governance::generator_controller::ExecuteMsg;
use astroport_governance::utils::{calc_voting_power, MAX_LOCK_TIME, WEEK};
use generator_controller::bps::BasicPoints;
use Event::*;
use VeEvent::*;

use astroport_tests::{
    controller_helper::ControllerHelper, escrow_helper::MULTIPLIER, mock_app, TerraAppExtension,
};

#[derive(Clone, Debug)]
enum Event {
    Vote(Vec<((String, String), u16)>),
    GaugePools,
    ChangePoolLimit(u64),
}

#[derive(Clone, Debug)]
enum VeEvent {
    CreateLock(f64, u64),
    IncreaseTime(u64),
    ExtendLock(f64),
    Withdraw,
}

struct Simulator {
    user_votes: HashMap<String, HashMap<String, u16>>,
    locks: HashMap<String, (Uint128, u64, f32)>,
    helper: ControllerHelper,
    router: TerraApp,
    owner: Addr,
    limit: u64,
    pairs: HashMap<(String, String), Addr>,
}

impl Simulator {
    pub fn init<T: Clone + Into<String>>(users: &[T]) -> Self {
        let mut router = mock_app();
        let owner = Addr::unchecked("owner");
        Self {
            helper: ControllerHelper::init(&mut router, &owner),
            user_votes: users
                .iter()
                .cloned()
                .map(|user| (user.into(), HashMap::new()))
                .collect(),
            locks: HashMap::new(),
            limit: 5,
            pairs: HashMap::new(),
            router,
            owner,
        }
    }

    fn escrow_events_router(&mut self, user: &str, event: VeEvent) {
        // We don't check voting escrow errors
        let _ = match event {
            CreateLock(amount, interval) => {
                self.helper
                    .escrow_helper
                    .mint_xastro(&mut self.router, user, amount as u64);
                self.helper.escrow_helper.create_lock(
                    &mut self.router,
                    user,
                    interval,
                    amount as f32,
                )
            }
            IncreaseTime(interval) => {
                self.helper
                    .escrow_helper
                    .extend_lock_time(&mut self.router, user, interval)
            }
            ExtendLock(amount) => {
                self.helper
                    .escrow_helper
                    .mint_xastro(&mut self.router, user, amount as u64);
                self.helper
                    .escrow_helper
                    .extend_lock_amount(&mut self.router, user, amount as f32)
            }
            Withdraw => self.helper.escrow_helper.withdraw(&mut self.router, user),
        };
    }

    fn vote(&mut self, user: &str, votes: Vec<((String, String), u16)>) -> Result<AppResponse> {
        let votes: Vec<_> = votes
            .iter()
            .map(|(tokens, bps)| {
                let addr = self
                    .pairs
                    .get(tokens)
                    .cloned()
                    .expect(&format!("Pair {}-{} was not found", tokens.0, tokens.1));
                (addr, *bps)
            })
            .collect();
        self.helper
            .vote(&mut self.router, user, votes.clone())
            .map(|response| {
                let lock_info = self
                    .helper
                    .escrow_helper
                    .query_lock_info(&mut self.router, user)
                    .unwrap();
                let vp = self
                    .helper
                    .escrow_helper
                    .query_user_vp(&mut self.router, user)
                    .unwrap();
                self.locks.insert(
                    user.to_string(),
                    (lock_info.slope, self.router.block_period(), vp),
                );
                self.user_votes.insert(user.to_string(), HashMap::new());
                for (pool, bps) in votes {
                    self.user_votes
                        .get_mut(user)
                        .expect("User not found!")
                        .insert(pool.to_string(), bps);
                }
                let user_info = self.helper.query_user_info(&mut self.router, user).unwrap();
                let total_apoints: u16 = user_info
                    .votes
                    .iter()
                    .cloned()
                    .map(|pair| u16::from(pair.1))
                    .sum();
                if total_apoints > 10000 {
                    panic!("{} > 10000", total_apoints)
                }
                assert_eq!(user_info.vote_ts, self.router.block_info().time.seconds());
                response
            })
    }

    fn change_pool_limit(&mut self, limit: u64) -> Result<AppResponse> {
        self.router
            .execute_contract(
                self.owner.clone(),
                self.helper.controller.clone(),
                &ExecuteMsg::ChangePoolsLimit { limit },
                &[],
            )
            .map(|response| {
                self.limit = limit;
                response
            })
    }

    pub fn event_router(&mut self, user: &str, event: Event) {
        println!("User {} Event {:?}", user, event);
        match event {
            Vote(votes) => {
                if let Err(err) = self.vote(user, votes) {
                    println!("{}", err);
                }
            }
            GaugePools => {
                if let Err(err) = self.helper.tune(&mut self.router) {
                    println!("{}", err);
                }
            }
            ChangePoolLimit(limit) => {
                if let Err(err) = self.change_pool_limit(limit) {
                    println!("{}", err);
                }
            }
        }
    }

    pub fn register_pools(&mut self, tokens: &[String]) {
        for token1 in tokens {
            for token2 in tokens {
                if matches!(token1.cmp(token2), Ordering::Less) {
                    self.pairs.insert(
                        (token1.to_string(), token2.to_string()),
                        self.helper
                            .create_pool_with_tokens(&mut self.router, token1, token2)
                            .unwrap(),
                    );
                }
            }
        }
    }

    pub fn simulate_case(
        &mut self,
        tokens: &[String],
        ve_events_tuples: &[(usize, String, VeEvent)],
        events_tuples: &[(usize, String, Event)],
    ) {
        self.register_pools(tokens);
        let pools = self
            .pairs
            .values()
            .map(|pool_addr| pool_addr.to_string())
            .collect_vec();

        let mut events: Vec<Vec<(String, Event)>> = vec![vec![]; MAX_PERIOD + 1];
        let mut ve_events: Vec<Vec<(String, VeEvent)>> = vec![vec![]; MAX_PERIOD + 1];

        for (period, user, event) in events_tuples.iter().cloned() {
            events[period].push((user, event));
        }
        for (period, user, event) in ve_events_tuples.iter().cloned() {
            ve_events[period].push((user, event))
        }

        for period in 0..events.len() {
            // vxASTRO events
            if let Some(period_events) = ve_events.get(period) {
                for (user, event) in period_events {
                    self.escrow_events_router(user, event.clone())
                }
            }
            // Generator controller events
            if let Some(period_events) = events.get(period) {
                if !period_events.is_empty() {
                    println!("Period {}:", period);
                }
                for (user, event) in period_events {
                    self.event_router(user, event.clone())
                }
            }

            let mut voted_pools: HashMap<String, f32> = HashMap::new();

            // Checking calculations
            for user in self.user_votes.keys() {
                let votes = self.user_votes.get(user).unwrap();
                if let Some((slope, start, vp)) = self.locks.get(user) {
                    let user_vp = calc_voting_power(
                        *slope,
                        Uint128::from((*vp * MULTIPLIER as f32) as u128),
                        *start,
                        period as u64,
                    );
                    let user_vp = user_vp.u128() as f32 / MULTIPLIER as f32;
                    votes.iter().for_each(|(pool, &bps)| {
                        let vp = voted_pools.entry(pool.clone()).or_default();
                        *vp += (bps as f32 / BasicPoints::MAX as f32) * user_vp
                    })
                }
            }
            let block_period = self.router.block_period();
            for pool_addr in &pools {
                let pool_vp = self
                    .helper
                    .query_voted_pool_info_at_period(&mut self.router, pool_addr, block_period + 1)
                    .unwrap()
                    .vxastro_amount
                    .u128() as f32
                    / MULTIPLIER as f32;
                let real_vp = voted_pools.get(pool_addr).cloned().unwrap_or(0f32);
                if (pool_vp - real_vp).abs() >= 10e-3 {
                    assert_eq!(pool_vp, real_vp, "Period: {}, pool: {}", period, pool_addr)
                }
            }
            self.router.next_block(WEEK);
        }
    }
}

const MAX_PERIOD: usize = 20;
const MAX_USERS: usize = 10;
const MAX_POOLS: usize = 5;
const MAX_EVENTS: usize = 100;

fn escrow_events_strategy() -> impl Strategy<Value = VeEvent> {
    prop_oneof![
        Just(VeEvent::Withdraw),
        (1f64..=100f64).prop_map(VeEvent::ExtendLock),
        (WEEK..MAX_LOCK_TIME).prop_map(VeEvent::IncreaseTime),
        ((1f64..=100f64), WEEK..MAX_LOCK_TIME).prop_map(|(a, b)| VeEvent::CreateLock(a, b)),
    ]
}

fn vote_strategy(tokens: Vec<String>) -> impl Strategy<Value = Event> {
    prop::collection::vec(
        (prop::sample::subsequence(tokens, 2), 1..=2500u16),
        1..MAX_POOLS,
    )
    .prop_filter_map(
        "Accepting only BPS sum <= 10000",
        |vec: Vec<(Vec<String>, u16)>| {
            let votes = vec
                .iter()
                .into_grouping_map_by(|(pair, _)| {
                    let mut pair = pair.clone();
                    pair.sort();
                    (pair[0].clone(), pair[1].clone())
                })
                .aggregate(|acc, _, (_, val)| Some(acc.unwrap_or(0) + *val))
                .into_iter()
                .collect_vec();
            if votes.iter().map(|(_, bps)| bps).sum::<u16>() <= 10000 {
                Some(Event::Vote(votes))
            } else {
                None
            }
        },
    )
}

fn controller_events_strategy(tokens: Vec<String>) -> impl Strategy<Value = Event> {
    prop_oneof![
        Just(Event::GaugePools),
        (2..=MAX_POOLS as u64).prop_map(Event::ChangePoolLimit),
        vote_strategy(tokens)
    ]
}

fn generate_cases() -> impl Strategy<
    Value = (
        Vec<String>,
        Vec<String>,
        Vec<(usize, String, VeEvent)>,
        Vec<(usize, String, Event)>,
    ),
> {
    let tokens_strategy =
        prop::collection::hash_set("[A-Z]{3}", MAX_POOLS * MAX_POOLS / 2 - MAX_POOLS);
    let users_strategy = prop::collection::vec("[a-z]{10}", 1..MAX_USERS);
    (users_strategy, tokens_strategy).prop_flat_map(|(users, tokens)| {
        (
            Just(users.clone()),
            Just(tokens.iter().cloned().collect()),
            prop::collection::vec(
                (
                    1..=MAX_PERIOD,
                    prop::sample::select(users.clone()),
                    escrow_events_strategy(),
                ),
                0..MAX_EVENTS,
            ),
            prop::collection::vec(
                (
                    1..=MAX_PERIOD,
                    prop::sample::select(users),
                    controller_events_strategy(tokens.iter().cloned().collect_vec()),
                ),
                0..MAX_EVENTS,
            ),
        )
    })
}

proptest! {
    #[test]
    fn run_simulations(
        case in generate_cases()
    ) {
        let (users, tokens, ve_events_tuples, events_tuples) = case;
        let mut simulator = Simulator::init(&users);
        simulator.simulate_case(&tokens, &ve_events_tuples[..], &events_tuples[..]);
    }
}

#[test]
fn exact_simulation() {
    let case = (
        ["rsgnawburh", "kxhuagnkvo"],
        ["FOO", "BAR"],
        [
            (4, "rsgnawburh", CreateLock(100.0, 1809600)),
            (5, "rsgnawburh", IncreaseTime(604800)),
            (6, "kxhuagnkvo", CreateLock(100.0, 604800)),
        ],
        [
            (
                4,
                "rsgnawburh",
                Vote(vec![(("BAR".to_string(), "FOO".to_string()), 10000)]),
            ),
            (
                6,
                "kxhuagnkvo",
                Vote(vec![(("BAR".to_string(), "FOO".to_string()), 10000)]),
            ),
            (
                6,
                "rsgnawburh",
                Vote(vec![(("BAR".to_string(), "FOO".to_string()), 10000)]),
            ),
        ],
    );

    let (users, tokens, ve_events_tuples, events_tuples) = case;
    let tokens = tokens.iter().map(|item| item.to_string()).collect_vec();
    let ve_events_tuples = ve_events_tuples
        .iter()
        .map(|(period, user, event)| (*period, user.to_string(), event.clone()))
        .collect_vec();
    let events_tuples = events_tuples
        .iter()
        .map(|(period, user, event)| (*period, user.to_string(), event.clone()))
        .collect_vec();

    let mut simulator = Simulator::init(&users);
    simulator.simulate_case(&tokens, &ve_events_tuples[..], &events_tuples[..]);
}
