use std::collections::HashMap;
use std::str::FromStr;

use astroport::{asset::AssetInfo, common::LP_SUBDENOM, incentives::RewardType};
use cosmwasm_std::{coin, coins, Decimal, Decimal256, Empty, Event, Uint128};
use cw_multi_test::Executor;
use cw_utils::PaymentError;
use itertools::Itertools;
use neutron_sdk::sudo::msg::{RequestPacket, TransferSudoMsg};

use astroport_emissions_controller::error::ContractError;
use astroport_emissions_controller::utils::get_epoch_start;
use astroport_governance::assembly::{ProposalVoteOption, ProposalVoterResponse};
use astroport_governance::emissions_controller::consts::{DAY, EPOCH_LENGTH};
use astroport_governance::emissions_controller::hub::{
    AstroPoolConfig, EmissionsState, HubMsg, OutpostInfo, OutpostParams, OutpostStatus, TuneInfo,
    UserInfoResponse,
};
use astroport_governance::emissions_controller::msg::{ExecuteMsg, VxAstroIbcMsg};
use astroport_governance::{assembly, emissions_controller};
use astroport_voting_escrow::state::UNLOCK_PERIOD;

use crate::common::helper::{ControllerHelper, PROPOSAL_VOTING_PERIOD};

mod common;

#[test]
pub fn voting_test() {
    let mut helper = ControllerHelper::new();

    let user = helper.app.api().addr_make("user");

    let err = helper
        .vote(&user, &[("lp_token".to_string(), Decimal::one())])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ZeroVotingPower {}
    );

    helper.lock(&user, 1000).unwrap();

    let lp_token1 = helper.create_pair("token1", "token2");
    let lp_token2 = helper.create_pair("token1", "token3");

    let neutron = OutpostInfo {
        astro_denom: helper.astro.clone(),
        params: None,
        astro_pool_config: None,
    };
    helper.add_outpost("neutron", neutron).unwrap();

    let whitelist_fee = helper.whitelisting_fee.clone();
    for pool in &[lp_token1.clone(), lp_token2.clone()] {
        helper.mint_tokens(&user, &[whitelist_fee.clone()]).unwrap();
        helper
            .whitelist(&user, pool, &[whitelist_fee.clone()])
            .unwrap();
    }

    let err = helper
        .vote(
            &user,
            &[
                (lp_token1.to_string(), Decimal::one()),
                (lp_token1.to_string(), Decimal::one()),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::DuplicatedVotes {}
    );

    let err = helper
        .vote(
            &user,
            &[
                (lp_token1.to_string(), Decimal::one()),
                (lp_token2.to_string(), Decimal::one()),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidTotalWeight {}
    );

    helper
        .vote(&user, &[(lp_token1.to_string(), Decimal::one())])
        .unwrap();

    let pool_vp = helper.query_pool_vp(lp_token1.as_str(), None).unwrap();
    let user_vp = helper.user_vp(&user, None).unwrap();
    assert_eq!(pool_vp, user_vp);

    let err = helper
        .vote(&user, &[(lp_token1.to_string(), Decimal::one())])
        .unwrap_err();

    helper.timetravel(1);
    let block_time = helper.app.block_info().time.seconds();
    let epoch_start = get_epoch_start(block_time);
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::VoteCooldown(epoch_start + EPOCH_LENGTH)
    );

    helper.timetravel(epoch_start + EPOCH_LENGTH);
    helper
        .vote(
            &user,
            &[
                (lp_token1.to_string(), Decimal::percent(50)),
                (lp_token2.to_string(), Decimal::percent(50)),
            ],
        )
        .unwrap();

    let old_pool_vp = helper
        .query_pool_vp(lp_token1.as_str(), Some(block_time))
        .unwrap();
    assert_eq!(old_pool_vp, pool_vp);

    // Check new voting power
    let pool1_vp = helper.query_pool_vp(lp_token1.as_str(), None).unwrap();
    let pool2_vp = helper.query_pool_vp(lp_token2.as_str(), None).unwrap();

    assert_eq!(pool1_vp.u128(), user_vp.u128() / 2);
    assert_eq!(pool1_vp, pool2_vp);
}

#[test]
fn test_whitelist() {
    let mut helper = ControllerHelper::new();
    let owner = helper.owner.clone();
    let whitelist_fee = helper.whitelisting_fee.clone();

    let lp_token = helper.create_pair("token1", "token2");

    let err = helper.whitelist(&owner, &lp_token, &[]).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PaymentError(PaymentError::NoFunds {})
    );

    helper
        .mint_tokens(&owner, &[whitelist_fee.clone()])
        .unwrap();
    let err = helper
        .whitelist(&owner, &lp_token, &coins(1, &whitelist_fee.denom))
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::IncorrectWhitelistFee(whitelist_fee.clone())
    );

    let err = helper
        .whitelist(&owner, &lp_token, &[whitelist_fee.clone()])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoOutpostForPool(lp_token.to_string())
    );

    let astro_pool = helper
        .create_pair(helper.astro.clone().as_str(), "uusd")
        .to_string();
    let neutron = OutpostInfo {
        astro_denom: helper.astro.clone(),
        params: None,
        astro_pool_config: Some(AstroPoolConfig {
            astro_pool: astro_pool.clone(),
            constant_emissions: Uint128::one(),
        }),
    };
    helper.add_outpost("neutron", neutron).unwrap();

    let err = helper
        .whitelist(&owner, &astro_pool, &[whitelist_fee.clone()])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::IsAstroPool {}
    );

    // Try to whitelist non-existent pool
    let err = helper
        .whitelist(
            &owner,
            "factory/neutron1invalidaddr/astroport/share",
            &[whitelist_fee.clone()],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Querier contract error: Generic error: Invalid input"
    ); // cosmwasm tried to query invalid 'neutron1invalidaddr' address

    helper
        .whitelist(&owner, &lp_token, &[whitelist_fee.clone()])
        .unwrap();

    let fee_receiver = helper.query_config().unwrap().fee_receiver;
    let fee_balance = helper
        .app
        .wrap()
        .query_balance(fee_receiver, &whitelist_fee.denom)
        .unwrap();
    assert_eq!(fee_balance, whitelist_fee);

    helper
        .mint_tokens(&owner, &[whitelist_fee.clone()])
        .unwrap();
    let err = helper
        .whitelist(&owner, &lp_token, &[whitelist_fee.clone()])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PoolAlreadyWhitelisted(lp_token.to_string())
    );

    let whitelist = helper.query_whitelist().unwrap();
    assert_eq!(whitelist, vec![lp_token.to_string()]);
}

