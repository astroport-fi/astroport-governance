use crate::test_utils::{mock_app, Helper};
use anyhow::Result;
use astroport_voting_escrow::contract::{MAX_LOCK_TIME, WEEK};
use cosmwasm_std::{Addr, Timestamp};
use proptest::collection::VecStrategy;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use terra_multi_test::{next_block, AppResponse, TerraApp};

mod test_utils;

#[derive(Clone, Default)]
struct Point {
    amount: f32,
    end: u64,
    verified: bool,
}

#[derive(Clone, Debug)]
enum LockEvent {
    CreateLock(u128, u64),
    IncreaseTime(u64),
    ExtendLock(u128),
    Withdraw,
    NoOp,
}

use LockEvent::*;

struct Simulator {
    // points history (history[period][user] = point)
    points: Vec<HashMap<String, Point>>,
    users: Vec<String>,
    helper: Helper,
    router: TerraApp,
}

fn get_period(time: u64) -> u64 {
    time / WEEK
}

fn apply_boost(amount: u128, interval: u64) -> f32 {
    (amount as f32 * 2.5 * interval as f32) / get_period(MAX_LOCK_TIME) as f32
}

impl Simulator {
    fn new<T: Clone + Into<String>>(users: &[T]) -> Self {
        let mut router = mock_app();
        Self {
            points: vec![HashMap::new(); 10000],
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

    fn create_lock(&mut self, user: &str, amount: u128, interval: u64) -> Result<AppResponse> {
        let block_period = self.block_period();
        let periods_interval = get_period(interval);
        self.helper
            .create_lock(&mut self.router, user, interval, amount as u64)
            .and_then(|response| {
                self.add_point(
                    block_period as usize,
                    user,
                    apply_boost(amount, periods_interval),
                    block_period + periods_interval,
                    true,
                );
                Ok(response)
            })
    }

    fn increase_time(&mut self, user: &str, interval: u64) -> Result<AppResponse> {
        let cur_period = self.block_period() as usize;
        let user_balance = self.calc_user_balance_at(cur_period, user);
        let prev_point = self
            .get_prev_point(user)
            .expect("We always need previous point!");
        self.helper
            .extend_lock_time(&mut self.router, user, interval)
            .and_then(|response| {
                self.add_point(
                    cur_period,
                    user,
                    user_balance,
                    prev_point.end + get_period(interval),
                    true,
                );
                Ok(response)
            })
    }

    fn extend_lock(&mut self, user: &str, amount: u128) -> Result<AppResponse> {
        let cur_period = self.block_period() as usize;
        let user_balance = self.calc_user_balance_at(cur_period, user);
        let prev_point = self
            .get_prev_point(user)
            .expect("We always need previous point!");
        let vp = apply_boost(amount, prev_point.end - cur_period as u64);
        self.helper
            .extend_lock_amount(&mut self.router, user, amount as u64)
            .and_then(|response| {
                self.add_point(cur_period, user, user_balance + vp, prev_point.end, true);
                Ok(response)
            })
    }

    fn withdraw(&mut self, user: &str) -> Result<AppResponse> {
        let cur_period = self.block_period();
        self.helper
            .withdraw(&mut self.router, user)
            .and_then(|response| {
                self.add_point(cur_period as usize, user, 0.0, cur_period, true);
                Ok(response)
            })
    }

    fn event_router(&mut self, user: &str, event: LockEvent) {
        match event {
            LockEvent::CreateLock(amount, interval) => {
                match self.create_lock(user, amount, interval) {
                    Err(err) => {
                        dbg!(err);
                        ()
                    }
                    _ => (),
                }
            }
            LockEvent::IncreaseTime(interval) => match self.increase_time(user, interval) {
                Err(err) => {
                    dbg!(err);
                    ()
                }
                _ => (),
            },
            LockEvent::ExtendLock(amount) => match self.extend_lock(user, amount) {
                Err(err) => {
                    dbg!(err);
                    ()
                }
                _ => (),
            },
            LockEvent::Withdraw => match self.withdraw(user) {
                Err(err) => {
                    dbg!(err);
                    ()
                }
                _ => (),
            },
            LockEvent::NoOp => self.checkpoint_user(user),
        }
        let real_balance = self.calc_user_balance_at(self.block_period() as usize, user);
        let contract_balance = self
            .helper
            .query_user_vp(&mut self.router, user)
            .unwrap_or(0.0);
        assert!((real_balance - contract_balance).abs() < 10e-5);
    }

    fn add_point<T: Into<String>>(
        &mut self,
        period: usize,
        user: T,
        amount: f32,
        end: u64,
        verified: bool,
    ) {
        let map = &mut self.points[period];
        map.extend(vec![(
            user.into(),
            Point {
                amount,
                end,
                verified,
            },
        )]);
    }

    fn get_prev_point(&mut self, user: &str) -> Option<Point> {
        let cur_period = (self.block_period() - 1) as usize;
        self.get_user_lock_at(cur_period, user)
    }

    fn checkpoint_user(&mut self, user: &str) {
        let cur_period = self.block_period() as usize;
        let user_balance = self.calc_user_balance_at(cur_period, user);
        let prev_point = self
            .get_prev_point(user)
            .expect("We always need previous point!");
        self.add_point(cur_period, user, user_balance, prev_point.end, true);
    }

    fn get_user_lock_at<T: Into<String>>(&mut self, period: usize, user: T) -> Option<Point> {
        let points_map = &mut self.points[period];
        match points_map.entry(user.into()) {
            Entry::Occupied(value) => Some(value.get().clone()),
            Entry::Vacant(_) => None,
        }
    }

    fn calc_user_balance_at(&mut self, period: usize, user: &str) -> f32 {
        match self.get_user_lock_at(period, user) {
            Some(Point {
                amount, verified, ..
            }) if verified => amount,
            _ => {
                let prev_point = self.get_user_lock_at(period - 1, user).unwrap_or_default();
                let dt = prev_point.end - period as u64 - 1;
                if dt == 0 {
                    0.0
                } else {
                    let new_vp = prev_point.amount - prev_point.amount / dt as f32;
                    if new_vp < 0.0 {
                        0.0
                    } else {
                        new_vp
                    }
                }
            }
        }
    }

    fn calc_total_balance_at(&mut self, period: usize) -> f32 {
        self.users.clone().iter().fold(0.0, |acc, user| {
            acc + self.calc_user_balance_at(period, &user)
        })
    }
}

use proptest::prelude::*;

fn events_strategy() -> impl Strategy<Value = LockEvent> {
    prop_oneof![
        Just(LockEvent::NoOp),
        Just(LockEvent::Withdraw),
        (0..100_u128).prop_map(LockEvent::ExtendLock),
        (0..MAX_LOCK_TIME).prop_map(LockEvent::IncreaseTime),
        (0..100_u128, 0..MAX_LOCK_TIME).prop_map(|(a, b)| LockEvent::CreateLock(a, b)),
    ]
}

fn generate_cases() -> impl Strategy<Value = (Vec<String>, Vec<(usize, String, LockEvent)>)> {
    let users_strategy = prop::collection::vec("[a-z]{4,32}", 3..10);
    users_strategy.prop_flat_map(|users| {
        (
            Just(users.clone()),
            prop::collection::vec(
                (0..105_usize, prop::sample::select(users), events_strategy()),
                300,
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
        let mut events: Vec<Vec<(String, LockEvent)>> = vec![vec![]; 105];
        let (users, events_tuples) = case;
        for (period, user, event) in events_tuples {
            events[period].push((user, event));
        };

        let mut simulator = Simulator::new(&users);
        simulator.router.update_block(|block| block.time = Timestamp::from_seconds(0));
        for user in users {
            simulator.mint(&user, 10000);
            simulator.event_router(&user, CreateLock(100, WEEK));
        }

        for period in 0..105_usize {
            if let Some(period_events) = events.get(period) {
                for (user, event) in period_events {
                    dbg!(period, user, event);
                }
            }
        }
    }
}
