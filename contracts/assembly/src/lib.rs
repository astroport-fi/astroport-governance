pub mod contract;
pub mod error;
pub mod state;

/// Exclusively to bypass wasmd migration limitation. Assembly doesn't have IBC features.
/// https://github.com/CosmWasm/wasmd/blob/7165e41cbf14d60a9fef4fb1e04c2c2e5e4e0cf4/x/wasm/keeper/keeper.go#L446
pub mod ibc;
pub mod queries;
pub mod utils;

pub mod migration;
#[cfg(test)]
mod unit_tests;
