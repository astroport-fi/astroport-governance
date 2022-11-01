use ap_voting_escrow_delegation::Config;
use astroport::common::OwnershipProposal;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Token {
    /// The amount of voting power to be delegated
    pub power: Uint128,
    /// Weekly voting power decay
    pub slope: Uint128,
    /// The start period when the delegated voting power start to decrease
    pub start: u64,
    /// The period when the delegated voting power should expire
    pub expire_period: u64,
}

/// Delegated voting power are stored using a (contract_addr => token_ID) key
pub const DELEGATED: Map<(&Addr, String), Token> = Map::new("delegated");

/// Delegated token history is stored using a token ID key
pub const TOKENS: Map<String, Token> = Map::new("tokens");
