use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
    /// The contract owner address
    pub owner: String,
    /// Astroport NFT token code identifier
    pub nft_token_code_id: u64,
    /// vxASTRO contract address
    pub voting_escrow_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    DelegateVxAstro {
        receiver: String,
        percentage: Uint128,
        cancel_time: u64,
        expire_time: u64,
        id: String,
    },
    CreateDelegation {
        percentage: Uint128,
        cancel_time: u64,
        expire_time: u64,
        id: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    AdjustedBalance { account: String },
    AdjustedBalanceAt { account: String, timestamp: u64 },
}
