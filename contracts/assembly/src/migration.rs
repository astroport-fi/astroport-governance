use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes a migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub proposal_voting_period: u64,
    pub proposal_effective_delay: u64,
    pub whitelisted_patterns: Option<Vec<String>>,
}

/// This structure stores general parameters for the Assembly contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigV100 {
    /// xASTRO token address
    pub xastro_token_addr: Addr,
    /// vxASTRO token address
    pub vxastro_token_addr: Addr,
    /// Builder unlock contract address
    pub builder_unlock_addr: Addr,
    /// Proposal voting period
    pub proposal_voting_period: u64,
    /// Proposal effective delay
    pub proposal_effective_delay: u64,
    /// Proposal expiration period
    pub proposal_expiration_period: u64,
    /// Proposal required deposit
    pub proposal_required_deposit: Uint128,
    /// Proposal required quorum
    pub proposal_required_quorum: Decimal,
    /// Proposal required threshold
    pub proposal_required_threshold: Decimal,
}

pub const CONFIGV100: Item<ConfigV100> = Item::new("config");
