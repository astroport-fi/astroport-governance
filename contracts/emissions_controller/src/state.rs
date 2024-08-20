use astroport::common::OwnershipProposal;
use cw_storage_plus::{Item, Map, SnapshotItem, SnapshotMap, Strategy};

use astroport_governance::emissions_controller::hub::{
    Config, OutpostInfo, TuneInfo, UserInfo, VotedPoolInfo,
};

/// Stores config at the given key.
pub const CONFIG: Item<Config> = Item::new("config");
/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
/// Array of pools eligible for voting.
pub const POOLS_WHITELIST: Item<Vec<String>> = Item::new("pools_whitelist");
/// Registered Astroport outposts with respective parameters.
pub const OUTPOSTS: Map<&str, OutpostInfo> = Map::new("outposts");
/// Historical user's voting information.
pub const USER_INFO: SnapshotMap<&str, UserInfo> = SnapshotMap::new(
    "user_info",
    "user_info____checkpoints",
    "user_info__changelog",
    Strategy::EveryBlock,
);
/// Historical pools voting power and the time when they were whitelisted.
pub const VOTED_POOLS: SnapshotMap<&str, VotedPoolInfo> = SnapshotMap::new(
    "voted_pools",
    "voted_pools____checkpoints",
    "voted_pools__changelog",
    Strategy::EveryBlock,
);
/// Historical tune information.
pub const TUNE_INFO: SnapshotItem<TuneInfo> = SnapshotItem::new(
    "tune_info",
    "tune_info____checkpoints",
    "tune_info__changelog",
    Strategy::EveryBlock,
);
