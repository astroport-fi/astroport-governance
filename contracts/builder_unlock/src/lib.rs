pub mod contract;
mod migration;
pub mod state;

// During development this import could be replaced with another astroport version.
// However, in production, the astroport version should be the same for all contracts.
pub use astroport_governance::astroport;
