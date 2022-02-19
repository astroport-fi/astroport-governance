use cosmwasm_std::{Addr, CosmosMsg, Decimal, StdError, StdResult, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter, Result};

pub const MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE: u64 = 50;
pub const MAX_PROPOSAL_REQUIRED_PERCENTAGE: u64 = 100;

/// ## Description
/// This structure holds the parameters used for creating an Assembly contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Address of xASTRO token
    pub xastro_token_addr: String,
    /// Address of vxASTRO token
    pub vxastro_token_addr: String,
    /// Address of the builder unlock contract
    pub builder_unlock_addr: String,
    /// Proposal voting period
    pub proposal_voting_period: u64,
    /// Proposal effective delay
    pub proposal_effective_delay: u64,
    /// Proposal expiration period
    pub proposal_expiration_period: u64,
    /// Proposal required deposit
    pub proposal_required_deposit: Uint128,
    /// Proposal required quorum
    pub proposal_required_quorum: String,
    /// Proposal required threshold
    pub proposal_required_threshold: String,
}

/// # Description
/// This enum describes all execute functions available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Receive a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// Cast a vote for an active proposal
    CastVote {
        /// Proposal identifier
        proposal_id: u64,
        /// Vote option
        vote: ProposalVoteOption,
    },
    /// Set the status of a proposal that expired
    EndProposal {
        /// Proposal identifier
        proposal_id: u64,
    },
    /// Execute a successful proposal
    ExecuteProposal {
        /// Proposal identifier
        proposal_id: u64,
    },
    /// Remove a proposal that was already executed (or failed/expired)
    RemoveCompletedProposal {
        /// Proposal identifier
        proposal_id: u64,
    },
    /// Update parameters in the Assembly contract
    /// ## Executor
    /// Only the Assembly contract is allowed to update its own parameters
    UpdateConfig(UpdateConfig),
}

/// # Description
/// Thie enum describes all the queries available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return the contract's configuration
    Config {},
    /// Return the current list of proposals
    Proposals {
        /// Id from which to start querying
        start: Option<u64>,
        /// The amount of proposals to return
        limit: Option<u32>,
    },
    /// Return information about a specific proposal
    Proposal { proposal_id: u64 },
    /// Return information about the votes cast on a specific proposal
    ProposalVotes { proposal_id: u64 },
}

/// ## Description
/// This structure stores data for a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Submit a new proposal in the Assembly
    SubmitProposal {
        title: String,
        description: String,
        link: Option<String>,
        messages: Option<Vec<ProposalMessage>>,
    },
}

/// ## Description
/// This structure stores general parameters for the Assembly contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// xASTRO token address
    pub xastro_token_addr: Addr,
    /// vxASTRO token address
    pub vxastro_token_addr: Addr,
    /// Builder unlock contract address
    pub builder_unlock_addr: Addr,
    /// Proposal voting period
    pub proposal_voting_period: u64,
    /// Proposal effective delay
    pub proposal_effective_delay: u64,
    /// Proposal expiration period
    pub proposal_expiration_period: u64,
    /// Proposal required deposit
    pub proposal_required_deposit: Uint128,
    /// Proposal required quorum
    pub proposal_required_quorum: Decimal,
    /// Proposal required threshold
    pub proposal_required_threshold: Decimal,
}

impl Config {
    pub fn validate(&self) -> StdResult<()> {
        if self.proposal_required_threshold > Decimal::percent(MAX_PROPOSAL_REQUIRED_PERCENTAGE)
            || self.proposal_required_threshold
                < Decimal::percent(MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE)
        {
            return Err(StdError::generic_err(format!(
                "The required threshold for a proposal cannot be lower than {}% or higher than {}%",
                MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE, MAX_PROPOSAL_REQUIRED_PERCENTAGE
            )));
        }

        if self.proposal_required_quorum > Decimal::percent(100u64) {
            return Err(StdError::generic_err(format!(
                "The required quorum for a proposal cannot be higher than {}%",
                MAX_PROPOSAL_REQUIRED_PERCENTAGE
            )));
        }

        Ok(())
    }
}

/// ## Description
/// This structure sotres the params used when updating the main Assembly contract params.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdateConfig {
    /// xASTRO token address
    pub xastro_token_addr: Option<String>,
    /// vxASTRO token address
    pub vxastro_token_addr: Option<String>,
    /// Builder unlock contract address
    pub builder_unlock_addr: Option<String>,
    /// Proposal voting period
    pub proposal_voting_period: Option<u64>,
    /// Proposal effective delay
    pub proposal_effective_delay: Option<u64>,
    /// Proposal expiration period
    pub proposal_expiration_period: Option<u64>,
    /// Proposal required deposit
    pub proposal_required_deposit: Option<u128>,
    /// Proposal required quorum
    pub proposal_required_quorum: Option<String>,
    /// Proposal required threshold
    pub proposal_required_threshold: Option<String>,
}

/// ## Description
/// This structure stores data for a proposal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Proposal {
    /// Unique proposal ID
    pub proposal_id: Uint64,
    /// The address of the proposal submitter
    pub submitter: Addr,
    /// Status of the proposal
    pub status: ProposalStatus,
    /// `For` power of proposal
    pub for_power: Uint128,
    /// `Against` power of proposal
    pub against_power: Uint128,
    /// `For` votes for the proposal
    pub for_voters: Vec<Addr>,
    /// `Against` votes for the proposal
    pub against_voters: Vec<Addr>,
    /// Start block of proposal
    pub start_block: u64,
    /// Start time of proposal
    pub start_time: u64,
    /// End block of proposal
    pub end_block: u64,
    /// Proposal title
    pub title: String,
    /// Proposal description
    pub description: String,
    /// Proposal link
    pub link: Option<String>,
    /// Proposal messages
    pub messages: Option<Vec<ProposalMessage>>,
    /// Amount of xASTRO deposited in order to post the proposal
    pub deposit_amount: Uint128,
}

/// ## Description
/// This enum describes available statuses/states for a Proposal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    Executed,
    Expired,
}

impl Display for ProposalStatus {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        match self {
            ProposalStatus::Active {} => fmt.write_str("active"),
            ProposalStatus::Passed {} => fmt.write_str("passed"),
            ProposalStatus::Rejected {} => fmt.write_str("rejected"),
            ProposalStatus::Executed {} => fmt.write_str("executed"),
            ProposalStatus::Expired {} => fmt.write_str("expired"),
        }
    }
}

/// ## Description
/// This structure describes a proposal message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposalMessage {
    /// Order of execution of the message
    pub order: Uint64,
    /// Execution message
    pub msg: CosmosMsg,
}

/// ## Description
/// This structure describes a proposal vote.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposalVote {
    /// Voted option for the proposal
    pub option: ProposalVoteOption,
    /// Vote power
    pub power: Uint128,
}

/// ## Description
/// This enum describes available options for voting on a proposal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ProposalVoteOption {
    For,
    Against,
}

impl Display for ProposalVoteOption {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        match self {
            ProposalVoteOption::For {} => fmt.write_str("for"),
            ProposalVoteOption::Against {} => fmt.write_str("against"),
        }
    }
}

/// ## Description
/// This structure describes a proposal vote response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposalVotesResponse {
    /// Proposal identifier
    pub proposal_id: u64,
    /// Total amount of `for` votes for a proposal
    pub for_power: Uint128,
    /// Total amount of `against` votes for a proposal.
    pub against_power: Uint128,
}

/// ## Description
/// This structure describes proposal list response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposalListResponse {
    pub proposal_count: Uint64,
    pub proposal_list: Vec<Proposal>,
}
