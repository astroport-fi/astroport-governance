use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, QuerierWrapper, StdResult, Uint128};
use QueryMsg::AdjustedBalance;

/// This structure stores the main parameters for the voting escrow delegation contract.
#[cw_serde]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// Astroport NFT contract address
    pub nft_addr: Addr,
    /// vxASTRO contract address
    pub voting_escrow_addr: Addr,
}

#[cw_serde]
pub struct InstantiateMsg {
    /// The contract owner address
    pub owner: String,
    /// Astroport NFT code identifier
    pub nft_code_id: u64,
    /// vxASTRO contract address
    pub voting_escrow_addr: String,
}

#[cw_serde]
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

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
    #[returns(Uint128)]
    AdjustedBalance {
        account: String,
        timestamp: Option<u64>,
    },
    #[returns(Uint128)]
    DelegatedVotingPower {
        account: String,
        timestamp: Option<u64>,
    },
}

/// This structure describes a Migration message.
#[cw_serde]
pub struct MigrateMsg {}

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
