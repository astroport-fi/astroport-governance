use std::cell::Cell;

use cosmwasm_std::{Addr, Storage};
use cw_multi_test::AddressGenerator;

/// Defines a custom address generator that creates simple addresses that
/// always use the format wasm1xxxxx to conform to Cosmos address formats
#[derive(Default)]
pub struct WasmAddressGenerator {
    address_counter: Cell<u64>,
}

impl AddressGenerator for WasmAddressGenerator {
    fn next_address(&self, _: &mut dyn Storage) -> Addr {
        let contract_number = self.address_counter.get() + 1;
        self.address_counter.set(contract_number);
        Addr::unchecked(format!("wasm1contract{}", contract_number))
    }
}