#[test]
fn test_outpost_management() {
    let mut helper = ControllerHelper::new();

    let mut neutron = OutpostInfo {
        astro_denom: helper.astro.clone(),
        params: None,
        astro_pool_config: Some(AstroPoolConfig {
            astro_pool: "wasm1pool".to_string(),
            constant_emissions: Uint128::one(),
        }),
    };

    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("random"),
            helper.emission_controller.clone(),
            &ExecuteMsg::Custom(HubMsg::UpdateOutpost {
                prefix: "neutron".to_string(),
                astro_denom: neutron.astro_denom.clone(),
                outpost_params: neutron.params.clone(),
                astro_pool_config: neutron.astro_pool_config.clone(),
            }),
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    let err = helper.add_outpost("neutron", neutron.clone()).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidOutpostPrefix("wasm1pool".to_string())
    );

    neutron.astro_pool_config.as_mut().unwrap().astro_pool =
        helper.create_pair("token1", "token2").to_string();
    neutron.astro_denom = "aa".to_string();

    let err = helper.add_outpost("neutron", neutron.clone()).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Invalid denom length [3,128]: aa"
    );

    neutron.astro_denom = "osmo1addr".to_string();
    let err = helper.add_outpost("neutron", neutron.clone()).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidHubAstroDenom(helper.astro.clone())
    );

    neutron.astro_denom.clone_from(&helper.astro);
    neutron
        .astro_pool_config
        .as_mut()
        .unwrap()
        .constant_emissions = Uint128::zero();
    let err = helper.add_outpost("neutron", neutron.clone()).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::ZeroAstroEmissions {}
    );

    neutron
        .astro_pool_config
        .as_mut()
        .unwrap()
        .constant_emissions = Uint128::one();
    helper.add_outpost("neutron", neutron.clone()).unwrap();

    let mut osmosis = OutpostInfo {
        astro_denom: "uastro".to_string(),
        params: Some(OutpostParams {
            emissions_controller: "osmo1controller".to_string(),
            voting_channel: "channel-1".to_string(),
            ics20_channel: "channel-2".to_string(),
        }),
        astro_pool_config: None,
    };

    let err = helper.add_outpost("osmo", osmosis.clone()).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidOutpostAstroDenom {}
    );

    osmosis.astro_denom =
        "ibc/C4CFF46FD6DE35CA4CF4CE031E643C8FDC9BA4B99AE598E9B0ED98FE3A2319F9".to_string();
    osmosis.params.as_mut().unwrap().ics20_channel = "ch-2".to_string();

    let err = helper.add_outpost("osmo", osmosis.clone()).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidOutpostIcs20Channel {}
    );

    osmosis.params.as_mut().unwrap().ics20_channel = "channel-2".to_string();
    osmosis.params.as_mut().unwrap().voting_channel = "channel-200".to_string();

    let err = helper.add_outpost("osmo", osmosis.clone()).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: The contract does not have channel channel-200"
    );

    osmosis.params.as_mut().unwrap().voting_channel = "channel-1".to_string();
    osmosis.params.as_mut().unwrap().emissions_controller = "terra1controller".to_string();

    let err = helper.add_outpost("osmo", osmosis.clone()).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::InvalidOutpostPrefix("terra1controller".to_string())
    );

    osmosis.params.as_mut().unwrap().emissions_controller = "osmo1controller".to_string();
    helper.add_outpost("osmo", osmosis.clone()).unwrap();

    let outposts = helper
        .app
        .wrap()
        .query_wasm_smart::<Vec<(String, OutpostInfo)>>(
            helper.emission_controller.clone(),
            &emissions_controller::hub::QueryMsg::ListOutposts {},
        )
        .unwrap();
    assert_eq!(
        outposts,
        vec![
            ("neutron".to_string(), neutron),
            ("osmo".to_string(), osmosis.clone())
        ]
    );

    // Whitelist and vote for neutron pool before removing outpost
    let user = helper.app.api().addr_make("user");
    helper
        .mint_tokens(&user, &[helper.whitelisting_fee.clone()])
        .unwrap();
    let lp_token = helper.create_pair("token1", "token3");
    helper
        .whitelist(&user, &lp_token, &[helper.whitelisting_fee.clone()])
        .unwrap();
    helper.lock(&user, 1000).unwrap();
    helper
        .vote(&user, &[(lp_token.to_string(), Decimal::one())])
        .unwrap();

    // Whitelist astro pool on Osmosis before marking it as ASTRO pool with flat emissions
    let osmosis_astro_pool = format!("factory/osmo1pool/{}", LP_SUBDENOM);
    helper
        .mint_tokens(&user, &[helper.whitelisting_fee.clone()])
        .unwrap();
    helper
        .whitelist(
            &user,
            &osmosis_astro_pool,
            &[helper.whitelisting_fee.clone()],
        )
        .unwrap();

    // Confirm it has been included
    let whitelist = helper
        .query_whitelist()
        .unwrap()
        .into_iter()
        .sorted()
        .collect_vec();
    assert_eq!(
        whitelist,
        vec![lp_token.to_string(), osmosis_astro_pool.clone()]
    );

    // Mark 'osmosis_astro_pool' as ASTRO pool
    osmosis.astro_pool_config = Some(AstroPoolConfig {
        astro_pool: osmosis_astro_pool,
        constant_emissions: Uint128::from(100000u128),
    });
    helper.add_outpost("osmo", osmosis.clone()).unwrap();

    // Confirm it has been excluded from whitelist
    let whitelist = helper.query_whitelist().unwrap();
    assert_eq!(whitelist, vec![lp_token.to_string()]);

    // Remove neutron outpost
    let rand_user = helper.app.api().addr_make("random");
    let err = helper
        .app
        .execute_contract(
            rand_user,
            helper.emission_controller.clone(),
            &ExecuteMsg::Custom(HubMsg::RemoveOutpost {
                prefix: "neutron".to_string(),
            }),
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::Custom(HubMsg::RemoveOutpost {
                prefix: "neutron".to_string(),
            }),
            &[],
        )
        .unwrap();

    // Cant vote for neutron pools anymore
    let user = helper.app.api().addr_make("user2");
    helper.lock(&user, 1000).unwrap();
    let err = helper
        .vote(&user, &[(lp_token.to_string(), Decimal::one())])
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::PoolIsNotWhitelisted(lp_token.to_string())
    );

    // Ensure neutron pool was removed from votable pools
    let voted_pools = helper.query_pools_vp(None).unwrap();
    assert_eq!(voted_pools, vec![]);
}

