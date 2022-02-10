use crate::test_utils::{mock_app, Helper, MULTIPLIER};
use anyhow::Result;
use cosmwasm_std::{Addr, Timestamp};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use terra_multi_test::{next_block, AppResponse, TerraApp};
use voting_escrow::contract::{MAX_LOCK_TIME, WEEK};

mod test_utils;

#[derive(Clone, Default, Debug)]
struct Point {
    amount: f64,
    end: u64,
}

#[derive(Clone, Debug)]
enum Event {
    CreateLock(f64, u64),
    IncreaseTime(u64),
    ExtendLock(f64),
    Withdraw,
    Blacklist,
}

use Event::*;

struct Simulator {
    // points history (history[period][user] = point)
    points: Vec<HashMap<String, Point>>,
    // current locked amount per user
    locked: HashMap<String, f64>,
    users: Vec<String>,
    helper: Helper,
    router: TerraApp,
}

fn get_period(time: u64) -> u64 {
    time / WEEK
}

fn apply_coefficient(amount: f64, interval: u64) -> f64 {
    let coeff = 1f64 + (1.5 * interval as f64) / get_period(MAX_LOCK_TIME) as f64;
    // imitating Decimal fraction multiplication in the contract
    (amount * coeff * MULTIPLIER as f64).trunc() / MULTIPLIER as f64
}

impl Simulator {
    fn new<T: Clone + Into<String>>(users: &[T]) -> Self {
        let mut router = mock_app();
        Self {
            points: vec![HashMap::new(); 10000],
            locked: Default::default(),
            users: users.iter().cloned().map(|user| user.into()).collect(),
            helper: Helper::init(&mut router, Addr::unchecked("owner")),
            router,
        }
    }

    fn mint(&mut self, user: &str, amount: u128) {
        self.helper
            .mint_xastro(&mut self.router, user, amount as u64)
    }

    fn block_period(&self) -> u64 {
        get_period(self.router.block_info().time.seconds())
    }

    fn app_next_period(&mut self) {
        self.router.update_block(next_block);
        self.router
            .update_block(|block| block.time = block.time.plus_seconds(WEEK));
    }

    fn create_lock(&mut self, user: &str, amount: f64, interval: u64) -> Result<AppResponse> {
        let block_period = self.block_period();
        let periods_interval = get_period(interval);
        self.helper
            .create_lock(&mut self.router, user, interval, amount as f32)
            .map(|response| {
                self.add_point(
                    block_period as usize,
                    user,
                    apply_coefficient(amount, periods_interval),
                    block_period + periods_interval,
                );
                self.locked.extend(vec![(user.to_string(), amount)]);
                response
            })
    }

    fn increase_time(&mut self, user: &str, interval: u64) -> Result<AppResponse> {
        self.helper
            .extend_lock_time(&mut self.router, user, interval)
            .map(|response| {
                let cur_period = self.block_period() as usize;
                let periods_interval = get_period(interval);
                let end = if let Some(point) = self.get_user_point_at(cur_period, user) {
                    point.end
                } else {
                    let prev_point = self
                        .get_prev_point(user)
                        .expect("We always need previous point!");
                    prev_point.end
                };
                let dt = end + periods_interval - cur_period as u64;
                let amount = self.locked.get(user).unwrap().to_owned();
                self.add_point(
                    cur_period,
                    user,
                    apply_coefficient(amount, dt),
                    end + periods_interval,
                );
                response
            })
    }

    fn extend_lock(&mut self, user: &str, amount: f64) -> Result<AppResponse> {
        self.helper
            .extend_lock_amount(&mut self.router, user, amount as f32)
            .map(|response| {
                let cur_period = self.block_period() as usize;
                let (user_balance, end) =
                    if let Some(point) = self.get_user_point_at(cur_period, user) {
                        (point.amount, point.end)
                    } else {
                        let prev_point = self
                            .get_prev_point(user)
                            .expect("We always need previous point!");
                        (self.calc_user_balance_at(cur_period, user), prev_point.end)
                    };
                let vp = apply_coefficient(amount, end - cur_period as u64);
                self.add_point(cur_period, user, user_balance + vp, end);
                let lock = self.locked.get_mut(user).unwrap();
                *lock += amount;
                response
            })
    }

    fn withdraw(&mut self, user: &str) -> Result<AppResponse> {
        self.helper
            .withdraw(&mut self.router, user)
            .map(|response| {
                let cur_period = self.block_period();
                self.add_point(cur_period as usize, user, 0.0, cur_period);
                self.locked.remove(user);
                response
            })
    }

