use astroport::common::OwnershipProposal;
use cw_storage_plus::{Item, Map};

use astroport_governance::tributes::{Config, TributeInfo};

/// Stores the contract config.
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores tributes. Key (epoch start, lp_token, asset_info) -> amount
/// asset_info is a binary representing [`AssetInfo`] converted with [`asset_info_key`],
pub const TRIBUTES: Map<(u64, &str, &[u8]), TributeInfo> = Map::new("tributes");
/// Last claim timestamp for a user. Key (user_addr) -> timestamp
pub const USER_LAST_CLAIM_EPOCH: Map<&str, u64> = Map::new("user_last_claim_epoch");
/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
