use std::marker::PhantomData;
use std::str::FromStr;

use astroport::tokenfactory_tracker;
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{coin, coins, to_json_binary, ContractResult, SystemResult, Uint128};
use cosmwasm_std::{
    from_json, Addr, Coin, Decimal, Empty, OwnedDeps, QuerierResult, Uint64, WasmQuery,
};
use test_case::test_case;

use astroport_governance::assembly::{
    Config, Proposal, ProposalStatus, QueryMsg, DELAY_INTERVAL, DEPOSIT_INTERVAL,
    EXPIRATION_PERIOD_INTERVAL, MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE,
    MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE, VOTING_PERIOD_INTERVAL,
};

use crate::contract::submit_proposal;
use crate::queries::query;
use crate::state::{CONFIG, PROPOSAL_COUNT};

const PROPOSAL_REQUIRED_DEPOSIT: u128 = *DEPOSIT_INTERVAL.start();
const XASTRO_DENOM: &str = "xastro";

// Mocked wasm queries handler
fn custom_wasm_handler(request: &WasmQuery) -> QuerierResult {
    match request {
        WasmQuery::Smart { msg, .. } => {
            if matches!(
                from_json(msg),
                Ok(tokenfactory_tracker::QueryMsg::TotalSupplyAt { .. })
            ) {
                SystemResult::Ok(ContractResult::Ok(
                    to_json_binary(&Uint128::zero()).unwrap(),
                ))
            } else if matches!(
                from_json(msg),
                Ok(astroport_governance::builder_unlock::msg::QueryMsg::State {})
            ) {
                SystemResult::Ok(ContractResult::Ok(
                    to_json_binary(&astroport_governance::builder_unlock::msg::StateResponse {
                        total_astro_deposited: Default::default(),
                        remaining_astro_tokens: Default::default(),
                        unallocated_astro_tokens: Default::default(),
                    })
                    .unwrap(),
                ))
            } else {
                unimplemented!()
            }
        }
        _ => unimplemented!(),
    }
}
fn mock_deps() -> OwnedDeps<MockStorage, MockApi, MockQuerier, Empty> {
    let mut querier = MockQuerier::new(&[]);
    querier.update_wasm(custom_wasm_handler);

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier,
        custom_query_type: PhantomData,
    }
}

#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), "title", "description", None, None ; "valid proposal")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), "X", "description", None, Some("Generic error: Title too short!") ; "short title")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), "title", "description", Some("X"), Some("Generic error: Link too short!") ; "short link")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), "title", "description", Some("https://some1.link"), Some("Generic error: Link is not whitelisted!") ; "link is not whitelisted")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), "title", "description", Some("https://some.link/<script>alert('test');</script>"), Some("Generic error: Link is not properly formatted or contains unsafe characters!") ; "malicious link")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), "title", "description", Some(&String::from_utf8(vec![b'X'; 129]).unwrap()), Some("Generic error: Link too long!") ; "long link")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), "title", "X", None, Some("Generic error: Description too short!") ; "short description")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), &String::from_utf8(vec![b'X'; 65]).unwrap(), "description", None, Some("Generic error: Title too long!") ; "long title")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), "title", &String::from_utf8(vec![b'X'; 1025]).unwrap(), None, Some("Generic error: Description too long!") ; "long description")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT - 1, XASTRO_DENOM), "title", "description", None, Some("Insufficient token deposit!") ; "invalid deposit")]
#[test_case(coins(PROPOSAL_REQUIRED_DEPOSIT, "random"), "title", "description", None, Some("Must send reserve token 'xastro'") ; "invalid coin deposit")]
#[test_case(vec![coin(PROPOSAL_REQUIRED_DEPOSIT, XASTRO_DENOM), coin(PROPOSAL_REQUIRED_DEPOSIT, "random")], "title", "description", None, Some("Sent more than one denomination") ; "additional invalid coin deposit")]
fn check_proposal_validation(
    funds: Vec<Coin>,
    title: &str,
    description: &str,
    link: Option<&str>,
    expected_error: Option<&str>,
) {
    // Linter is not able to properly parse test_case macro; keep these lines
    let _ = coins(0, "keep_it");
    let _ = coin(0, "keep_it");

    let mut deps = mock_deps();
    let env = mock_env();

    // Mocked instantiation
    PROPOSAL_COUNT
        .save(deps.as_mut().storage, &Uint64::zero())
        .unwrap();
    let config = Config {
        xastro_denom: XASTRO_DENOM.to_string(),
        xastro_denom_tracking: "".to_string(),
        vxastro_token_addr: None,
        voting_escrow_delegator_addr: None,
        ibc_controller: None,
        generator_controller: None,
        hub: None,
        builder_unlock_addr: Addr::unchecked(""),
        proposal_voting_period: *VOTING_PERIOD_INTERVAL.start(),
        proposal_effective_delay: *DELAY_INTERVAL.start(),
        proposal_expiration_period: *EXPIRATION_PERIOD_INTERVAL.start(),
        proposal_required_deposit: PROPOSAL_REQUIRED_DEPOSIT.into(),
        proposal_required_quorum: Decimal::from_str(MINIMUM_PROPOSAL_REQUIRED_QUORUM_PERCENTAGE)
            .unwrap(),
        proposal_required_threshold: Decimal::from_atomics(
            MINIMUM_PROPOSAL_REQUIRED_THRESHOLD_PERCENTAGE,
            2,
        )
        .unwrap(),
        whitelisted_links: vec!["https://some.link/".to_string()],
        guardian_addr: None,
    };
    CONFIG.save(deps.as_mut().storage, &config).unwrap();

    let result = submit_proposal(
        deps.as_mut(),
        mock_env(),
        mock_info("creator", &funds),
        title.to_string(),
        description.to_string(),
        link.map(|s| s.to_string()),
        vec![],
        None,
    );

    if let Some(err_msg) = expected_error {
        assert_eq!(err_msg, result.unwrap_err().to_string())
    } else {
        result.unwrap();

        let bin_resp = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::Proposal { proposal_id: 1 },
        )
        .unwrap();
        let proposal: Proposal = from_json(&bin_resp).unwrap();

        assert_eq!(
            proposal,
            Proposal {
                proposal_id: 1u64.into(),
                submitter: Addr::unchecked("creator"),
                status: ProposalStatus::Active,
                for_power: Default::default(),
                outpost_for_power: Default::default(),
                against_power: Default::default(),
                outpost_against_power: Default::default(),
                start_block: env.block.height,
                start_time: env.block.time.seconds(),
                end_block: env.block.height + config.proposal_voting_period,
                delayed_end_block: env.block.height
                    + config.proposal_voting_period
                    + config.proposal_effective_delay,
                expiration_block: env.block.height
                    + config.proposal_voting_period
                    + config.proposal_effective_delay
                    + config.proposal_expiration_period,
                title: title.to_string(),
                description: description.to_string(),
                link: link.map(|s| s.to_string()),
                messages: vec![],
                deposit_amount: funds[0].amount,
                ibc_channel: None,
                total_voting_power: Default::default(),
            }
        );
    }
}
