use cosmwasm_std::{to_binary, DepsMut, IbcReceiveResponse};

use astroport_governance::{
    assembly::Proposal,
    assembly::QueryMsg,
    interchain::{ProposalSnapshot, Response},
};

use crate::{error::ContractError, state::CONFIG};

/// Query the Assembly for a proposal and return the result in an
/// IBC acknowledgement
///
/// If the proposal doesn't exist, the Outpost will see a generic ABCI error
/// and not "proposal not found" due to limitations in wasmd
pub fn handle_ibc_query_proposal(
    deps: DepsMut,
    id: u64,
) -> Result<IbcReceiveResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let proposal: Proposal = deps.querier.query_wasm_smart(
        config.assembly_addr,
        &QueryMsg::Proposal { proposal_id: id },
    )?;

    let proposal_snapshot = ProposalSnapshot {
        id: proposal.proposal_id,
        start_time: proposal.start_time,
    };

    let ack_data = to_binary(&Response::QueryProposal(proposal_snapshot))?;
    Ok(IbcReceiveResponse::new()
        .set_ack(ack_data)
        .add_attribute("query", "proposal")
        .add_attribute("proposal_id", id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use astroport_governance::interchain::Hub;
    use cosmwasm_std::{from_binary, testing::mock_info, Addr, IbcPacketReceiveMsg, Uint64};

    use crate::{
        contract::instantiate,
        execute::execute,
        ibc::ibc_packet_receive,
        mock::{
            mock_all, mock_ibc_packet, setup_channel, ASSEMBLY, CW20ICS20, GENERATOR_CONTROLLER,
            OWNER, STAKING,
        },
    };

    // Test Cases:
    //
    // Expect Success
    //      - Proposal should not be queried without Assembly

    #[test]
    fn query_proposal_fails_invalid_assembly() {
        let (mut deps, env, info) = mock_all(OWNER);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::hub::InstantiateMsg {
                owner: OWNER.to_string(),
                assembly_addr: "invalid".to_string(),
                cw20_ics20_addr: CW20ICS20.to_string(),
                staking_addr: STAKING.to_string(),
                generator_controller_addr: GENERATOR_CONTROLLER.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        // Set up valid IBC channel
        setup_channel(deps.as_mut(), env.clone());

        // Add allowed Outpost
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "outpost".to_string(),
                outpost_channel: "channel-3".to_string(),
                cw20_ics20_channel: "channel-1".to_string(),
            },
        )
        .unwrap();

        let ibc_query_proposal = to_binary(&Hub::QueryProposal { id: 1 }).unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_query_proposal);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env, msg).unwrap();

        let ack: Response = from_binary(&res.acknowledgement).unwrap();
        match ack {
            Response::Result { error, .. } => {
                assert!(error.is_some());
            }
            _ => panic!("Wrong response type"),
        }

        // No messages should be emitted
        assert_eq!(res.messages.len(), 0);
    }

    // Test Cases:
    //
    // Expect Success
    //      - An IBC ack contains the correct information

    #[test]
    fn query_proposal() {
        let (mut deps, env, info) = mock_all(OWNER);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport_governance::hub::InstantiateMsg {
                owner: OWNER.to_string(),
                assembly_addr: ASSEMBLY.to_string(),
                cw20_ics20_addr: CW20ICS20.to_string(),
                staking_addr: STAKING.to_string(),
                generator_controller_addr: GENERATOR_CONTROLLER.to_string(),
                ibc_timeout_seconds: 10,
            },
        )
        .unwrap();

        // Set up valid IBC channel
        setup_channel(deps.as_mut(), env.clone());

        // Add allowed Outpost
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(OWNER, &[]),
            astroport_governance::hub::ExecuteMsg::AddOutpost {
                outpost_addr: "outpost".to_string(),
                outpost_channel: "channel-3".to_string(),
                cw20_ics20_channel: "channel-1".to_string(),
            },
        )
        .unwrap();

        let ibc_query_proposal = to_binary(&Hub::QueryProposal { id: 1 }).unwrap();
        let recv_packet = mock_ibc_packet("channel-3", ibc_query_proposal);

        let msg = IbcPacketReceiveMsg::new(recv_packet, Addr::unchecked("relayer"));
        let res = ibc_packet_receive(deps.as_mut(), env, msg).unwrap();

        let ack: Response = from_binary(&res.acknowledgement).unwrap();
        match ack {
            Response::QueryProposal(proposal) => {
                assert_eq!(proposal.id, Uint64::from(1u64));
            }
            _ => panic!("Wrong response type"),
        }

        // No message must be emitted, the ack contains the data
        assert_eq!(res.messages.len(), 0);
    }
}
