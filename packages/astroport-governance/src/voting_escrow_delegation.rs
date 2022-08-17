use crate::voting_escrow_delegation::QueryMsg::AdjustedBalance;
use cosmwasm_std::{QuerierWrapper, StdResult, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
    /// The contract owner address
    pub owner: String,
    /// Astroport NFT code identifier
    pub nft_code_id: u64,
    /// vxASTRO contract address
    pub voting_escrow_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    CreateDelegation {
        /// The share of voting power (in bps) that will be delegated to the recipient
        bps: u16,
        expire_time: u64,
        token_id: String,
        recipient: String,
    },
    ExtendDelegation {
        /// The share of voting power (in bps) that will be delegated to the recipient
        bps: u16,
        expire_time: u64,
        token_id: String,
    },
    UpdateConfig {
        /// vxASTRO contract address
        new_voting_escrow: Option<String>,
    },
    /// Propose a new owner for the contract
    ProposeNewOwner { new_owner: String, expires_in: u64 },
    /// Remove the ownership transfer proposal
    DropOwnershipProposal {},
    /// Claim contract ownership
    ClaimOwnership {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    AdjustedBalance {
        account: String,
        timestamp: Option<u64>,
    },
    DelegatedVotingPower {
        account: String,
        timestamp: Option<u64>,
    },
}

/// Queries current user's adjusted voting power from the voting escrow delegation contract.
pub fn get_adjusted_balance(
    querier: &QuerierWrapper,
    escrow_delegation_addr: String,
    account: String,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    querier.query_wasm_smart(
        escrow_delegation_addr,
        &AdjustedBalance { account, timestamp },
    )
}