#[test]
fn test_tune_only_hub() {
    let mut helper = ControllerHelper::new();
    let owner = helper.owner.clone();

    let epoch_start = get_epoch_start(helper.app.block_info().time.seconds());

    let err = helper.tune(&owner).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::TuneCooldown(epoch_start + EPOCH_LENGTH)
    );

    let lp_token1 = helper.create_pair("token1", "token2");
    let lp_token2 = helper.create_pair("token1", "token3");
    let astro_pool = helper
        .create_pair(helper.astro.clone().as_str(), "uusd")
        .to_string();

    let neutron = OutpostInfo {
        astro_denom: helper.astro.clone(),
        params: None,
        astro_pool_config: Some(AstroPoolConfig {
            astro_pool: astro_pool.clone(),
            constant_emissions: 1_000_000_000u128.into(),
        }),
    };
    helper.add_outpost("neutron", neutron.clone()).unwrap();

    let user = helper.app.api().addr_make("user");

    let whitelist_fee = helper.whitelisting_fee.clone();
    for pool in &[lp_token1.clone(), lp_token2.clone()] {
        helper.mint_tokens(&user, &[whitelist_fee.clone()]).unwrap();
        helper
            .whitelist(&user, pool, &[whitelist_fee.clone()])
            .unwrap();
    }

    helper.lock(&user, 1000).unwrap();

    helper
        .vote(
            &user,
            &[
                (lp_token1.to_string(), Decimal::percent(50)),
                (lp_token2.to_string(), Decimal::percent(50)),
            ],
        )
        .unwrap();

    helper.timetravel(EPOCH_LENGTH - 1);
    let err = helper.tune(&owner).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::TuneCooldown(epoch_start + EPOCH_LENGTH)
    );

    helper.timetravel(1);
    // Top up ASTRO for emissions
    helper
        .mint_tokens(
            &helper.emission_controller.clone(),
            &coins(50_000_000_000_000, helper.astro.clone()),
        )
        .unwrap();
    helper.tune(&owner).unwrap();

    let cur_emissions = helper.query_current_emissions().unwrap().emissions_amount;
    let expected_rps = Decimal256::from_ratio(cur_emissions.u128() / 2, EPOCH_LENGTH);
    let rewards = helper.query_rewards(&lp_token1).unwrap();
    let epoch_start = get_epoch_start(helper.app.block_info().time.seconds());
    let first_epoch_start = epoch_start;
    assert_eq!(rewards.len(), 1);
    assert_eq!(rewards[0].rps, expected_rps);
    assert_eq!(
        rewards[0].reward,
        RewardType::Ext {
            info: AssetInfo::native(&helper.astro),
            next_update_ts: epoch_start + EPOCH_LENGTH
        }
    );
    // Check astro pool
    let rewards = helper.query_rewards(&astro_pool).unwrap();
    let expected_rps = Decimal256::from_ratio(
        neutron
            .astro_pool_config
            .as_ref()
            .unwrap()
            .constant_emissions,
        EPOCH_LENGTH,
    );
    assert_eq!(rewards.len(), 1);
    assert_eq!(rewards[0].rps, expected_rps);
    assert_eq!(
        rewards[0].reward,
        RewardType::Ext {
            info: AssetInfo::native(&helper.astro),
            next_update_ts: epoch_start + EPOCH_LENGTH
        }
    );

    let epoch_start = get_epoch_start(helper.app.block_info().time.seconds());
    let err = helper.tune(&owner).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::TuneCooldown(epoch_start + EPOCH_LENGTH)
    );

    // Imagine bot executed the tune late
    helper.timetravel(EPOCH_LENGTH + 3 * DAY);

    // Mocking received ASTRO in staking
    helper
        .mint_tokens(
            &helper.staking.clone(),
            &coins(500_000_000_000, helper.astro.clone()),
        )
        .unwrap();

    let sim_tune_result = helper.query_simulate_tune().unwrap();

    helper.tune(&owner).unwrap();

    let actual_emissions_state = helper.query_current_emissions().unwrap();
    assert_eq!(sim_tune_result.new_emissions_state, actual_emissions_state);
    let actual_tune_info = helper.query_tune_info(None).unwrap();
    assert_eq!(
        sim_tune_result.next_pools_grouped,
        actual_tune_info.pools_grouped,
    );

    // Reset incentives as nobody claimed rewards
    helper.reset_astro_reward(&lp_token1).unwrap();

    // User didn't change his votes. Emissions were 3 days late, thus their duration is 11 days.
    let cur_emissions = helper.query_current_emissions().unwrap().emissions_amount;
    let expected_rps = Decimal256::from_ratio(cur_emissions.u128() / 2, EPOCH_LENGTH - 3 * DAY);
    let rewards = helper.query_rewards(&lp_token1).unwrap();
    let epoch_start = get_epoch_start(helper.app.block_info().time.seconds());
    assert_eq!(rewards.len(), 1);
    assert_eq!(rewards[0].rps, expected_rps);
    assert_eq!(
        rewards[0].reward,
        RewardType::Ext {
            info: AssetInfo::native(&helper.astro),
            next_update_ts: epoch_start + EPOCH_LENGTH
        }
    );
    // Check astro pool
    let rewards = helper.query_rewards(&astro_pool).unwrap();
    let expected_rps = Decimal256::from_ratio(
        neutron
            .astro_pool_config
            .as_ref()
            .unwrap()
            .constant_emissions,
        EPOCH_LENGTH - 3 * DAY,
    );
    assert_eq!(rewards.len(), 1);
    assert_eq!(rewards[0].rps, expected_rps);
    assert_eq!(
        rewards[0].reward,
        RewardType::Ext {
            info: AssetInfo::native(&helper.astro),
            next_update_ts: epoch_start + EPOCH_LENGTH
        }
    );

    let mut tune_info = helper.query_tune_info(None).unwrap();
    tune_info
        .pools_grouped
        .iter_mut()
        .for_each(|(_, pools)| pools.sort());
    let expected_tune_info = TuneInfo {
        tune_ts: epoch_start,
        pools_grouped: HashMap::from([(
            "neutron".to_string(),
            vec![
                (lp_token1.to_string(), Uint128::new(146666666665)),
                (lp_token2.to_string(), Uint128::new(146666666665)),
                (astro_pool.to_string(), Uint128::new(1000000000)),
            ]
            .into_iter()
            .sorted()
            .collect(),
        )]),
        outpost_emissions_statuses: Default::default(),
        emissions_state: EmissionsState {
            xastro_rate: Decimal::from_str("499501.4995004995004995").unwrap(),
            collected_astro: 499999999999u128.into(),
            ema: 366666666664u128.into(),
            emissions_amount: 293333333331u128.into(),
        },
    };
    assert_eq!(tune_info, expected_tune_info);

    // Check historical tune info
    let mut tune_info = helper.query_tune_info(Some(first_epoch_start + 1)).unwrap();
    tune_info
        .pools_grouped
        .iter_mut()
        .for_each(|(_, pools)| pools.sort());
    let expected_tune_info = TuneInfo {
        tune_ts: first_epoch_start,
        pools_grouped: HashMap::from([(
            "neutron".to_string(),
            vec![
                (lp_token1.to_string(), Uint128::new(133600000000)),
                (lp_token2.to_string(), Uint128::new(133600000000)),
                (astro_pool.to_string(), Uint128::new(1000000000)),
            ]
            .into_iter()
            .sorted()
            .collect(),
        )]),
        outpost_emissions_statuses: Default::default(),
        emissions_state: EmissionsState {
            xastro_rate: Decimal::one(),
            collected_astro: 0u128.into(),
            ema: 99999999999u128.into(),
            emissions_amount: 267200000000u128.into(),
        },
    };
    assert_eq!(tune_info, expected_tune_info);
}

