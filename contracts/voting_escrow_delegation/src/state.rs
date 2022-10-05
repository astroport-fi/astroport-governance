use astroport_governance::astroport::common::OwnershipProposal;
use astroport_governance::voting_escrow_delegation::{Config, Token};
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Delegated voting power are stored using a (contract_addr => token_ID) key
pub const DELEGATED: Map<(&Addr, String), Token> = Map::new("delegated");

/// Delegated token history is stored using a token ID key
pub const TOKENS: Map<String, Token> = Map::new("tokens");
