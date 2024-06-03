use crate::emissions_controller::msg::VxAstroIbcMsg;
use astroport::incentives::InputSchedule;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;

use crate::voting_escrow::UpdateMarketingInfo;

/// This structure describes the basic settings for creating a contract.
#[cw_serde]
pub struct OutpostInstantiateMsg {
    /// Contract owner
    pub owner: String,
    /// ASTRO denom on the chain
    pub astro_denom: String,
    /// vxASTRO contract code id
    pub vxastro_code_id: u64,
    /// vxASTRO token marketing info
    pub vxastro_marketing_info: Option<UpdateMarketingInfo>,
    /// xASTRO denom
    pub vxastro_deposit_denom: String,
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
    /// vxASTRO IBC channel
    pub voting_ibc_channel: String,
    /// Emissions controller on the Hub
    pub hub_emissions_controller: String,
    /// Official ICS20 IBC channel from this outpost to the Hub
    pub ics20_channel: String,
}
