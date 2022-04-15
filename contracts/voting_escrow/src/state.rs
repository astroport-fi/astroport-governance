use astroport::common::OwnershipProposal;
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_storage_plus::{Item, Map, SnapshotMap, Strategy, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure stores the main parameters for the voting escrow contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// Address that can only blacklist vxASTRO stakers and remove their governance power
    pub guardian_addr: Addr,
    /// The xASTRO token contract address
    pub deposit_token_addr: Addr,
    /// The maximum % of staked xASTRO that is confiscated upon an early exit
    pub max_exit_penalty: Decimal,
    /// The address that receives slashed ASTRO (slashed xASTRO is burned in order to claim ASTRO)
    pub slashed_fund_receiver: Option<Addr>,
    /// The address of $ASTRO
    pub astro_addr: Addr,
    /// The address of $xASTRO staking contract
    pub xastro_staking_addr: Addr,
}

/// ## Description
/// This structure stores points along the checkpoint history for every vxASTRO staker.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Point {
    /// The staker's vxASTRO voting power
    pub power: Uint128,
    /// The start period when the staker's voting power start to decrease
    pub start: u64,
    /// The period when the lock should expire
    pub end: u64,
    /// Weekly voting power decay
    pub slope: Uint128,
}

/// ## Description
/// This structure stores data about the lockup position for a specific vxASTRO staker.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Lock {
    /// The total amount of xASTRO tokens that were deposited in the vxASTRO position
    pub amount: Uint128,
    /// The start period when the lock was created
    pub start: u64,
    /// The timestamp when the lock position expires
    pub end: u64,
    /// the last period when the lock's time was increased
    pub last_extend_lock_period: u64,
}

/// ## Description
/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores all user locks history
pub const LOCKED: SnapshotMap<Addr, Lock> = SnapshotMap::new(
    "locked",
    "locked__checkpoints",
    "locked__changelog",
    Strategy::EveryBlock,
);

/// ## Description
/// Stores the checkpoint history for every staker (addr => period)
/// Total voting power checkpoints are stored using a (contract_addr => period) key
pub const HISTORY: Map<(Addr, U64Key), Point> = Map::new("history");

/// ## Description
/// Scheduled slope changes per period (week)
pub const SLOPE_CHANGES: Map<U64Key, Uint128> = Map::new("slope_changes");

/// ## Description
/// Last period when a scheduled slope change was applied
pub const LAST_SLOPE_CHANGE: Item<u64> = Item::new("last_slope_change");

/// ## Description
/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// ## Description
/// Contains blacklisted staker addresses
pub const BLACKLIST: Item<Vec<Addr>> = Item::new("blacklist");
