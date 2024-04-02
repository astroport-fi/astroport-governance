#![cfg(not(tarpaulin_include))]

pub mod address_generator;
pub mod base;

use address_generator::WasmAddressGenerator;
use astroport_governance::utils::{get_lite_period, EPOCH_START};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{Empty, Timestamp};
use cw_multi_test::{App, BankKeeper, BasicAppBuilder, FailingModule, WasmKeeper};

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
        .with_wasm::<FailingModule<Empty, Empty, Empty>, WasmKeeper<Empty, Empty>>(
            WasmKeeper::new_with_custom_address_generator(WasmAddressGenerator::default()),
        )
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
        get_lite_period(self.block_info().time.seconds()).unwrap()
    }
}
