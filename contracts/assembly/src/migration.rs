use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, CosmosMsg, Decimal, DepsMut, StdResult, Storage, Uint128, Uint64};
use cw_storage_plus::{Item, Map};

use astroport_governance::assembly::{Config, Proposal, ProposalMessage, ProposalStatus};

use crate::state::{CONFIG, PROPOSALS};

/// This structure describes a migration message.
#[cw_serde]
pub struct MigrateMsg {
    ibc_controller: Option<String>,
}

pub fn migrate_config(deps: &mut DepsMut, msg: &MigrateMsg) -> StdResult<()> {
    #[cw_serde]
    struct ConfigV110 {
        pub xastro_token_addr: Addr,
        pub vxastro_token_addr: Option<Addr>,
        pub builder_unlock_addr: Addr,
        pub proposal_voting_period: u64,
        pub proposal_effective_delay: u64,
        pub proposal_expiration_period: u64,
        pub proposal_required_deposit: Uint128,
        pub proposal_required_quorum: Decimal,
        pub proposal_required_threshold: Decimal,
        pub whitelisted_links: Vec<String>,
    }

    let config: ConfigV110 = Item::new("config").load(deps.storage)?;
    let mut config = Config {
        builder_unlock_addr: config.builder_unlock_addr,
        ibc_controller: None,
        proposal_effective_delay: config.proposal_effective_delay,
        proposal_expiration_period: config.proposal_expiration_period,
        proposal_required_deposit: config.proposal_required_deposit,
        proposal_required_quorum: config.proposal_required_quorum,
        proposal_required_threshold: config.proposal_required_threshold,
        proposal_voting_period: config.proposal_voting_period,
        vxastro_token_addr: config.vxastro_token_addr,
        whitelisted_links: config.whitelisted_links,
        xastro_token_addr: config.xastro_token_addr,
    };

    if let Some(ref ibc_controller) = msg.ibc_controller {
        config.ibc_controller = Some(deps.api.addr_validate(ibc_controller)?);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(())
}

#[cw_serde]
enum ProposalStatus110 {
    Active,
    Executed,
    Expired,
    Passed,
    Rejected,
}

/// This structure describes an old proposal message.
#[cw_serde]
pub struct OldProposalMessage {
    /// Order of execution of the message
    pub order: Uint64,
    /// Execution message
    pub msg: CosmosMsg,
}

#[cw_serde]
struct ProposalV110 {
    pub proposal_id: Uint64,
    pub submitter: Addr,
    pub status: ProposalStatus110,
    pub for_power: Uint128,
    pub against_power: Uint128,
    pub for_voters: Vec<Addr>,
    pub against_voters: Vec<Addr>,
    pub start_block: u64,
    pub start_time: u64,
    pub end_block: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub messages: Option<Vec<OldProposalMessage>>,
    pub deposit_amount: Uint128,
}

const PROPOSALS_V110: Map<u64, ProposalV110> = Map::new("proposals");

pub fn migrate_proposals_from_v110(storage: &mut dyn Storage) -> StdResult<()> {
    let proposals = PROPOSALS_V110
        .range(storage, None, None, cosmwasm_std::Order::Ascending {})
        .collect::<StdResult<Vec<_>>>()?;

    for (key, proposal) in proposals {
        PROPOSALS.save(
            storage,
            key,
            &Proposal {
                proposal_id: proposal.proposal_id,
                submitter: proposal.submitter,
                status: match proposal.status {
                    ProposalStatus110::Active => ProposalStatus::Active,
                    ProposalStatus110::Executed => ProposalStatus::Executed,
                    ProposalStatus110::Expired => ProposalStatus::Expired,
                    ProposalStatus110::Passed => ProposalStatus::Passed,
                    ProposalStatus110::Rejected => ProposalStatus::Rejected,
                },
                for_power: proposal.for_power,
                against_power: proposal.against_power,
                for_voters: proposal.for_voters,
                against_voters: proposal.against_voters,
                start_block: proposal.start_block,
                start_time: proposal.start_time,
                end_block: proposal.end_block,
                title: proposal.title,
                description: proposal.description,
                link: proposal.link,
                messages: proposal.messages.map(|v| {
                    v.into_iter()
                        .map(|m| ProposalMessage { msg: m.msg })
                        .collect()
                }),
                deposit_amount: proposal.deposit_amount,
                ibc_channel: None,
            },
        )?;
    }
    Ok(())
}

#[cw_serde]
pub struct ProposalV121 {
    pub proposal_id: Uint64,
    pub submitter: Addr,
    pub status: ProposalStatus,
    pub for_power: Uint128,
    pub against_power: Uint128,
    pub for_voters: Vec<Addr>,
    pub against_voters: Vec<Addr>,
    pub start_block: u64,
    pub start_time: u64,
    pub end_block: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub messages: Option<Vec<OldProposalMessage>>,
    pub deposit_amount: Uint128,
    pub ibc_channel: Option<String>,
}

const PROPOSALS_V121: Map<u64, ProposalV121> = Map::new("proposals");

pub fn migrate_proposals_from_v121(storage: &mut dyn Storage) -> StdResult<()> {
    let proposals = PROPOSALS_V121
        .range(storage, None, None, cosmwasm_std::Order::Ascending {})
        .collect::<StdResult<Vec<_>>>()?;

    for (key, proposal) in proposals {
        PROPOSALS.save(
            storage,
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
                title: proposal.title,
                description: proposal.description,
                link: proposal.link,
                messages: proposal.messages.map(|v| {
                    v.into_iter()
                        .map(|m| ProposalMessage { msg: m.msg })
                        .collect()
                }),
                deposit_amount: proposal.deposit_amount,
                ibc_channel: proposal.ibc_channel,
            },
        )?;
    }
    Ok(())
}
