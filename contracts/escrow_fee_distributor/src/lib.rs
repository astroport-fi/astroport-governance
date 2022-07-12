pub mod contract;
mod error;
pub mod state;
mod utils;

// During development this import could be replaced with another astroport version.
// However, in production, the astroport version should be the same for all contracts.
use astroport_governance::astroport;

#[cfg(test)]
mod testing;
