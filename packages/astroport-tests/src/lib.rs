pub mod base;
use astroport_governance::utils::{get_period, EPOCH_START};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::Timestamp;
use cw_multi_test::{App, BankKeeper, BasicAppBuilder};

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod controller_helper;
#[allow(clippy::all)]
#[allow(dead_code)]
pub mod escrow_helper;

pub fn mock_app() -> App {
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(EPOCH_START);
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();

    BasicAppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .build(|_, _, _| {})
}

pub trait TerraAppExtension {
    fn next_block(&mut self, time: u64);
    fn block_period(&self) -> u64;
}

impl TerraAppExtension for App {
    fn next_block(&mut self, time: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(time);
            block.height += 1
        });
    }

    fn block_period(&self) -> u64 {
        get_period(self.block_info().time.seconds()).unwrap()
    }
}