#[test]
fn test_tune_outpost() {
    let mut helper = ControllerHelper::new();
    let owner = helper.owner.clone();

    let lp_token1 = "factory/osmo1pool1/astroport/share";
    let lp_token2 = "factory/osmo1pool2/astroport/share";
    let astro_pool = "factory/osmo1astropool/astroport/share";

    let osmosis = OutpostInfo {
        astro_denom: "ibc/6569E05DEE32B339D9286A52BE33DFCEFC97267F23EF9CFDE0C055140967A9A5"
            .to_string(),
        params: Some(OutpostParams {
            emissions_controller: "osmo1emissionscontroller".to_string(),
            voting_channel: "channel-1".to_string(),
            ics20_channel: "channel-2".to_string(),
        }),
        astro_pool_config: Some(AstroPoolConfig {
            astro_pool: astro_pool.to_string(),
            constant_emissions: 1_000_000_000u128.into(),
        }),
    };
    helper.add_outpost("osmo", osmosis.clone()).unwrap();

    let whitelist_fee = helper.whitelisting_fee.clone();
    for pool in [lp_token1, lp_token2] {
        helper
            .mint_tokens(&owner, &[whitelist_fee.clone()])
            .unwrap();
        helper
            .whitelist(&owner, pool, &[whitelist_fee.clone()])
            .unwrap();
    }

    let user = helper.app.api().addr_make("user");
    helper.lock(&user, 1000).unwrap();

    helper
        .vote(
            &user,
            &[
                (lp_token1.to_string(), Decimal::percent(50)),
                (lp_token2.to_string(), Decimal::percent(50)),
            ],
        )
        .unwrap();

    helper
        .mint_tokens(
            &helper.emission_controller.clone(),
            &coins(50_000_000_000_000, helper.astro.clone()),
        )
        .unwrap();

    helper.timetravel(EPOCH_LENGTH);
    helper.tune(&owner).unwrap();

    let epoch_start = get_epoch_start(helper.app.block_info().time.seconds());
    let mut tune_info = helper.query_tune_info(None).unwrap();
    tune_info
        .pools_grouped
        .iter_mut()
        .for_each(|(_, pools)| pools.sort());
    let expected_tune_info = TuneInfo {
        tune_ts: epoch_start,
        pools_grouped: HashMap::from([(
            "osmo".to_string(),
            vec![
                (lp_token1.to_string(), Uint128::new(133600000000)),
                (lp_token2.to_string(), Uint128::new(133600000000)),
                (astro_pool.to_string(), Uint128::new(1000000000)),
            ]
            .into_iter()
            .sorted()
            .collect(),
        )]),
        outpost_emissions_statuses: HashMap::from([(
            "osmo".to_string(),
            OutpostStatus::InProgress,
        )]),
        emissions_state: EmissionsState {
            xastro_rate: Decimal::one(),
            collected_astro: 0u128.into(),
            ema: 99999999999u128.into(),
            emissions_amount: 267200000000u128.into(),
        },
    };
    assert_eq!(tune_info, expected_tune_info);

    // Try to retry outposts which are being in progress
    let err = helper.retry_failed_outposts(&owner).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoFailedOutpostsToRetry {}
    );

    // Mock ics20 IBC timeout
    helper
        .app
        .wasm_sudo(
            helper.emission_controller.clone(),
            &TransferSudoMsg::Timeout {
                request: RequestPacket {
                    sequence: None,
                    source_port: None,
                    source_channel: Some("channel-2".to_string()),
                    destination_port: None,
                    destination_channel: None,
                    data: None,
                    timeout_height: None,
                    timeout_timestamp: None,
                },
            },
        )
        .unwrap();

    // Try to mock ics20 message failure right after timeout even tho this must be impossible
    let err = helper
        .app
        .wasm_sudo(
            helper.emission_controller.clone(),
            &TransferSudoMsg::Error {
                request: RequestPacket {
                    sequence: None,
                    source_port: None,
                    source_channel: Some("channel-2".to_string()),
                    destination_port: None,
                    destination_channel: None,
                    data: None,
                    timeout_height: None,
                    timeout_timestamp: None,
                },
                details: "".to_string(),
            },
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Outpost osmo is not in progress"
    );

    // Retry failed outposts and mock IBC acknowledgment packet
    helper.retry_failed_outposts(&owner).unwrap();
    helper
        .app
        .wasm_sudo(
            helper.emission_controller.clone(),
            &TransferSudoMsg::Response {
                request: RequestPacket {
                    sequence: None,
                    source_port: None,
                    source_channel: Some("channel-2".to_string()),
                    destination_port: None,
                    destination_channel: None,
                    data: None,
                    timeout_height: None,
                    timeout_timestamp: None,
                },
                data: Default::default(),
            },
        )
        .unwrap();

    helper.timetravel(10000);

    let mut tune_info = helper.query_tune_info(None).unwrap();
    tune_info
        .pools_grouped
        .iter_mut()
        .for_each(|(_, pools)| pools.sort());
    let expected_tune_info = TuneInfo {
        tune_ts: epoch_start,
        pools_grouped: HashMap::from([(
            "osmo".to_string(),
            vec![
                (lp_token1.to_string(), Uint128::new(133600000000)),
                (lp_token2.to_string(), Uint128::new(133600000000)),
                (astro_pool.to_string(), Uint128::new(1000000000)),
            ]
            .into_iter()
            .sorted()
            .collect(),
        )]),
        outpost_emissions_statuses: HashMap::from([("osmo".to_string(), OutpostStatus::Done)]),
        emissions_state: EmissionsState {
            xastro_rate: Decimal::one(),
            collected_astro: 0u128.into(),
            ema: 99999999999u128.into(),
            emissions_amount: 267200000000u128.into(),
        },
    };
    assert_eq!(tune_info, expected_tune_info);

    // Confirm there is no outposts to retry
    let err = helper.retry_failed_outposts(&owner).unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::NoFailedOutpostsToRetry {}
    );
}

