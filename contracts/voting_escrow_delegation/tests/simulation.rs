use anyhow::Result;
use astroport_governance::utils::WEEK;

use crate::test_helper::{mock_app, Helper};
use cosmwasm_std::{Addr, Uint128};
use cw_multi_test::{next_block, App, AppResponse};

mod test_helper;

#[derive(Clone, Debug)]
enum Event {
    CreateDelegation(Uint128, u64, String, String),
    ExtendDelegation(Uint128, u64, String),
}

use Event::*;

struct Simulator {
    helper: Helper,
    router: App,
}

impl Simulator {
    fn new() -> Self {
        let mut router = mock_app();
        Self {
            helper: Helper::init(&mut router, Addr::unchecked("owner")),
            router,
        }
    }

    fn mint(&mut self, user: &str, amount: u128) {
        self.helper
            .escrow_helper
            .mint_xastro(&mut self.router, user, amount as u64)
    }

    fn create_lock(&mut self, user: &str, amount: f64, interval: u64) -> Result<AppResponse> {
        self.helper
            .escrow_helper
            .create_lock(&mut self.router, user, interval, amount as f32)
    }

    fn app_next_period(&mut self) {
        self.router.update_block(next_block);
        self.router
            .update_block(|block| block.time = block.time.plus_seconds(WEEK));
    }

    fn create_delegation(
        &mut self,
        user: &str,
        percentage: Uint128,
        expire_time: u64,
        token_id: String,
        recipient: String,
    ) -> Result<AppResponse> {
        self.helper.create_delegation(
            &mut self.router,
            user,
            percentage,
            expire_time,
            token_id,
            recipient,
        )
    }

    fn extend_delegation(
        &mut self,
        user: &str,
        percentage: Uint128,
        expire_time: u64,
        token_id: String,
    ) -> Result<AppResponse> {
        self.helper
            .extend_delegation(&mut self.router, user, percentage, expire_time, token_id)
    }

    fn event_router(&mut self, user: &str, event: Event) {
        println!("User {} Event {:?}", user, event);
        match event {
            Event::CreateDelegation(percentage, expire_time, token_id, recipient) => {
                if let Err(err) =
                    self.create_delegation(user, percentage, expire_time, token_id, recipient)
                {
                    dbg!(err);
                }
            }
            Event::ExtendDelegation(percentage, expire_time, token_id) => {
                if let Err(err) = self.extend_delegation(user, percentage, expire_time, token_id) {
                    dbg!(err);
                }
            }
        }
    }
}

use proptest::prelude::*;

const MAX_PERIOD: usize = 10;
const MAX_USERS: usize = 6;
const MAX_TOKENS: usize = 10;
const MAX_EVENTS: usize = 100;

fn events_strategy() -> impl Strategy<Value = Event> {
    prop_oneof![
        (
            1u64..=100u64,
            1..MAX_PERIOD,
            prop::collection::vec("[a-z]{4,32}", 1..MAX_USERS),
            prop::collection::vec("[t-z]{6,32}", 1..MAX_TOKENS)
        )
            .prop_map(|(a, b, c, d)| {
                Event::CreateDelegation(
                    Uint128::from(a),
                    WEEK * b as u64,
                    c.iter().next().unwrap().to_string(),
                    d.iter().next().unwrap().to_string(),
                )
            }),
        (
            1u64..=100u64,
            1..MAX_PERIOD,
            prop::collection::vec("[t-z]{2,32}", 1..MAX_TOKENS)
        )
            .prop_map(|(a, b, c)| {
                Event::ExtendDelegation(
                    Uint128::from(a),
                    WEEK * b as u64,
                    c.iter().next().unwrap().to_string(),
                )
            }),
    ]
}

