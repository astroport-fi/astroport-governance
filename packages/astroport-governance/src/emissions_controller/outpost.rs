use astroport::incentives::InputSchedule;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;

use crate::assembly::ProposalVoteOption;
use crate::emissions_controller::msg::VxAstroIbcMsg;
use crate::voting_escrow::UpdateMarketingInfo;

/// This structure describes the basic settings for creating a contract.
#[cw_serde]
pub struct OutpostInstantiateMsg {
    /// Contract owner
    pub owner: String,
    /// ASTRO denom on the chain
    pub astro_denom: String,
    /// xASTRO denom
    pub xastro_denom: String,
    /// vxASTRO contract code id
    pub vxastro_code_id: u64,
    /// vxASTRO token marketing info
    pub vxastro_marketing_info: UpdateMarketingInfo,
    /// Astroport Factory contract
    pub factory: String,
    /// Emissions controller on the Hub
    pub hub_emissions_controller: String,
    /// Official ICS20 IBC channel from this outpost to the Hub
    pub ics20_channel: String,
}

#[cw_serde]
pub enum OutpostMsg {
    /// SetEmissions is a permissionless endpoint that allows setting ASTRO emissions for the next epoch
    /// from the Hub by leveraging IBC hooks.
    SetEmissions {
        schedules: Vec<(String, InputSchedule)>,
    },
    /// Same as SetEmissions but it allows using funds from contract balance (if available).
    /// This endpoint can be called only by contract owner. It is meant to be used in case of
    /// IBC hook wasn't triggered upon ics20 packet arrival, for example, if a chain doesn't support IBC hooks.
    PermissionedSetEmissions {
        schedules: Vec<(String, InputSchedule)>,
    },
    /// Permissioned to the contract owner.
    /// Allows to clawback ASTRO tokens bridged but not yet used in schedules.
    /// ASTRO tokens are sent back to the Hub emissions controller.
    ClawbackAstro {},
    /// Allows using vxASTRO voting power to vote on general DAO proposals.
    /// The contract requires a proposal with specific id to be registered via
    /// a special permissionless IBC message.
    CastVote {
        /// Proposal id
        proposal_id: u64,
        /// Vote option
        vote: ProposalVoteOption,
    },
    UpdateConfig {
        /// Voting IBC wasm<>wasm channel
        voting_ibc_channel: Option<String>,
        /// Emissions controller on the Hub
        hub_emissions_controller: Option<String>,
        /// Official ICS20 IBC channel from this outpost to the Hub
        ics20_channel: Option<String>,
    },
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Config returns the contract configuration
    #[returns(Config)]
    Config {},
    /// QueryUserIbcStatus returns the status of the user's IBC request.
    /// Whether they have a pending request or an error.
    #[returns(UserIbcStatus)]
    QueryUserIbcStatus { user: String },
    /// QueryRegisteredProposals returns the list of registered proposals.
    #[returns(Vec<RegisteredProposal>)]
    QueryRegisteredProposals {
        limit: Option<u8>,
        start_after: Option<u64>,
    },
    /// QueryProposalVoters returns the list of voters for the proposal.
    #[returns(Vec<String>)]
    QueryProposalVoters {
        proposal_id: u64,
        limit: Option<u8>,
        start_after: Option<String>,
    },
}

/// Contains failed IBC along with the error message
#[cw_serde]
pub struct UserIbcError {
    pub msg: VxAstroIbcMsg,
    pub err: String,
}

/// Contains the pending IBC message or an error
#[cw_serde]
pub struct UserIbcStatus {
    pub pending_msg: Option<VxAstroIbcMsg>,
    pub error: Option<UserIbcError>,
}

/// General contract configuration
#[cw_serde]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// vxASTRO contract address
    pub vxastro: Addr,
    /// ASTRO denom on the chain
    pub astro_denom: String,
    /// Astroport Factory contract
    pub factory: Addr,
    /// The Astroport Incentives contract
    pub incentives_addr: Addr,
    /// vxASTRO IBC channel
    pub voting_ibc_channel: String,
    /// Emissions controller on the Hub
    pub hub_emissions_controller: String,
    /// ICS20 IBC channel from this outpost to the Hub
    pub ics20_channel: String,
}

/// Contains the proposal id and the start time.
/// Used exclusively in query response.
#[cw_serde]
pub struct RegisteredProposal {
    pub id: u64,
    pub start_time: u64,
}
