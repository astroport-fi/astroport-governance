use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;

/// Describes the execute messages available in the contract
#[cw_serde]
pub enum ExecuteMsg {
    /// Kick an unlocked voter's voting power from the Generator Controller lite
    KickUnlocked {
        /// The address of the user to kick
        user: Addr,
    },
}
