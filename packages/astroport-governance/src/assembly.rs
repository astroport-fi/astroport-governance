use crate::assembly::helpers::is_safe_link;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, CosmosMsg, Decimal, StdError, StdResult, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;
use std::fmt::{Display, Formatter, Result};

#[cfg(not(feature = "testnet"))]
mod proposal_constants {
    pub const MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE: u64 = 33;
    pub const MAX_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE: u64 = 100;
    pub const MINIMUM_DELAY: u64 = 14_400;
    pub const MINIMUM_EXPIRATION_PERIOD: u64 = 100_800;
}

#[cfg(feature = "testnet")]
mod proposal_constants {
    pub const MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE: u64 = 33;
    pub const MAX_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE: u64 = 100;
    pub const MINIMUM_DELAY: u64 = 50;
    pub const MINIMUM_EXPIRATION_PERIOD: u64 = 400;
}

pub use proposal_constants::*;

/// Proposal validation attributes
const MIN_TITLE_LENGTH: usize = 4;
const MAX_TITLE_LENGTH: usize = 64;
const MIN_DESC_LENGTH: usize = 4;
const MAX_DESC_LENGTH: usize = 1024;
const MIN_LINK_LENGTH: usize = 12;
const MAX_LINK_LENGTH: usize = 128;

/// Special characters that are allowed in proposal text
const SAFE_TEXT_CHARS: &str = "!&?#()*+'-./\"";

/// This structure holds the parameters used for creating an Assembly contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Address of xASTRO token
    pub xastro_token_addr: String,
    /// Address of vxASTRO token
    pub vxastro_token_addr: Option<String>,
    /// Astroport IBC controller contract
    pub ibc_controller: Option<String>,
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
    /// Whitelisted links
    pub whitelisted_links: Vec<String>,
}

/// This enum describes all execute functions available in the contract.
#[cw_serde]
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
    /// Check messages execution
    CheckMessages {
        /// messages
        messages: Vec<ProposalMessage>,
    },
    /// The last endpoint which is executed only if all proposal messages have been passed
    CheckMessagesPassed {},
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
    /// Update proposal status InProgress -> Executed or Failed.
    /// ## Executor
    /// Only the IBC controller contract is allowed to call this method.
    IBCProposalCompleted {
        proposal_id: u64,
        status: ProposalStatus,
    },
}

/// Thie enum describes all the queries available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Return the contract's configuration
    #[returns(Config)]
    Config {},
    /// Return the current list of proposals
    #[returns(ProposalListResponse)]
    Proposals {
        /// Id from which to start querying
        start: Option<u64>,
        /// The amount of proposals to return
        limit: Option<u32>,
    },
    /// Return information about a specific proposal
    #[returns(Proposal)]
    Proposal { proposal_id: u64 },
    /// Return information about the votes cast on a specific proposal
    #[returns(ProposalVotesResponse)]
    ProposalVotes { proposal_id: u64 },
    /// Return user voting power for a specific proposal
    #[returns(Uint128)]
    UserVotingPower { user: String, proposal_id: u64 },
    /// Return total voting power for a specific proposal
    #[returns(Uint128)]
    TotalVotingPower { proposal_id: u64 },
}

/// This structure stores data for a CW20 hook message.
#[cw_serde]
pub enum Cw20HookMsg {
    /// Submit a new proposal in the Assembly
    SubmitProposal {
        title: String,
        description: String,
        link: Option<String>,
        messages: Option<Vec<ProposalMessage>>,
        /// If proposal should be executed on a remote chain this field should specify governance channel
        ibc_channel: Option<String>,
    },
}

/// This structure stores general parameters for the Assembly contract.
#[cw_serde]
pub struct Config {
    /// xASTRO token address
    pub xastro_token_addr: Addr,
    /// vxASTRO token address
    pub vxastro_token_addr: Option<Addr>,
    /// Astroport IBC controller contract
    pub ibc_controller: Option<Addr>,
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
    /// Whitelisted links
    pub whitelisted_links: Vec<String>,
}

impl Config {
    pub fn validate(&self) -> StdResult<()> {
        if self.proposal_required_threshold
            > Decimal::percent(MAX_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE)
            || self.proposal_required_threshold
                < Decimal::percent(MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE)
        {
            return Err(StdError::generic_err(format!(
                "The required threshold for a proposal cannot be lower than {}% or higher than {}%",
                MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE,
                MAX_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE
            )));
        }

        if self.proposal_required_quorum
            > Decimal::percent(MAX_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE)
        {
            return Err(StdError::generic_err(format!(
                "The required quorum for a proposal cannot be higher than {}%",
                MAX_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE
            )));
        }

        if self.proposal_effective_delay < MINIMUM_DELAY {
            return Err(StdError::generic_err(format!(
                "The effective delay for a proposal cannot be less than {} blocks.",
                MINIMUM_DELAY
            )));
        }

        if self.proposal_expiration_period < MINIMUM_EXPIRATION_PERIOD {
            return Err(StdError::generic_err(format!(
                "The expiration period for a proposal cannot be less than {} blocks.",
                MINIMUM_EXPIRATION_PERIOD
            )));
        }

        Ok(())
    }
}

