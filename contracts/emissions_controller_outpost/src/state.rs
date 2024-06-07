use astroport::common::OwnershipProposal;
use cw_storage_plus::{Item, Map};

use astroport_governance::emissions_controller::msg::VxAstroIbcMsg;
use astroport_governance::emissions_controller::outpost::{Config, UserIbcError};

/// Stores config at the given key.
pub const CONFIG: Item<Config> = Item::new("config");

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
/// Stores the latest IBC error and message.
pub const USER_IBC_ERROR: Map<&str, UserIbcError> = Map::new("user_ibc_error");
/// Keeps the list of users with pending IBC requests.
/// The contract blocks any new IBC messages for these users
/// until the previous one is acknowledged, failed or timed out.
pub const PENDING_MESSAGES: Map<&str, VxAstroIbcMsg> = Map::new("pending_messages");
/// Map of registered proposals (proposal id -> start time).
/// Users are allowed to vote only on registered proposals.
pub const REGISTERED_PROPOSALS: Map<u64, u64> = Map::new("registered_proposals");
/// Contains all the voters per proposal. Map proposal id -> voter address.
pub const PROPOSAL_VOTERS: Map<(u64, String), ()> = Map::new("proposal_votes");
