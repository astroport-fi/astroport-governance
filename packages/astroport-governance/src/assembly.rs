use crate::assembly::helpers::is_safe_link;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, CosmosMsg, Decimal, StdError, StdResult, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;
use std::fmt::{Display, Formatter, Result};
use std::str::FromStr;

#[cfg(not(feature = "testnet"))]
mod proposal_constants {
    use std::ops::RangeInclusive;

    pub const MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE: u64 = 33;
    pub const MAX_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE: u64 = 100;
    pub const MAX_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE: &str = "1";
    pub const MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE: &str = "0.01";
    pub const VOTING_PERIOD_INTERVAL: RangeInclusive<u64> = 12342..=7 * 12342;
    // from 0.5 to 1 day in blocks (7 seconds per block)
    pub const DELAY_INTERVAL: RangeInclusive<u64> = 6171..=12342;
    pub const EXPIRATION_PERIOD_INTERVAL: RangeInclusive<u64> = 12342..=86_399;
    // from 10k to 60k $xASTRO
    pub const DEPOSIT_INTERVAL: RangeInclusive<u128> = 10000000000..=60000000000;
}

#[cfg(feature = "testnet")]
mod proposal_constants {
    use std::ops::RangeInclusive;

    pub const MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE: u64 = 33;
    pub const MAX_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE: u64 = 100;
    pub const MAX_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE: &str = "1";
    pub const MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE: &str = "0.001";
    pub const VOTING_PERIOD_INTERVAL: RangeInclusive<u64> = 200..=7 * 12342;
    // from ~350 sec to 1 day in blocks (7 seconds per block)
    pub const DELAY_INTERVAL: RangeInclusive<u64> = 50..=12342;
    pub const EXPIRATION_PERIOD_INTERVAL: RangeInclusive<u64> = 400..=86_399;
    // from 0.001 to 60k $xASTRO
    pub const DEPOSIT_INTERVAL: RangeInclusive<u128> = 1000..=60000000000;
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
    /// Voting Escrow delegator address
    pub voting_escrow_delegator_addr: Option<String>,
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
    UpdateConfig(Box<UpdateConfig>),
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
    /// Return proposal voters of specified proposal
    #[returns(Vec<Addr>)]
    ProposalVoters {
        /// Proposal unique id
        proposal_id: u64,
        /// Proposal vote option
        vote_option: ProposalVoteOption,
        /// Id from which to start querying
        start: Option<u64>,
        /// The amount of proposals to return
        limit: Option<u32>,
    },
    /// Return information about a specific proposal
    #[returns(ProposalResponse)]
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
    /// Voting Escrow delegator address
    pub voting_escrow_delegator_addr: Option<Addr>,
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
                "The required threshold for a proposal cannot be lower than {MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE}% or higher than {MAX_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE}%"
            )));
        }

        let max_quorum = Decimal::from_str(MAX_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE)?;
        let min_quorum = Decimal::from_str(MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE)?;
        if self.proposal_required_quorum > max_quorum || self.proposal_required_quorum < min_quorum
        {
            return Err(StdError::generic_err(format!(
                "The required quorum for a proposal cannot be lower than {}% or higher than {}%",
                min_quorum * Decimal::from_ratio(100u8, 1u8),
                max_quorum * Decimal::from_ratio(100u8, 1u8)
            )));
        }

        if !DELAY_INTERVAL.contains(&self.proposal_effective_delay) {
            return Err(StdError::generic_err(format!(
                "The effective delay for a proposal cannot be lower than {} or higher than {}",
                DELAY_INTERVAL.start(),
                DELAY_INTERVAL.end()
            )));
        }

        if !EXPIRATION_PERIOD_INTERVAL.contains(&self.proposal_expiration_period) {
            return Err(StdError::generic_err(format!(
                "The expiration period for a proposal cannot be lower than {} or higher than {}",
                EXPIRATION_PERIOD_INTERVAL.start(),
                EXPIRATION_PERIOD_INTERVAL.end()
            )));
        }

        if !VOTING_PERIOD_INTERVAL.contains(&self.proposal_voting_period) {
            return Err(StdError::generic_err(format!(
                "The voting period for a proposal should be more than {} or less than {} blocks.",
                VOTING_PERIOD_INTERVAL.start(),
                VOTING_PERIOD_INTERVAL.end()
            )));
        }

        if !DEPOSIT_INTERVAL.contains(&self.proposal_required_deposit.u128()) {
            return Err(StdError::generic_err(format!(
                "The required deposit for a proposal cannot be lower than {} or higher than {}",
                DEPOSIT_INTERVAL.start(),
                DEPOSIT_INTERVAL.end()
            )));
        }

        if self.voting_escrow_delegator_addr.is_some() && self.vxastro_token_addr.is_none() {
            return Err(StdError::generic_err(
                "The Voting Escrow contract should be specified to use the Voting Escrow Delegator contract."
            ));
        }

        Ok(())
    }
}

/// This structure stores the params used when updating the main Assembly contract params.
#[cw_serde]
pub struct UpdateConfig {
    /// xASTRO token address
    pub xastro_token_addr: Option<String>,
    /// vxASTRO token address
    pub vxastro_token_addr: Option<String>,
    /// Voting Escrow delegator address
    pub voting_escrow_delegator_addr: Option<String>,
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
    /// Delayed end block of proposal
    pub delayed_end_block: u64,
    /// Expiration block of proposal
    pub expiration_block: u64,
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

/// This structure describes a proposal response.
#[cw_serde]
pub struct ProposalResponse {
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
    /// Start block of proposal
    pub start_block: u64,
    /// Start time of proposal
    pub start_time: u64,
    /// End block of proposal
    pub end_block: u64,
    /// Delayed end block of proposal
    pub delayed_end_block: u64,
    /// Expiration block of proposal
    pub expiration_block: u64,
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
    pub proposal_list: Vec<ProposalResponse>,
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
            if !(is_safe_link(link) && link.contains('.') && link.ends_with('/')) {
                return Err(StdError::generic_err(format!(
                    "Link is not properly formatted or contains unsafe characters: {link}."
                )));
            }
        }

        Ok(())
    }
}