#[test]
fn test_lock_unlock_vxastro() {
    let mut helper = ControllerHelper::new();

    // Ensure nobody but vxASTRO can call UpdateUserVotes endpoint
    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("random"),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::UpdateUserVotes {
                user: "user".to_string(),
                is_unlock: false,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );
    helper
        .app
        .execute_contract(
            helper.vxastro.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::UpdateUserVotes {
                user: helper.app.api().addr_make("random").to_string(),
                is_unlock: false,
            },
            &[],
        )
        .unwrap();

    let owner = helper.owner.clone();
    helper
        .mint_tokens(&owner, &[coin(1000_000000, helper.astro.clone())])
        .unwrap();
    let whitelisting_fee = helper.whitelisting_fee.clone();

    helper
        .add_outpost(
            "neutron",
            OutpostInfo {
                astro_denom: helper.astro.clone(),
                params: None,
                astro_pool_config: None,
            },
        )
        .unwrap();

    let pool1 = helper.create_pair("token1", "token2");
    helper
        .whitelist(&owner, &pool1, &[whitelisting_fee.clone()])
        .unwrap();
    let pool2 = helper.create_pair("token1", "token3");
    helper
        .whitelist(&owner, &pool2, &[whitelisting_fee.clone()])
        .unwrap();

    let alice = helper.app.api().addr_make("alice");
    helper.lock(&alice, 1_000000).unwrap();

    let bob = helper.app.api().addr_make("bob");
    helper.lock(&bob, 1_000000).unwrap();

    let voting_block_ts = helper.app.block_info().time.seconds();
    // Alice and Bob vote 50:50 for two existing pools
    for user in [&alice, &bob] {
        helper
            .vote(
                user,
                &[
                    (pool1.to_string(), Decimal::percent(50)),
                    (pool2.to_string(), Decimal::percent(50)),
                ],
            )
            .unwrap();

        let user_info = helper.user_info(user, None).unwrap();
        let user_info_historical = helper.user_info(user, Some(voting_block_ts)).unwrap();

        assert_eq!(user_info, user_info_historical);
        assert_eq!(
            user_info,
            UserInfoResponse {
                vote_ts: voting_block_ts,
                voting_power: 1_000000u128.into(),
                votes: HashMap::from([
                    (pool1.to_string(), Decimal::percent(50)),
                    (pool2.to_string(), Decimal::percent(50)),
                ]),
                applied_votes: HashMap::from([
                    (pool1.to_string(), Decimal::percent(50)),
                    (pool2.to_string(), Decimal::percent(50)),
                ])
            }
        );
    }

    // Assert pools voting power
    for pool in [&pool1, &pool2] {
        let pool_vp = helper.query_pool_vp(pool.as_str(), None).unwrap();
        assert_eq!(pool_vp.u128(), 1_000000);
    }

    helper.timetravel(3 * DAY);

    // Alice locks more astro
    helper.lock(&alice, 1_000000).unwrap();

    // Ensure pool voting power is updated
    for pool in [&pool1, &pool2] {
        let pool_vp = helper.query_pool_vp(pool.as_str(), None).unwrap();
        assert_eq!(pool_vp.u128(), 1_500000);
    }

    // Bob starts unlocking
    helper.unlock(&bob).unwrap();

    // Ensure pool voting power is updated
    for pool in [&pool1, &pool2] {
        let pool_vp = helper.query_pool_vp(pool.as_str(), None).unwrap();
        assert_eq!(pool_vp.u128(), 1_000000);
    }

    helper.timetravel(2 * DAY);

    // Bob relocks
    helper.relock(&bob).unwrap();

    // Ensure pool voting power is updated
    for pool in [&pool1, &pool2] {
        let pool_vp = helper.query_pool_vp(pool.as_str(), None).unwrap();
        assert_eq!(pool_vp.u128(), 1_500000);
    }

    // Check historical queries
    for user in [&alice, &bob] {
        // Contract state is finalized at the end of the voting block,
        // thus we are querying the next block
        let user_info = helper.user_info(user, Some(voting_block_ts + 1)).unwrap();

        assert_eq!(
            user_info,
            UserInfoResponse {
                vote_ts: voting_block_ts,
                voting_power: 1_000000u128.into(),
                votes: HashMap::from([
                    (pool1.to_string(), Decimal::percent(50)),
                    (pool2.to_string(), Decimal::percent(50)),
                ]),
                applied_votes: HashMap::from([
                    (pool1.to_string(), Decimal::percent(50)),
                    (pool2.to_string(), Decimal::percent(50)),
                ])
            }
        );
    }

    let voted_pools = helper.query_pools_vp(Some(5)).unwrap();
    let mut expected_pools = vec![
        (pool1.to_string(), 1_500000u128.into()),
        (pool2.to_string(), 1_500000u128.into()),
    ];
    expected_pools.sort();
    assert_eq!(voted_pools, expected_pools);

    // Unlock and withdraw
    helper.unlock(&alice).unwrap();
    helper.unlock(&bob).unwrap();

    helper.timetravel(UNLOCK_PERIOD);

    helper.withdraw(&alice).unwrap();
    helper.withdraw(&bob).unwrap();

    let alice_balance = helper
        .app
        .wrap()
        .query_balance(alice, &helper.xastro)
        .unwrap();
    let bob_balance = helper
        .app
        .wrap()
        .query_balance(bob, &helper.xastro)
        .unwrap();
    assert_eq!(alice_balance, coin(2_000000, &helper.xastro));
    assert_eq!(bob_balance, coin(1_000000, &helper.xastro));
}