fn generate_cases() -> impl Strategy<Value = (Vec<String>, Vec<(usize, String, String, Event)>)> {
    let users_strategy = prop::collection::vec("[a-z]{4,32}", 1..MAX_USERS);

    users_strategy.prop_flat_map(|users| {
        (
            Just(users.clone()),
            prop::collection::vec(
                (
                    1..=MAX_PERIOD,
                    prop::sample::select(users.clone()),
                    prop::sample::select(users.clone()),
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
        let mut events: Vec<Vec<(String, String, Event)>> = vec![vec![]; MAX_PERIOD + 1];
        let (users, events_tuples) = case;
        for (period, user, recipient, event) in events_tuples {
            events[period].push((user.to_string(), recipient.to_string(), event));
        }

        let mut simulator = Simulator::new();
        for user in users {
            simulator.mint(user.as_str(), 10000);
            simulator.create_lock(user.as_str(), 500 as f64, WEEK * 11).unwrap();
        }

        for period in 1..=MAX_PERIOD {
            if let Some(period_events) = events.get(period) {
                if !period_events.is_empty() {
                    println!("Period {}:", period);
                }
                for (user, recipient, event) in period_events {
                    // check user's balance before the delegation
                    let user_balance_before = simulator
                        .helper
                        .adjusted_balance(&mut simulator.router, user, None)
                        .unwrap();

                    // check user's delegated balance before the delegation
                    let user_delegated_balance_before = simulator
                        .helper
                        .delegated_balance(&mut simulator.router, user, None)
                        .unwrap();

                    // check recipient's balance before the delegation
                    let recipient_balance_before = simulator
                        .helper
                        .adjusted_balance(&mut simulator.router, recipient, None)
                        .unwrap();

                    // check recipient's delegated balance before the delegation
                    let recipient_delegated_balance_before = simulator
                        .helper
                        .delegated_balance(&mut simulator.router, recipient, None)
                        .unwrap();

                    // try to execute user's event
                    simulator.event_router(user, event.clone());

                    // check user's balance after the delegation
                    let user_balance_after = simulator
                        .helper
                        .adjusted_balance(&mut simulator.router, user, None)
                        .unwrap();

                    // check user's delegated balance
                    let user_delegated_balance_after = simulator
                        .helper
                        .delegated_balance(&mut simulator.router, user, None)
                        .unwrap();

                    // check recipient's balance after the delegation
                    let recipient_balance_after = simulator
                        .helper
                        .adjusted_balance(&mut simulator.router, recipient, None)
                        .unwrap();

                     // check recipient's delegated balance after the delegation
                    let recipient_delegated_balance_after = simulator
                        .helper
                        .delegated_balance(&mut simulator.router, recipient, None)
                        .unwrap();

                    // check user's balance
                    assert_eq!(
                        user_balance_after,
                        user_balance_before
                            - (user_delegated_balance_after - user_delegated_balance_before)
                    );

                    // check recipient's balance
                    assert_eq!(
                        recipient_balance_after,
                        recipient_balance_before
                            - (recipient_delegated_balance_after - recipient_delegated_balance_before)
                    );
                }
            }

            simulator.app_next_period()
        }
    }
}

#[test]
fn exact_simulation() {
    let case = (
        ["user1", "user2"],
        [
            (
                1,
                "user1",
                "user2",
                CreateDelegation(
                    Uint128::new(100),
                    WEEK * 2,
                    "token_1".to_string(),
                    "user2".to_string(),
                ),
            ),
            (
                1,
                "user2",
                "user1",
                CreateDelegation(
                    Uint128::new(50),
                    WEEK * 2,
                    "token_2".to_string(),
                    "user1".to_string(),
                ),
            ),
            (
                2,
                "user2",
                "user1",
                CreateDelegation(
                    Uint128::new(30),
                    WEEK * 2,
                    "token_3".to_string(),
                    "user1".to_string(),
                ),
            ),
            (
                3,
                "user2",
                "user1",
                ExtendDelegation(Uint128::new(70), WEEK * 5, "token_2".to_string()),
            ),
            (
                4,
                "user1",
                "user2",
                ExtendDelegation(Uint128::new(60), WEEK * 4, "token_1".to_string()),
            ),
            (
                5,
                "user1",
                "user3",
                CreateDelegation(
                    Uint128::new(100),
                    WEEK * 4,
                    "token_4".to_string(),
                    "user3".to_string(),
                ),
            ),
            (
                6,
                "user2",
                "user1",
                CreateDelegation(
                    Uint128::new(100),
                    WEEK * 4,
                    "token_5".to_string(),
                    "user1".to_string(),
                ),
            ),
        ],
    );

    let mut events: Vec<Vec<(String, String, Event)>> = vec![vec![]; MAX_PERIOD + 1];
    let (users, events_tuples) = case;
    for (period, user, recipient, event) in events_tuples {
        events[period].push((user.to_string(), recipient.to_string(), event));
    }

    let mut simulator = Simulator::new();
    for user in users {
        simulator.mint(user, 10000);
        simulator.create_lock(user, 500 as f64, WEEK * 10).unwrap();
    }

    for period in 1..=MAX_PERIOD {
        if let Some(period_events) = events.get(period) {
            if !period_events.is_empty() {
                println!("Period {}:", period);
            }
            for (user, recipient, event) in period_events {
                // check user's balance before the delegation
                let user_balance_before = simulator
                    .helper
                    .adjusted_balance(&mut simulator.router, user, None)
                    .unwrap();

                let user_delegated_balance_before = simulator
                    .helper
                    .delegated_balance(&mut simulator.router, user, None)
                    .unwrap();

                // check recipient's balance before the delegation
                let recipient_balance_before = simulator
                    .helper
                    .adjusted_balance(&mut simulator.router, recipient, None)
                    .unwrap();

                // try to execute user's event
                simulator.event_router(user, event.clone());

                // check user's balance after the delegation
                let user_balance_after = simulator
                    .helper
                    .adjusted_balance(&mut simulator.router, user, None)
                    .unwrap();

                // check user's delegated balance
                let user_delegated_balance_after = simulator
                    .helper
                    .delegated_balance(&mut simulator.router, user, None)
                    .unwrap();

                // check recipient's balance after the delegation
                let recipient_balance_after = simulator
                    .helper
                    .adjusted_balance(&mut simulator.router, recipient, None)
                    .unwrap();

                // check user's balance
                assert_eq!(
                    user_balance_after,
                    user_balance_before
                        - (user_delegated_balance_after - user_delegated_balance_before)
                );

                // check recipient's balance
                assert_eq!(
                    recipient_balance_after,
                    recipient_balance_before
                        + (user_delegated_balance_after - user_delegated_balance_before)
                );
            }
        }

        simulator.app_next_period()
    }
}
