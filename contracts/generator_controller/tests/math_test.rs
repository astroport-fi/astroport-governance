use std::collections::HashMap;

use anyhow::Result;
use astroport_governance::generator_controller::ExecuteMsg;
use cosmwasm_std::Addr;
use terra_multi_test::{AppResponse, Executor, TerraApp};

use astroport_governance::utils::WEEK;
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
enum Event<'a> {
    Vote { votes: Vec<(&'a str, u16)> },
    GaugePools,
    ChangePoolLimit { limit: u64 },
}

#[derive(Clone, Debug)]
enum VeEvent {
    CreateLock(f64, u64),
    IncreaseTime(u64),
    ExtendLock(f64),
    Withdraw,
}

struct Simulator<'a> {
    user_votes: HashMap<&'a str, HashMap<&'a str, u16>>,
    helper: ControllerHelper,
    router: TerraApp,
    owner: Addr,
    limit: u64,
}

impl<'a> Simulator<'a> {
    pub fn init(users: &[&'a str]) -> Self {
        let mut router = mock_app();
        let owner = Addr::unchecked("owner");
        Self {
            helper: ControllerHelper::init(&mut router, &owner),
            user_votes: users
                .into_iter()
                .map(|&user| (user, HashMap::new()))
                .collect(),
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
                    .extend_lock_amount(&mut self.router, user, amount as f32)
            }
            Withdraw => self.helper.escrow_helper.withdraw(&mut self.router, user),
        };
    }

    fn vote(&mut self, user: &str, votes: Vec<(&'a str, u16)>) -> Result<AppResponse> {
        self.helper
            .vote(&mut self.router, user, votes.clone())
            .map(|response| {
                for (pool, bps) in votes {
                    self.user_votes
                        .get_mut(user)
                        .expect("User not found!")
                        .insert(pool, bps);
                }
                let user_info = self.helper.query_user_info(&mut self.router, user).unwrap();
                let total_apoints: u16 = user_info
                    .votes
                    .iter()
                    .cloned()
                    .map(|pair| u16::from(pair.1))
                    .sum();
                assert_eq!(total_apoints, 10000);
                assert_eq!(user_info.vote_ts, self.router.block_info().time.seconds());
                response
            })
    }

    fn change_pool_limit(&mut self, limit: u64) -> Result<AppResponse> {
        self.router
            .execute_contract(
                self.owner.clone(),
                self.helper.controller.clone(),
                &ExecuteMsg::ChangePoolLimit { limit },
                &[],
            )
            .map(|response| {
                self.limit = limit;
                response
            })
    }

    pub fn event_router(&mut self, user: &str, event: Event<'a>) {
        match event {
            Vote { votes } => {
                if let Err(err) = self.vote(user, votes) {
                    println!("{}", err.to_string());
                }
            }
            GaugePools => {
                if let Err(err) = self.helper.gauge(&mut self.router, self.owner.as_str()) {
                    println!("{}", err.to_string());
                }
            }
            ChangePoolLimit { limit } => {
                if let Err(err) = self.change_pool_limit(limit) {
                    println!("{}", err.to_string());
                }
            }
        }
    }
}

const MAX_PERIOD: usize = 10;
const MAX_USERS: usize = 6;
const MAX_EVENTS: usize = 100;

#[test]
fn exact_simulation() {
    let escrow_case = [(0, "bpcy", CreateLock(100.0, 3024000))];
    let case = (
        ["bpcy"],
        ["pool1", "pool2", "pool3"],
        [(
            0,
            "bpcy",
            Vote {
                votes: vec![("pool1", 1000u16), ("pool2", 3000u16), ("pool3", 6000u16)],
            },
        )],
    );

    let mut events: Vec<Vec<(&str, Event)>> = vec![vec![]; MAX_PERIOD + 1];
    let mut ve_events: Vec<Vec<(&str, VeEvent)>> = vec![vec![]; MAX_PERIOD + 1];
    let (users, pools, events_tuples) = case;
    for (period, user, event) in events_tuples {
        events[period].push((user, event));
    }
    for (period, user, event) in escrow_case {
        ve_events[period].push((user, event))
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

        let mut voted_pools: HashMap<&str, f32> = HashMap::new();

        // Checking calculations
        for user in users {
            let votes = simulator.user_votes.get(user).unwrap();
            let user_vp = simulator
                .helper
                .escrow_helper
                .query_user_vp(&mut simulator.router, user)
                .unwrap();
            votes.iter().for_each(|(pool, &bps)| {
                let vp = voted_pools.entry(pool).or_default();
                *vp += (bps as f32 / BasicPoints::MAX as f32) * user_vp
            })
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
            let real_vp = voted_pools.get(pool_addr).unwrap().to_owned();
            if (pool_vp - real_vp).abs() >= 10e-3 {
                assert_eq!(pool_vp, real_vp, "Period: {}, pool: {}", period, pool_addr)
            }
        }
        simulator.router.next_block(WEEK);
    }
}