#[test]
fn test_some_epochs() {
    let mut helper = ControllerHelper::new();
    let owner = helper.owner.clone();
    let whitelisting_fee = helper.whitelisting_fee.clone();

    helper
        .add_outpost(
            "osmo",
            OutpostInfo {
                astro_denom: "ibc/C4CFF46FD6DE35CA4CF4CE031E643C8FDC9BA4B99AE598E9B0ED98FE3A2319F9"
                    .to_string(),
                params: Some(OutpostParams {
                    emissions_controller: "osmo1controller".to_string(),
                    voting_channel: "channel-1".to_string(),
                    ics20_channel: "channel-2".to_string(),
                }),
                astro_pool_config: None,
            },
        )
        .unwrap();
    let pool1 = "osmo1pool1";
    let pool2 = "osmo1pool2";
    helper
        .mint_tokens(&owner, &coins(100000000, helper.astro.clone()))
        .unwrap();
    helper
        .whitelist(&owner, pool1, &[whitelisting_fee.clone()])
        .unwrap();
    helper
        .whitelist(&owner, pool2, &[whitelisting_fee.clone()])
        .unwrap();

    let user1 = helper.app.api().addr_make("user1");
    helper.lock(&user1, 1_000000).unwrap();
    let user2 = helper.app.api().addr_make("user2");
    helper.lock(&user2, 1_000000).unwrap();

    helper
        .vote(&user1, &[(pool1.to_string(), Decimal::one())])
        .unwrap();
    helper
        .vote(&user2, &[(pool2.to_string(), Decimal::one())])
        .unwrap();

    // Preparing controller balance for tuning
    helper
        .mint_tokens(
            &helper.emission_controller.clone(),
            &coins(100_000_000_000_000, helper.astro.clone()),
        )
        .unwrap();

    helper.timetravel(EPOCH_LENGTH);

    helper.tune(&user1).unwrap();
    let voted_pools = helper.query_pools_vp(None).unwrap();
    assert_eq!(
        voted_pools,
        [
            (pool1.to_string(), 1_000000u128.into()),
            (pool2.to_string(), 1_000000u128.into()),
        ]
    );

    helper.unlock(&user1).unwrap();

    helper.timetravel(EPOCH_LENGTH);
    helper.tune(&user1).unwrap();

    // User1 unlocked, user2 still keeps his votes
    let voted_pools = helper.query_pools_vp(None).unwrap();
    let expected_pools = vec![
        (pool1.to_string(), 0u128.into()),
        (pool2.to_string(), 1_000000u128.into()),
    ];
    assert_eq!(voted_pools, expected_pools);

    helper.relock(&user1).unwrap();
    helper.unlock(&user2).unwrap();

    helper.timetravel(EPOCH_LENGTH);
    helper.tune(&user1).unwrap();

    // User1 relocked, user2 unlocked
    let voted_pools = helper.query_pools_vp(None).unwrap();
    assert_eq!(
        voted_pools,
        [
            (pool1.to_string(), 1_000000u128.into()),
            (pool2.to_string(), 0u128.into()),
        ]
    );

    // Allow only 1 pool for tuning
    helper
        .app
        .execute_contract(
            owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::Custom(HubMsg::UpdateConfig {
                pools_per_outpost: Some(1),
                whitelisting_fee: None,
                fee_receiver: None,
                emissions_multiple: None,
                max_astro: None,
            }),
            &[],
        )
        .unwrap();

    helper.timetravel(EPOCH_LENGTH);
    helper.tune(&user1).unwrap();

    // pool2 was removed from votable pools
    let voted_pools = helper.query_pools_vp(None).unwrap();
    assert_eq!(voted_pools, [(pool1.to_string(), 1_000000u128.into())]);

    // And from whitelist
    let whitelist = helper.query_whitelist().unwrap();
    assert_eq!(whitelist, vec![pool1.to_string()]);

    // If user2 relocks his votes won't be restored as pool2 must be whitelisted again
    helper.relock(&user2).unwrap();
    let voted_pools = helper.query_pools_vp(None).unwrap();
    assert_eq!(voted_pools, [(pool1.to_string(), 1_000000u128.into())]);

    // Whitelist pool2 again
    helper
        .whitelist(&owner, pool2, &[whitelisting_fee.clone()])
        .unwrap();

    // Ensure that user2 votes are not applied
    let user2_info = helper.user_info(&user2, None).unwrap();
    assert_eq!(user2_info.applied_votes, HashMap::new());

    // User2 must refresh his votes
    helper.refresh_user_votes(&user2).unwrap();

    // His contribution must be restored
    let voted_pools = helper.query_pools_vp(None).unwrap();
    assert_eq!(
        voted_pools,
        [
            (pool1.to_string(), 1_000000u128.into()),
            (pool2.to_string(), 1_000000u128.into()),
        ]
    );
}

