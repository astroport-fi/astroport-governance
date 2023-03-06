pub mod assembly;
pub mod builder_unlock;
pub mod escrow_fee_distributor;
pub mod generator_controller;
pub mod nft;
pub mod utils;
pub mod voting_escrow;
pub mod voting_escrow_delegation;

use assembly::ProposalMessage;
pub use astroport;
use cosmwasm_schema::cw_serde;

// Default pagination constants
pub const DEFAULT_LIMIT: u32 = 10;
pub const MAX_LIMIT: u32 = 30;

// TODO: replace all uses of this enum at the end of the merge
#[cw_serde]
pub enum ControllerExecuteMsg {
    IbcExecuteProposal {
        channel_id: String,
        proposal_id: u64,
        messages: Vec<ProposalMessage>,
    },
}