    fn append2blacklist(&mut self, user: &str) -> Result<AppResponse> {
        self.helper
            .blacklist(&mut self.router, user)
            .map(|response| {
                let cur_period = self.block_period();
                self.add_point(cur_period as usize, user, 0.0, cur_period);
                response
            })
    }

    fn event_router(&mut self, user: &str, event: Event) {
        println!("User {} Event {:?}", user, event);
        match event {
            Event::CreateLock(amount, interval) => {
                if let Err(err) = self.create_lock(user, amount, interval) {
                    dbg!(err);
                }
            }
            Event::IncreaseTime(interval) => {
                if let Err(err) = self.increase_time(user, interval) {
                    dbg!(err);
                }
            }
            Event::ExtendLock(amount) => {
                if let Err(err) = self.extend_lock(user, amount) {
                    dbg!(err);
                }
            }
            Event::Withdraw => {
                if let Err(err) = self.withdraw(user) {
                    dbg!(err);
                }
            }
            Event::Blacklist => {
                if let Err(err) = self.append2blacklist(user) {
                    dbg!(err);
                }
            }
        }
        let real_balance = self
            .get_user_point_at(self.block_period() as usize, user)
            .map(|point| point.amount)
            .unwrap_or_else(|| self.calc_user_balance_at(self.block_period() as usize, user));
        let contract_balance = self
            .helper
            .query_user_vp(&mut self.router, user)
            .unwrap_or(0.0) as f64;
        if (real_balance - contract_balance).abs() >= 10e-3 {
            assert_eq!(real_balance, contract_balance)
        };
    }

    fn checkpoint_all_users(&mut self) {
        let cur_period = self.block_period() as usize;
        self.users.clone().iter().for_each(|user| {
            // we need to calc point only if it was not calculated yet
            if self.get_user_point_at(cur_period, user).is_none() {
                self.checkpoint_user(user)
            }
        })
    }

    fn add_point<T: Into<String>>(&mut self, period: usize, user: T, amount: f64, end: u64) {
        let map = &mut self.points[period];
        map.extend(vec![(user.into(), Point { amount, end })]);
    }

    fn get_prev_point(&mut self, user: &str) -> Option<Point> {
        let prev_period = (self.block_period() - 1) as usize;
        self.get_user_point_at(prev_period, user)
    }

    fn checkpoint_user(&mut self, user: &str) {
        let cur_period = self.block_period() as usize;
        let user_balance = self.calc_user_balance_at(cur_period, user);
        let prev_point = self
            .get_prev_point(user)
            .expect("We always need previous point!");
        self.add_point(cur_period, user, user_balance, prev_point.end);
    }

    fn get_user_point_at<T: Into<String>>(&mut self, period: usize, user: T) -> Option<Point> {
        let points_map = &mut self.points[period];
        match points_map.entry(user.into()) {
            Entry::Occupied(value) => Some(value.get().clone()),
            Entry::Vacant(_) => None,
        }
    }

    fn calc_user_balance_at(&mut self, period: usize, user: &str) -> f64 {
        match self.get_user_point_at(period, user) {
            Some(point) => point.amount,
            None => {
                let prev_point = self
                    .get_user_point_at(period - 1, user)
                    .expect("We always need previous point!");
                let dt = prev_point.end.saturating_sub(period as u64 - 1);
                if dt == 0 {
                    0.0
                } else {
                    prev_point.amount - prev_point.amount / dt as f64
                }
            }
        }
    }

    fn calc_total_balance_at(&mut self, period: usize) -> f64 {
        self.users.clone().iter().fold(0.0, |acc, user| {
            acc + self.get_user_point_at(period, user).unwrap().amount
        })
    }
}

use proptest::prelude::*;

const MAX_PERIOD: usize = 10;
const MAX_USERS: usize = 6;
const MAX_EVENTS: usize = 100;

fn amount_strategy() -> impl Strategy<Value = f64> {
    (1f64..=100f64).prop_map(|val| (val * MULTIPLIER as f64).trunc() / MULTIPLIER as f64)
}

fn events_strategy() -> impl Strategy<Value = Event> {
    prop_oneof![
        Just(Event::Withdraw),
        Just(Event::Blacklist),
        amount_strategy().prop_map(Event::ExtendLock),
        (0..MAX_LOCK_TIME).prop_map(Event::IncreaseTime),
        (amount_strategy(), 0..MAX_LOCK_TIME).prop_map(|(a, b)| Event::CreateLock(a, b)),
    ]
}