#[test]
fn test_interchain_governance() {
    let mut helper = ControllerHelper::new();
    let owner = helper.owner.clone();

    // No proposal yet
    let err = helper.register_proposal(1).unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Querier contract error: type: astroport_governance::assembly::Proposal; key: [00, 09, 70, 72, 6F, 70, 6F, 73, 61, 6C, 73, 00, 00, 00, 00, 00, 00, 00, 01] not found");

    helper.submit_proposal(&owner).unwrap();

    // No outposts yet but it shouldn't fail transaction
    let resp = helper.register_proposal(1).unwrap();
    assert!(
        !resp.has_event(
            &Event::new("wasm")
                .add_attributes([("action", "register_proposal"), ("outpost", "osmo")])
        ),
        "Controller tried to register outpost {:?}",
        resp.events
    );

    // Add outpost
    helper
        .add_outpost(
            "osmo",
            OutpostInfo {
                astro_denom: "ibc/C4CFF46FD6DE35CA4CF4CE031E643C8FDC9BA4B99AE598E9B0ED98FE3A2319F9"
                    .to_string(),
                params: Some(OutpostParams {
                    emissions_controller: "osmo1controller".to_string(),
                    voting_channel: "channel-1".to_string(),
                    ics20_channel: "channel-2".to_string(),
                }),
                astro_pool_config: None,
            },
        )
        .unwrap();

    // Submit 2nd proposal. Ensure it registers a proposal on osmosis
    let resp = helper.submit_proposal(&owner).unwrap();
    resp.assert_event(
        &Event::new("wasm").add_attributes([("action", "register_proposal"), ("outpost", "osmo")]),
    );

    // Now we can register 1st proposal
    helper.register_proposal(1).unwrap();
    resp.assert_event(
        &Event::new("wasm").add_attributes([("action", "register_proposal"), ("outpost", "osmo")]),
    );

    // Timeout both proposals
    helper.blocktravel(PROPOSAL_VOTING_PERIOD + 1);

    let err = helper.register_proposal(1).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Proposal is not active"
    );

    // Emulate outpost vote after a voting period is over.
    // It shouldn't fail but must not register a vote
    let resp = helper
        .mock_packet_receive(VxAstroIbcMsg::GovernanceVote {
            voter: "osmo1voter".to_string(),
            voting_power: Default::default(),
            proposal_id: 1,
            vote: ProposalVoteOption::For,
        })
        .unwrap();
    resp.assert_event(
        &Event::new("wasm")
            .add_attributes([("action", "cast_vote"), ("error", "Voting period ended!")]),
    );

    // Submit 3rd proposal
    helper.submit_proposal(&owner).unwrap();

    // Emulate vote from osmosis
    let resp = helper
        .mock_packet_receive(VxAstroIbcMsg::GovernanceVote {
            voter: "osmo1voter".to_string(),
            voting_power: 1_000000u128.into(),
            proposal_id: 3,
            vote: ProposalVoteOption::For,
        })
        .unwrap();

    resp.assert_event(&Event::new("wasm").add_attributes([
        ("action", "cast_vote"),
        ("proposal_id", "3"),
        ("voter", "osmo1voter"),
        ("vote", "for"),
        ("voting_power", "1000000"),
    ]));

    // Ensure voter has been reflected in assembly
    let proposal = helper
        .app
        .wrap()
        .query_wasm_smart::<assembly::Proposal>(
            helper.assembly.clone(),
            &assembly::QueryMsg::Proposal { proposal_id: 3 },
        )
        .unwrap();
    assert_eq!(proposal.for_power.u128(), 1_000000);

    let voters = helper
        .app
        .wrap()
        .query_wasm_smart::<Vec<ProposalVoterResponse>>(
            helper.assembly.clone(),
            &assembly::QueryMsg::ProposalVoters {
                proposal_id: 3,
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(
        voters,
        vec![ProposalVoterResponse {
            address: "osmo1voter".to_string(),
            vote_option: ProposalVoteOption::For,
        }]
    );
}

#[test]
fn test_change_ownership() {
    let mut helper = ControllerHelper::new();

    let new_owner = helper.app.api().addr_make("new_owner");

    // New owner
    let msg = ExecuteMsg::<Empty>::ProposeNewOwner {
        new_owner: new_owner.to_string(),
        expires_in: 100, // seconds
    };

    // Unauthorized check
    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("not_owner"),
            helper.emission_controller.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose a new owner
    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.emission_controller.clone(),
            &msg,
            &[],
        )
        .unwrap();

    // Claim from invalid addr
    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("invalid_addr"),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop the ownership proposal
    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::DropOwnershipProposal {},
            &[],
        )
        .unwrap();

    // Claim ownership
    let err = helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose a new owner again
    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.emission_controller.clone(),
            &msg,
            &[],
        )
        .unwrap();
    helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.emission_controller.clone(),
            &ExecuteMsg::<Empty>::ClaimOwnership {},
            &[],
        )
        .unwrap();

    assert_eq!(helper.query_config().unwrap().owner.to_string(), new_owner)
}

