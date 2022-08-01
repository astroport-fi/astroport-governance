use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport_governance::astroport::common::OwnershipProposal;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, SnapshotMap, Strategy};

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// This structure stores the main parameters for the voting escrow delegation contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// Astroport NFT contract address
    pub nft_addr: Addr,
    /// vxASTRO contract address
    pub voting_escrow_addr: Addr,
}

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Token {
    /// The amount of voting power to be delegated
    pub bias: Uint128,
    /// Weekly voting power decay
    pub slope: Uint128,
    /// The start period when the delegated voting power start to decrease
    pub start: u64,
    /// The period when the delegated voting power should expire
    pub expire_period: u64,
}

/// Stores all user delegate history
pub const DELEGATED: SnapshotMap<(Addr, String), Token> = SnapshotMap::new(
    "delegated",
    "delegated__checkpoints",
    "delegated__changelog",
    Strategy::EveryBlock,
);

/// Stores all token history
pub const TOKENS: SnapshotMap<String, Token> = SnapshotMap::new(
    "tokens",
    "tokens__checkpoints",
    "tokens__changelog",
    Strategy::EveryBlock,
);