fn generate_cases() -> impl Strategy<Value = (Vec<String>, Vec<(usize, String, Event)>)> {
    let users_strategy = prop::collection::vec("[a-z]{4,32}", 1..MAX_USERS);
    users_strategy.prop_flat_map(|users| {
        (
            Just(users.clone()),
            prop::collection::vec(
                (
                    1..=MAX_PERIOD,
                    prop::sample::select(users),
                    events_strategy(),
                ),
                0..MAX_EVENTS,
            ),
        )
    })
}

proptest! {
    #[test]
    fn run_simulations
    (
        case in generate_cases()
    ) {
        let mut events: Vec<Vec<(String, Event)>> = vec![vec![]; MAX_PERIOD + 1];
        let (users, events_tuples) = case;
        for (period, user, event) in events_tuples {
            events[period].push((user, event));
        };

        let mut simulator = Simulator::new(&users);
        simulator.router.update_block(|block| block.time = Timestamp::from_seconds(0));
        for user in users {
            simulator.mint(&user, 10000);
            simulator.add_point(0, user, 0.0, 104);
        }
        simulator.app_next_period();

        for period in 1..=MAX_PERIOD {
            if let Some(period_events) = events.get(period) {
                for (user, event) in period_events {
                    simulator.event_router(user, event.clone())
                }
            }
            simulator.checkpoint_all_users();
            let real_balance = simulator.calc_total_balance_at(period);
            let contract_balance = simulator
                .helper
                .query_total_vp(&mut simulator.router)
                .unwrap_or(0.0) as f64;
            if (real_balance - contract_balance).abs() >= 10e-3 {
                assert_eq!(real_balance, contract_balance)
            };
            // evaluate passed periods in history
            for check_period in 1..period {
                let real_balance = simulator.calc_total_balance_at(check_period);
                let contract_balance = simulator
                    .helper
                    .query_total_vp_at(&mut simulator.router, check_period as u64 * WEEK)
                    .unwrap_or(0.0) as f64;
                if (real_balance - contract_balance).abs() >= 10e-3 {
                    assert_eq!(real_balance, contract_balance)
                };
            }
            simulator.app_next_period()
        }
    }
}

#[test]
fn exact_simulation() {
    let case = (
        ["juqgboowsqprzrhahcqb", "eipcy"],
        [
            (3, "juqgboowsqprzrhahcqb", Blacklist),
            (1, "eipcy", CreateLock(65.279624, 7862522)),
            (7, "juqgboowsqprzrhahcqb", Blacklist),
            (2, "juqgboowsqprzrhahcqb", CreateLock(81.395287, 5129908)),
            (5, "eipcy", Blacklist),
        ],
    );

    let mut events: Vec<Vec<(String, Event)>> = vec![vec![]; MAX_PERIOD + 1];
    let (users, events_tuples) = case;
    for (period, user, event) in events_tuples {
        events[period].push((user.to_string(), event));
    }

    let mut simulator = Simulator::new(&users);
    simulator
        .router
        .update_block(|block| block.time = Timestamp::from_seconds(0));
    for user in users {
        simulator.mint(user, 10000);
        simulator.add_point(0, user, 0.0, 104);
    }
    simulator.app_next_period();

    for period in 1..=MAX_PERIOD {
        if let Some(period_events) = events.get(period) {
            if !period_events.is_empty() {
                println!("Period {}:", period);
            }
            for (user, event) in period_events {
                simulator.event_router(user, event.clone())
            }
        }
        simulator.checkpoint_all_users();
        let real_balance = simulator.calc_total_balance_at(period);
        let contract_balance = simulator
            .helper
            .query_total_vp(&mut simulator.router)
            .unwrap_or(0.0) as f64;
        if (real_balance - contract_balance).abs() >= 10e-3 {
            println!("Assert failed at period {}", period);
            assert_eq!(real_balance, contract_balance)
        };
        // evaluate passed periods in history
        for check_period in 1..period {
            let real_balance = simulator.calc_total_balance_at(check_period);
            let contract_balance = simulator
                .helper
                .query_total_vp_at(&mut simulator.router, check_period as u64 * WEEK)
                .unwrap_or(0.0) as f64;
            if (real_balance - contract_balance).abs() >= 10e-3 {
                assert_eq!(real_balance, contract_balance)
            };
        }
        simulator.app_next_period()
    }
}
