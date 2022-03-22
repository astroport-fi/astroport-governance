use std::collections::HashMap;

use anyhow::Result;
use cosmwasm_std::{Addr, Decimal, Uint128};
use itertools::Itertools;
use proptest::prelude::*;
use terra_multi_test::{AppResponse, Executor, TerraApp};

use astroport_governance::generator_controller::ExecuteMsg;
use astroport_governance::utils::{calc_voting_power, MAX_LOCK_TIME, WEEK};
use generator_controller::bps::BasicPoints;
use Event::*;
use VeEvent::*;

use crate::test_utils::controller_helper::ControllerHelper;
use crate::test_utils::escrow_helper::MULTIPLIER;
use crate::test_utils::mock_app;
use crate::test_utils::TerraAppExtension;

#[cfg(test)]
mod test_utils;

#[derive(Clone, Debug)]
enum Event {
    Vote(Vec<(String, u16)>),
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
    locks: HashMap<String, (Decimal, u64, f32)>,
    helper: ControllerHelper,
    router: TerraApp,
    owner: Addr,
    limit: u64,
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

    fn vote(&mut self, user: &str, votes: Vec<(String, u16)>) -> Result<AppResponse> {
        let votes: Vec<_> = votes
            .iter()
            .map(|(pool, bps)| (pool.as_str(), *bps))
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
                if let Err(err) = self.helper.gauge(&mut self.router, self.owner.as_str()) {
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

fn vote_strategy(pools: Vec<String>) -> impl Strategy<Value = Event> {
    prop::collection::vec((prop::sample::select(pools), 1..=2500u16), 1..MAX_POOLS).prop_filter_map(
        "Accepting only BPS sum <= 10000",
        |vec: Vec<(String, u16)>| {
            let votes = vec
                .iter()
                .into_grouping_map_by(|(pool, _)| pool.clone())
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

fn controller_events_strategy(pools: Vec<String>) -> impl Strategy<Value = Event> {
    prop_oneof![
        Just(Event::GaugePools),
        // (1..=10u64).prop_map(Event::ChangePoolLimit),
        vote_strategy(pools)
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
    let pools_strategy = prop::collection::vec("[a-z]{4}", 1..MAX_POOLS);
    let users_strategy = prop::collection::vec("[a-z]{10}", 1..MAX_USERS);
    (users_strategy, pools_strategy).prop_flat_map(|(users, pools)| {
        (
            Just(users.clone()),
            Just(pools.clone()),
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
                    controller_events_strategy(pools),
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
        let mut events: Vec<Vec<(String, Event)>> = vec![vec![]; MAX_PERIOD + 1];
        let mut ve_events: Vec<Vec<(String, VeEvent)>> = vec![vec![]; MAX_PERIOD + 1];
        let (users, pools, ve_events_tuples, events_tuples) = case;
        for (period, user, event) in events_tuples {
            events[period].push((user.to_string(), event));
        }
        for (period, user, event) in ve_events_tuples {
            ve_events[period].push((user.to_string(), event))
        }

        let mut simulator = Simulator::init(&users);

        for period in 0..events.len() {
            // vxASTRO events
            if let Some(period_events) = ve_events.get(period) {
                for (user, event) in period_events {
                    simulator.escrow_events_router(user, event.clone())
                }
            }
            // Generator controller events
            if let Some(period_events) = events.get(period) {
                if !period_events.is_empty() {
                    println!("Period {}:", period);
                }
                for (user, event) in period_events {
                    simulator.event_router(user, event.clone())
                }
            }

            let mut voted_pools: HashMap<String, f32> = HashMap::new();

            // Checking calculations
            for user in users.iter() {
                let votes = simulator.user_votes.get(user).unwrap();
                if let Some((slope, start, vp)) = simulator.locks.get(user) {
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
            let block_period = simulator.router.block_period();
            for pool_addr in pools.iter() {
                let pool_vp = simulator
                    .helper
                    .query_voted_pool_info_at_period(&mut simulator.router, pool_addr, block_period + 1)
                    .unwrap()
                    .vxastro_amount
                    .u128() as f32
                    / MULTIPLIER as f32;
                let real_vp = voted_pools.get(pool_addr).cloned().unwrap_or(0f32);
                if (pool_vp - real_vp).abs() >= 10e-3 {
                    assert_eq!(pool_vp, real_vp, "Period: {}, pool: {}", period, pool_addr)
                }
            }
            simulator.router.next_block(WEEK);
        }
    }
}

#[test]
fn exact_simulation() {
    let case = (
        ["egzyhzadde", "rsgnawburh", "kxhuagnkvo"],
        ["xyuq", "krhr"],
        [
            (4, "rsgnawburh", CreateLock(100.0, 1809600)),
            (5, "rsgnawburh", IncreaseTime(604800)),
            (6, "kxhuagnkvo", CreateLock(100.0, 604800)),
        ],
        [
            (4, "rsgnawburh", Vote(vec![("krhr".to_string(), 10000)])),
            (6, "kxhuagnkvo", Vote(vec![("krhr".to_string(), 10000)])),
            (6, "rsgnawburh", Vote(vec![("xyuq".to_string(), 10000)])),
        ],
    );

    let mut events: Vec<Vec<(String, Event)>> = vec![vec![]; MAX_PERIOD + 1];
    let mut ve_events: Vec<Vec<(String, VeEvent)>> = vec![vec![]; MAX_PERIOD + 1];
    let (users, pools, ve_events_tuples, events_tuples) = case;
    for (period, user, event) in events_tuples {
        events[period].push((user.to_string(), event));
    }
    for (period, user, event) in ve_events_tuples {
        ve_events[period].push((user.to_string(), event))
    }

    let mut simulator = Simulator::init(&users);

    for period in 0..events.len() {
        // vxASTRO events
        if let Some(period_events) = ve_events.get(period) {
            for (user, event) in period_events {
                simulator.escrow_events_router(user, event.clone())
            }
        }
        // Generator controller events
        if let Some(period_events) = events.get(period) {
            if !period_events.is_empty() {
                println!("Period {}:", period);
            }
            for (user, event) in period_events {
                simulator.event_router(user, event.clone())
            }
        }

        let mut voted_pools: HashMap<String, f32> = HashMap::new();

        // Checking calculations
        for user in users {
            let votes = simulator.user_votes.get(user).unwrap();
            if let Some((slope, start, vp)) = simulator.locks.get(user) {
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
        let block_period = simulator.router.block_period();
        for pool_addr in pools {
            let pool_vp = simulator
                .helper
                .query_voted_pool_info_at_period(&mut simulator.router, pool_addr, block_period + 1)
                .unwrap()
                .vxastro_amount
                .u128() as f32
                / MULTIPLIER as f32;
            let real_vp = voted_pools.get(pool_addr).cloned().unwrap_or(0f32);
            if (pool_vp - real_vp).abs() >= 10e-3 {
                assert_eq!(pool_vp, real_vp, "Period: {}, pool: {}", period, pool_addr)
            }
        }
        simulator.router.next_block(WEEK);
    }
}