#[test]
fn test_update_config() {
    let mut helper = ControllerHelper::new();

    let fee_receiver = helper.app.api().addr_make("fee_receiver");
    let msg = ExecuteMsg::Custom(HubMsg::UpdateConfig {
        pools_per_outpost: Some(8),
        whitelisting_fee: Some(coin(100, "astro")),
        fee_receiver: Some(fee_receiver.to_string()),
        emissions_multiple: Some(Decimal::percent(90)),
        max_astro: Some(1_000_000u128.into()),
    });

    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("random"),
            helper.emission_controller.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::Unauthorized {}
    );

    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.emission_controller.clone(),
            &msg,
            &[],
        )
        .unwrap();

    let config = helper.query_config().unwrap();

    assert_eq!(
        config,
        emissions_controller::hub::Config {
            owner: helper.owner.clone(),
            assembly: helper.assembly.clone(),
            vxastro: helper.vxastro.clone(),
            factory: helper.factory.clone(),
            astro_denom: helper.astro.clone(),
            xastro_denom: helper.xastro.clone(),
            staking: helper.staking.clone(),
            incentives_addr: helper.incentives.clone(),
            pools_per_outpost: 8,
            whitelisting_fee: coin(100, "astro"),
            fee_receiver,
            whitelist_threshold: Decimal::percent(1),
            emissions_multiple: Decimal::percent(90),
            max_astro: 1_000_000u128.into(),
        }
    );
}