/// This structure sotres the params used when updating the main Assembly contract params.
#[cw_serde]
pub struct UpdateConfig {
    /// Astroport IBC controller contract
    pub ibc_controller: Option<String>,
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
    /// Links to remove from whitelist
    pub whitelist_remove: Option<Vec<String>>,
    /// Links to add to whitelist
    pub whitelist_add: Option<Vec<String>>,
}

/// This structure stores data for a proposal.
#[cw_serde]
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
    /// IBC channel
    pub ibc_channel: Option<String>,
}

impl Proposal {
    pub fn validate(&self, whitelisted_links: Vec<String>) -> StdResult<()> {
        // Title validation
        if self.title.len() < MIN_TITLE_LENGTH {
            return Err(StdError::generic_err("Title too short!"));
        }
        if self.title.len() > MAX_TITLE_LENGTH {
            return Err(StdError::generic_err("Title too long!"));
        }
        if !self.title.chars().all(|c| {
            c.is_ascii_alphanumeric() || c.is_ascii_whitespace() || SAFE_TEXT_CHARS.contains(c)
        }) {
            return Err(StdError::generic_err(
                "Title is not in alphanumeric format!",
            ));
        }

        // Description validation
        if self.description.len() < MIN_DESC_LENGTH {
            return Err(StdError::generic_err("Description too short!"));
        }
        if self.description.len() > MAX_DESC_LENGTH {
            return Err(StdError::generic_err("Description too long!"));
        }
        if !self.description.chars().all(|c| {
            c.is_ascii_alphanumeric() || c.is_ascii_whitespace() || SAFE_TEXT_CHARS.contains(c)
        }) {
            return Err(StdError::generic_err(
                "Description is not in alphanumeric format",
            ));
        }

        // Link validation
        if let Some(link) = &self.link {
            if link.len() < MIN_LINK_LENGTH {
                return Err(StdError::generic_err("Link too short!"));
            }
            if link.len() > MAX_LINK_LENGTH {
                return Err(StdError::generic_err("Link too long!"));
            }
            if !whitelisted_links.iter().any(|wl| link.starts_with(wl)) {
                return Err(StdError::generic_err("Link is not whitelisted!"));
            }
            if !is_safe_link(link) {
                return Err(StdError::generic_err(
                    "Link is not properly formatted or contains unsafe characters!",
                ));
            }
        }

        Ok(())
    }
}

/// This enum describes available statuses/states for a Proposal.
#[cw_serde]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    InProgress,
    Failed,
    Executed,
    Expired,
}

impl Display for ProposalStatus {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        match self {
            ProposalStatus::Active {} => fmt.write_str("active"),
            ProposalStatus::Passed {} => fmt.write_str("passed"),
            ProposalStatus::Rejected {} => fmt.write_str("rejected"),
            ProposalStatus::InProgress => fmt.write_str("in_progress"),
            ProposalStatus::Failed => fmt.write_str("failed"),
            ProposalStatus::Executed {} => fmt.write_str("executed"),
            ProposalStatus::Expired {} => fmt.write_str("expired"),
        }
    }
}

/// This structure describes a proposal message.
#[cw_serde]
pub struct ProposalMessage {
    /// Order of execution of the message
    pub order: Uint64,
    /// Execution message
    pub msg: CosmosMsg,
}

/// This structure describes a proposal vote.
#[cw_serde]
pub struct ProposalVote {
    /// Voted option for the proposal
    pub option: ProposalVoteOption,
    /// Vote power
    pub power: Uint128,
}

/// This enum describes available options for voting on a proposal.
#[cw_serde]
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

/// This structure describes a proposal vote response.
#[cw_serde]
pub struct ProposalVotesResponse {
    /// Proposal identifier
    pub proposal_id: u64,
    /// Total amount of `for` votes for a proposal
    pub for_power: Uint128,
    /// Total amount of `against` votes for a proposal.
    pub against_power: Uint128,
}

/// This structure describes a proposal list response.
#[cw_serde]
pub struct ProposalListResponse {
    /// The amount of proposals returned
    pub proposal_count: Uint64,
    /// The list of proposals that are returned
    pub proposal_list: Vec<Proposal>,
}

pub mod helpers {
    use cosmwasm_std::{StdError, StdResult};

    const SAFE_LINK_CHARS: &str = "-_:/?#@!$&()*+,;=.~[]'%";

    /// Checks if the link is valid. Returns a boolean value.
    pub fn is_safe_link(link: &str) -> bool {
        link.chars()
            .all(|c| c.is_ascii_alphanumeric() || SAFE_LINK_CHARS.contains(c))
    }

    /// Validating the list of links. Returns an error if a list has an invalid link.
    pub fn validate_links(links: &[String]) -> StdResult<()> {
        for link in links {
            if !(is_safe_link(link) && link.ends_with('/')) {
                return Err(StdError::generic_err(format!(
                    "Link is not properly formatted or contains unsafe characters: {}.",
                    link
                )));
            }
        }

        Ok(())
    }
}
