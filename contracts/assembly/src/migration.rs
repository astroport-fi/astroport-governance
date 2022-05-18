use crate::state::PROPOSALS;
use astroport_governance::assembly::{Config, Proposal, ProposalMessage, ProposalStatus};
use cosmwasm_std::{Addr, Decimal, DepsMut, StdError, StdResult, Uint128, Uint64};
use cw_storage_plus::{Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes a migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub proposal_voting_period: Option<u64>,
    pub proposal_effective_delay: Option<u64>,
    pub whitelisted_links: Option<Vec<String>>,
}

/// This structure stores general parameters for the Assembly contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigV100 {
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

pub const CONFIGV100: Item<ConfigV100> = Item::new("config");

/// This structure stores general parameters for the Assembly contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigV101 {
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
    /// Whitelisted links
    pub whitelisted_links: Vec<String>,
}

pub const CONFIGV101: Item<ConfigV101> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposalV100 {
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

pub const PROPOSALS_V100: Map<U64Key, ProposalV100> = Map::new("proposals");

/// Migrate proposals to V1.1.1
pub(crate) fn migrate_proposals_to_v111(deps: &mut DepsMut, cfg: &Config) -> StdResult<()> {
    let proposals_v100 = PROPOSALS_V100
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending {})
        .map(|pair| {
            let (_, proposal) = pair?;
            Ok(proposal)
        })
        .collect::<Result<Vec<ProposalV100>, StdError>>()?;

    for proposal in proposals_v100 {
        PROPOSALS.save(
            deps.storage,
            U64Key::new(proposal.proposal_id.u64()),
            &Proposal {
                proposal_id: proposal.proposal_id,
                submitter: proposal.submitter,
                status: proposal.status,
                for_power: proposal.for_power,
                against_power: proposal.against_power,
                for_voters: proposal.for_voters,
                against_voters: proposal.against_voters,
                start_block: proposal.start_block,
                start_time: proposal.start_time,
                end_block: proposal.end_block,
                delayed_end_block: proposal.end_block + cfg.proposal_effective_delay,
                expiration_block: proposal.end_block
                    + cfg.proposal_effective_delay
                    + cfg.proposal_expiration_period,
                title: proposal.title,
                description: proposal.description,
                link: proposal.link,
                messages: proposal.messages,
                deposit_amount: proposal.deposit_amount,
            },
        )?;
    }

    Ok(())
}
