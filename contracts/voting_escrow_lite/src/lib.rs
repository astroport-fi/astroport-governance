pub mod contract;
pub mod state;

// During development this import could be replaced with another astroport version.
// However, in production, the astroport version should be the same for all contracts.
pub use astroport_governance::astroport;

pub mod error;
pub mod execute;
mod marketing_validation;
pub mod query;
mod utils;
