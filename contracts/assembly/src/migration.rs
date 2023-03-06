use crate::astroport::asset::addr_opt_validate;
use crate::state::{CONFIG, PROPOSALS};
use astroport_governance::assembly::{Config, Proposal, ProposalMessage, ProposalStatus};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, DepsMut, StdError, StdResult, Uint128, Uint64};
use cw_storage_plus::{Item, Map};

/// This structure describes a migration message.
#[cw_serde]
pub struct MigrateMsg {
    voting_escrow_delegator_addr: Option<String>,
    vxastro_token_addr: Option<String>,
    ibc_controller: Option<String>,
}

#[cw_serde]
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

#[cw_serde]
pub struct ProposalV130 {
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
}

pub const PROPOSALS_V100: Map<u64, ProposalV100> = Map::new("proposals");

/// This structure stores general parameters for the Assembly contract(v1.0.0).
#[cw_serde]
pub struct ConfigV100 {
    /// xASTRO token address
    pub xastro_token_addr: Addr,
    /// vxASTRO token address
    pub vxastro_token_addr: Option<Addr>,
    /// Voting Escrow delegator address
    pub voting_escrow_delegator_addr: Option<Addr>,
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

#[cw_serde]
pub struct ConfigV130 {
    /// xASTRO token address
    pub xastro_token_addr: Addr,
    /// vxASTRO token address
    pub vxastro_token_addr: Option<Addr>,
    /// Voting Escrow delegator address
    pub voting_escrow_delegator_addr: Option<Addr>,
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

pub const CONFIG_V100: Item<ConfigV100> = Item::new("config");
pub const CONFIG_V130: Item<ConfigV130> = Item::new("config");

/// Migrate proposals to V1.1.1
pub(crate) fn migrate_proposals_to_v111(deps: &mut DepsMut, cfg: &ConfigV100) -> StdResult<()> {
    let proposals_v100 = PROPOSALS_V100
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending {})
        .collect::<Result<Vec<_>, StdError>>()?;

    for (key, proposal) in proposals_v100 {
        PROPOSALS.save(
            deps.storage,
            key,
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
                ibc_channel: None,
            },
        )?;
    }

    Ok(())
}

/// Migrate proposals to V1.4.0
pub(crate) fn migrate_proposals_to_v140(deps: DepsMut) -> StdResult<()> {
    let v130_proposals_interface: Map<u64, ProposalV130> = Map::new("proposals");
    let proposals_v130 = v130_proposals_interface
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending {})
        .collect::<StdResult<Vec<_>>>()?;

    for (key, proposal) in proposals_v130 {
        PROPOSALS.save(
            deps.storage,
            key,
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
                delayed_end_block: proposal.delayed_end_block,
                expiration_block: proposal.expiration_block,
                title: proposal.title,
                description: proposal.description,
                link: proposal.link,
                messages: proposal.messages,
                deposit_amount: proposal.deposit_amount,
                ibc_channel: None,
            },
        )?;
    }

    Ok(())
}

/// Migrate contract config to V1.3.0
pub(crate) fn migrate_config_to_130(
    deps: &mut DepsMut,
    cfg_v100: ConfigV100,
    msg: MigrateMsg,
) -> StdResult<()> {
    let mut cfg = ConfigV130 {
        xastro_token_addr: cfg_v100.xastro_token_addr,
        vxastro_token_addr: cfg_v100.vxastro_token_addr,
        voting_escrow_delegator_addr: addr_opt_validate(
            deps.api,
            &msg.voting_escrow_delegator_addr,
        )?,
        builder_unlock_addr: cfg_v100.builder_unlock_addr,
        proposal_voting_period: cfg_v100.proposal_voting_period,
        proposal_effective_delay: cfg_v100.proposal_effective_delay,
        proposal_expiration_period: cfg_v100.proposal_expiration_period,
        proposal_required_deposit: cfg_v100.proposal_required_deposit,
        proposal_required_quorum: cfg_v100.proposal_required_quorum,
        proposal_required_threshold: cfg_v100.proposal_required_threshold,
        whitelisted_links: cfg_v100.whitelisted_links,
    };

    if let Some(vxastro_token_addr) = msg.vxastro_token_addr {
        cfg.vxastro_token_addr = Some(deps.api.addr_validate(vxastro_token_addr.as_str())?);
    }

    if cfg.voting_escrow_delegator_addr.is_some() && cfg.vxastro_token_addr.is_none() {
        return Err(StdError::generic_err(
            "The Voting Escrow contract should be specified to use the Voting Escrow Delegator contract."
        ));
    }

    CONFIG_V130.save(deps.storage, &cfg)?;

    Ok(())
}

/// Migrate contract config to V1.4.0
pub(crate) fn migrate_config_to_140(deps: DepsMut, msg: MigrateMsg) -> StdResult<()> {
    let cfg_v130 = CONFIG_V130.load(deps.storage)?;

    let cfg = Config {
        xastro_token_addr: cfg_v130.xastro_token_addr,
        vxastro_token_addr: cfg_v130.vxastro_token_addr,
        voting_escrow_delegator_addr: cfg_v130.voting_escrow_delegator_addr,
        ibc_controller: addr_opt_validate(deps.api, &msg.ibc_controller)?,
        builder_unlock_addr: cfg_v130.builder_unlock_addr,
        proposal_voting_period: cfg_v130.proposal_voting_period,
        proposal_effective_delay: cfg_v130.proposal_effective_delay,
        proposal_expiration_period: cfg_v130.proposal_expiration_period,
        proposal_required_deposit: cfg_v130.proposal_required_deposit,
        proposal_required_quorum: cfg_v130.proposal_required_quorum,
        proposal_required_threshold: cfg_v130.proposal_required_threshold,
        whitelisted_links: cfg_v130.whitelisted_links,
    };

    CONFIG.save(deps.storage, &cfg)?;

    Ok(())
}
