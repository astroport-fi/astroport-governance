use astroport::asset::addr_validate_to_lower;
use astroport_governance::{
    assembly::{Config, Proposal, ProposalMessage, ProposalStatus},
    U64Key,
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{DepsMut, StdError, StdResult, Storage};

use crate::state::{CONFIG, PROPOSALS};

/// This structure describes a migration message.
#[cw_serde]
pub struct MigrateMsg {
    ibc_controller: Option<String>,
}

pub fn migrate_config(deps: &mut DepsMut, msg: &MigrateMsg) -> StdResult<()> {
    let config = astro_assembly110::state::CONFIG.load(deps.storage)?;
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
        config.ibc_controller = Some(addr_validate_to_lower(deps.api, ibc_controller)?);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(())
}

pub fn migrate_proposals(storage: &mut dyn Storage) -> StdResult<()> {
    let proposals = astro_assembly110::state::PROPOSALS
        .range(storage, None, None, cosmwasm_std::Order::Ascending {})
        .collect::<Result<Vec<_>, StdError>>()?;

    for (key, proposal) in proposals {
        use astroport_governance110::assembly::ProposalStatus as ProposalStatus110;
        PROPOSALS.save(
            storage,
            U64Key::new(key),
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
                        .map(|m| ProposalMessage {
                            msg: m.msg,
                            order: m.order,
                        })
                        .collect()
                }),
                deposit_amount: proposal.deposit_amount,
                ibc_channel: None,
            },
        )?;
    }
    Ok(())
}
