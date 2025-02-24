use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use cosmwasm_std::{coin, coins, Addr, Decimal, Uint128};
use cw20::Cw20Coin;
use cw_multi_test::Executor;
use itertools::Itertools;

use crate::common::contracts::token_contract;
use astroport_governance::emissions_controller::consts::EPOCH_LENGTH;
use astroport_governance::tributes::{
    ExecuteMsg, QueryMsg, TributeFeeInfo, REWARDS_AMOUNT_LIMITS, TOKEN_TRANSFER_GAS_LIMIT,
};
use astroport_tributes::error::ContractError;

use crate::common::helper::Helper;

mod common;

#[test]
fn test_change_ownership() {
    let mut helper = Helper::new();

    let new_owner = helper.app.api().addr_make("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        new_owner: new_owner.to_string(),
        expires_in: 100, // seconds
    };

    // Unauthorized check
    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("not_owner"),
            helper.tributes.clone(),
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
            helper.tributes.clone(),
            &ExecuteMsg::ClaimOwnership {},
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
        .execute_contract(helper.owner.clone(), helper.tributes.clone(), &msg, &[])
        .unwrap();

    // Claim from invalid addr
    let err = helper
        .app
        .execute_contract(
            helper.app.api().addr_make("invalid_addr"),
            helper.tributes.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop the ownership proposal
    helper
        .app
        .execute_contract(
            helper.owner.clone(),
            helper.tributes.clone(),
            &ExecuteMsg::DropOwnershipProposal {},
            &[],
        )
        .unwrap();

    // Claim ownership
    let err = helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.tributes.clone(),
            &ExecuteMsg::ClaimOwnership {},
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
        .execute_contract(helper.owner.clone(), helper.tributes.clone(), &msg, &[])
        .unwrap();
    helper
        .app
        .execute_contract(
            new_owner.clone(),
            helper.tributes.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap();

    assert_eq!(helper.query_config().unwrap().owner.to_string(), new_owner)
}

#[test]
fn test_add_tributes_flow() {
    let mut helper = Helper::new();

    let user = helper.app.api().addr_make("user");
    let tribute_amount = 100_000000u128;
    let tribute = Asset::native("reward", 100_000000u128);

    // Try to add tribute without funds
    let err = helper
        .add_tribute(&user, "rand_lp_token", &tribute, &[])
        .unwrap_err();

    assert_eq!(
        ContractError::InsuffiicientTributeToken {
            reward: tribute.to_string()
        },
        err.downcast().unwrap()
    );

    let funds = [tribute.as_coin().unwrap(), helper.fee.clone()];
    helper.mint_tokens(&user, &funds).unwrap();

    // Try non-whitelisted lp token
    let err = helper
        .add_tribute(&user, "rand_lp_token", &tribute, &funds)
        .unwrap_err();
    assert_eq!(
        ContractError::LpTokenNotWhitelisted {},
        err.downcast().unwrap()
    );

    let lp_token = helper.create_pair("token1", "token2");
    helper.whitelist(&lp_token).unwrap();

    let err = helper
        .add_tribute(
            &user,
            &lp_token,
            &tribute,
            &coins(tribute_amount - 1, "reward"),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::InsuffiicientTributeToken {
            reward: tribute.to_string()
        },
        err.downcast().unwrap()
    );

    helper.mint_tokens(&user, &coins(1, "rnd")).unwrap();
    // Add random coin in funds
    let err = helper
        .add_tribute(
            &user,
            &lp_token,
            &tribute,
            &[
                tribute.as_coin().unwrap(),
                helper.fee.clone(),
                coin(1, "rnd"),
            ],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Supplied coins contain unexpected 1rnd"
    );

    helper
        .add_tribute(&user, &lp_token, &tribute, &funds)
        .unwrap();

    // Now extending the tribute doesn't require fee
    let is_fee_required = helper
        .app
        .wrap()
        .query_wasm_smart::<bool>(
            &helper.tributes,
            &QueryMsg::IsFeeExpected {
                lp_token: lp_token.clone(),
                asset_info: tribute.info.clone(),
            },
        )
        .unwrap();
    assert!(!is_fee_required, "Fee is not expected");

    let funds = [tribute.as_coin().unwrap()];
    helper.mint_tokens(&user, &funds).unwrap();
    helper
        .add_tribute(&user, &lp_token, &tribute, &funds)
        .unwrap();

    // Check ASTRO tribute

    let tribute = Asset::native(&helper.astro, tribute_amount);
    helper
        .mint_tokens(
            &user,
            &[
                tribute.as_coin().unwrap(),
                coin(helper.fee.amount.u128() + 1, &helper.astro),
            ],
        )
        .unwrap();

    // Sending only tribute astro without fee
    let err = helper
        .add_tribute(&user, &lp_token, &tribute, &[tribute.as_coin().unwrap()])
        .unwrap_err();
    assert_eq!(
        ContractError::TributeFeeExpected {
            fee: helper.fee.to_string()
        },
        err.downcast().unwrap()
    );

    // Sending less fee than expected
    let err = helper
        .add_tribute(
            &user,
            &lp_token,
            &tribute,
            &coins(tribute_amount + helper.fee.amount.u128() - 1, &helper.astro),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::TributeFeeExpected {
            fee: helper.fee.to_string()
        },
        err.downcast().unwrap()
    );

    // Add more ASTRO than expected
    let err = helper
        .add_tribute(
            &user,
            &lp_token,
            &tribute,
            &coins(tribute_amount + helper.fee.amount.u128() + 1, &helper.astro),
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Supplied coins contain unexpected 1astro"
    );

    helper
        .add_tribute(
            &user,
            &lp_token,
            &tribute,
            &coins(tribute_amount + helper.fee.amount.u128(), &helper.astro),
        )
        .unwrap();
}

#[test]
fn test_multiple_rewards() {
    let mut helper = Helper::new();

    let user = helper.app.api().addr_make("user");

    let lp_token = helper.create_pair("token1", "token2");
    helper.whitelist(&lp_token).unwrap();

    let voter = helper.app.api().addr_make("voter1");
    helper.lock(&voter, 1_000000).unwrap();
    helper
        .vote(&voter, &[(lp_token.clone(), Decimal::one())])
        .unwrap();

    let rewards_limit = helper.query_config().unwrap().rewards_limit;

    for i in 0..rewards_limit {
        let tribute = Asset::native(format!("reward{i}"), 100_000000u128);
        let funds = [tribute.as_coin().unwrap(), helper.fee.clone()];
        helper.mint_tokens(&user, &funds).unwrap();

        helper
            .add_tribute(&user, &lp_token, &tribute, &funds)
            .unwrap();
    }

    let tribute = Asset::native("reward10", 100_000000u128);
    let funds = [tribute.as_coin().unwrap(), helper.fee.clone()];
    helper.mint_tokens(&user, &funds).unwrap();
    let err = helper
        .add_tribute(&user, &lp_token, &tribute, &funds)
        .unwrap_err();
    assert_eq!(
        ContractError::RewardsLimitExceeded {
            limit: rewards_limit
        },
        err.downcast().unwrap()
    );

    let tributes = helper.query_pool_tributes(&lp_token, None).unwrap();

    assert_eq!(
        tributes,
        [
            Asset::native("reward0", 100_000000u128),
            Asset::native("reward1", 100_000000u128),
            Asset::native("reward2", 100_000000u128),
            Asset::native("reward3", 100_000000u128),
            Asset::native("reward4", 100_000000u128),
            Asset::native("reward5", 100_000000u128),
            Asset::native("reward6", 100_000000u128),
            Asset::native("reward7", 100_000000u128),
            Asset::native("reward8", 100_000000u128),
            Asset::native("reward9", 100_000000u128),
        ]
    );

    let to_claim = helper.simulate_claim(&voter).unwrap();
    assert_eq!(to_claim, []);

    helper.timetravel(EPOCH_LENGTH);

    let to_claim = helper
        .simulate_claim(&voter)
        .unwrap()
        .into_iter()
        .sorted_by(|a, b| a.info.to_string().cmp(&b.info.to_string()))
        .collect_vec();
    assert_eq!(
        to_claim,
        [
            Asset::native("reward0", 100_000000u128),
            Asset::native("reward1", 100_000000u128),
            Asset::native("reward2", 100_000000u128),
            Asset::native("reward3", 100_000000u128),
            Asset::native("reward4", 100_000000u128),
            Asset::native("reward5", 100_000000u128),
            Asset::native("reward6", 100_000000u128),
            Asset::native("reward7", 100_000000u128),
            Asset::native("reward8", 100_000000u128),
            Asset::native("reward9", 100_000000u128),
        ]
    );

    let now = helper.app.block_info().time.seconds();
    let tributes = helper
        .query_all_epoch_tributes(Some(now), None)
        .unwrap()
        .into_iter()
        .sorted_by(|a, b| a.1.info.to_string().cmp(&b.1.info.to_string()))
        .collect_vec();
    assert_eq!(
        tributes,
        [
            (lp_token.clone(), Asset::native("reward0", 100_000000u128)),
            (lp_token.clone(), Asset::native("reward1", 100_000000u128)),
            (lp_token.clone(), Asset::native("reward2", 100_000000u128)),
            (lp_token.clone(), Asset::native("reward3", 100_000000u128)),
            (lp_token.clone(), Asset::native("reward4", 100_000000u128)),
            (lp_token.clone(), Asset::native("reward5", 100_000000u128)),
            (lp_token.clone(), Asset::native("reward6", 100_000000u128)),
            (lp_token.clone(), Asset::native("reward7", 100_000000u128)),
            (lp_token.clone(), Asset::native("reward8", 100_000000u128)),
            (lp_token.clone(), Asset::native("reward9", 100_000000u128)),
        ]
    );

    let tributes = helper
        .query_all_epoch_tributes(
            Some(now),
            Some((lp_token.clone(), AssetInfo::native("reward8"))),
        )
        .unwrap();
    assert_eq!(
        tributes,
        [(lp_token.clone(), Asset::native("reward9", 100_000000u128)),]
    );

    // Prev epoch tributes
    assert_eq!(
        helper
            .query_all_epoch_tributes(Some(now - EPOCH_LENGTH), None)
            .unwrap(),
        []
    );

    // Next epoch tributes
    assert_eq!(helper.query_all_epoch_tributes(None, None).unwrap(), []);
}

#[test]
fn test_cw20_tributes() {
    let mut helper = Helper::new();

    let lp_token = helper.create_pair("token1", "token2");
    helper.whitelist(&lp_token).unwrap();

    let user = helper.app.api().addr_make("user");

    let token_code_id = helper.app.store_code(token_contract());
    let token_addr = helper
        .app
        .instantiate_contract(
            token_code_id,
            user.clone(),
            &cw20_base::msg::InstantiateMsg {
                name: "Token".to_string(),
                symbol: "TKN".to_string(),
                decimals: 6,
                initial_balances: vec![Cw20Coin {
                    address: user.to_string(),
                    amount: Uint128::MAX,
                }],
                mint: None,
                marketing: None,
            },
            &[],
            "label",
            None,
        )
        .unwrap();

    let tribute = Asset::cw20_unchecked(&token_addr, 100_000000u128);

    let funds = [helper.fee.clone()];
    helper.mint_tokens(&user, &funds).unwrap();

    // Try to add cw20 tribute without allowance
    let err = helper
        .add_tribute(&user, &lp_token, &tribute, &funds)
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "No allowance for this account"
    );

    helper
        .app
        .execute_contract(
            user.clone(),
            token_addr.clone(),
            &cw20_base::msg::ExecuteMsg::IncreaseAllowance {
                spender: helper.tributes.to_string(),
                amount: 100_000000u128.into(),
                expires: None,
            },
            &[],
        )
        .unwrap();

    helper
        .add_tribute(&user, &lp_token, &tribute, &funds)
        .unwrap();

    let voter = helper.app.api().addr_make("voter");
    // Vote for pool
    helper.lock(&voter, 1_000000).unwrap();
    helper
        .vote(&voter, &[(lp_token.clone(), Decimal::one())])
        .unwrap();

    helper.timetravel(EPOCH_LENGTH);

    assert_eq!(helper.simulate_claim(&voter).unwrap(), [tribute.clone()]);

    helper.claim(&voter, None).unwrap();

    let cw20_bal = tribute.info.query_pool(&helper.app.wrap(), &voter).unwrap();
    assert_eq!(cw20_bal, tribute.amount);
}

#[test]
fn test_claim() {
    let mut helper = Helper::new();

    let lp_token1 = helper.create_pair("token1", "token2");
    let lp_token2 = helper.create_pair("token1", "token3");
    let lp_token3 = helper.create_pair("token2", "token3");

    for lp_token in [&lp_token1, &lp_token2, &lp_token3] {
        helper.whitelist(lp_token).unwrap();
    }

    let user = helper.app.api().addr_make("user");
    let tribute = Asset::native("reward", 100_000000u128);
    let funds = [tribute.as_coin().unwrap(), helper.fee.clone()];
    helper.mint_tokens(&user, &funds).unwrap();
    helper
        .add_tribute(&user, &lp_token1, &tribute, &funds)
        .unwrap();

    let voter1 = helper.app.api().addr_make("voter1");

    // Nothing to claim as tributes are for the next epoch
    let to_claim = helper.simulate_claim(&voter1).unwrap();
    assert_eq!(to_claim, []);

    helper.claim(&voter1, None).unwrap();

    helper.timetravel(EPOCH_LENGTH);

    // Nothing to claim as user didn't have vxASTRO
    let to_claim = helper.simulate_claim(&voter1).unwrap();
    assert_eq!(to_claim, []);

    helper.lock(&voter1, 1_000000).unwrap();
    // Vote for pools
    helper
        .vote(
            &voter1,
            &[
                (lp_token1.clone(), Decimal::percent(33)),
                (lp_token2.clone(), Decimal::percent(33)),
                (lp_token3.clone(), Decimal::percent(33)),
            ],
        )
        .unwrap();

    // Still nothing
    assert_eq!(helper.simulate_claim(&voter1).unwrap(), []);

    // Add tributes again
    helper.mint_tokens(&user, &funds).unwrap();
    helper
        .add_tribute(&user, &lp_token1, &tribute, &funds)
        .unwrap();

    helper.timetravel(EPOCH_LENGTH);

    // Now user is eligible for 100 tokens
    assert_eq!(helper.simulate_claim(&voter1).unwrap(), [tribute.clone()]);

    // Claim tributes
    helper.claim(&voter1, None).unwrap();
    let voter1_bal = helper.app.wrap().query_balance(&voter1, "reward").unwrap();
    assert_eq!(voter1_bal.amount, tribute.amount);

    // Another voter with 1 vxASTRO
    let voter2 = helper.app.api().addr_make("voter2");
    helper.lock(&voter2, 1_000000).unwrap();
    // Vote for pools
    helper
        .vote(
            &voter2,
            &[
                (lp_token1.clone(), Decimal::percent(33)),
                (lp_token2.clone(), Decimal::percent(33)),
                (lp_token3.clone(), Decimal::percent(33)),
            ],
        )
        .unwrap();

    // Add tributes to different pools every different epoch

    helper.mint_tokens(&user, &funds).unwrap();
    helper
        .add_tribute(&user, &lp_token1, &tribute, &funds)
        .unwrap();

    helper.timetravel(EPOCH_LENGTH);

    helper.mint_tokens(&user, &funds).unwrap();
    helper
        .add_tribute(&user, &lp_token2, &tribute, &funds)
        .unwrap();

    helper.timetravel(EPOCH_LENGTH);

    helper.mint_tokens(&user, &funds).unwrap();
    helper
        .add_tribute(&user, &lp_token3, &tribute, &funds)
        .unwrap();

    helper.timetravel(EPOCH_LENGTH);

    // Both voters must now be eligible for 150 tokens
    assert_eq!(
        helper.simulate_claim(&voter1).unwrap(),
        [tribute.info.with_balance(150_000000u128)]
    );
    assert_eq!(
        helper.simulate_claim(&voter2).unwrap(),
        [tribute.info.with_balance(150_000000u128)]
    );

    // Claim tributes
    helper.claim(&voter1, None).unwrap();
    helper.claim(&voter1, None).unwrap();
    let voter1_bal = helper.app.wrap().query_balance(&voter1, "reward").unwrap();
    assert_eq!(voter1_bal.amount.u128(), 250_000000);

    helper.claim(&voter2, None).unwrap();
    let voter2_bal = helper.app.wrap().query_balance(&voter2, "reward").unwrap();
    assert_eq!(voter2_bal.amount.u128(), 150_000000);
}

#[test]
fn test_update_config() {
    let mut helper = Helper::new();
    let owner = helper.owner.clone();

    let random = helper.app.api().addr_make("random");

    let err = helper.update_config(&random, None, None, None).unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let err = helper
        .update_config(
            &owner,
            Some(TributeFeeInfo {
                fee: coin(0, "astro"),
                fee_collector: owner.clone(),
            }),
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(
        ContractError::InvalidTributeFeeAmount {},
        err.downcast().unwrap()
    );

    let err = helper
        .update_config(
            &owner,
            Some(TributeFeeInfo {
                fee: coin(1, "astro"),
                fee_collector: Addr::unchecked(""),
            }),
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Invalid input");

    let err = helper
        .update_config(
            &owner,
            Some(TributeFeeInfo {
                fee: coin(1, "a"),
                fee_collector: owner.clone(),
            }),
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Invalid denom length [3,128]: a"
    );

    helper
        .update_config(
            &owner,
            Some(TributeFeeInfo {
                fee: coin(1, "astro"),
                fee_collector: owner.clone(),
            }),
            None,
            None,
        )
        .unwrap();

    let config = helper.query_config().unwrap();
    assert_eq!(config.tribute_fee_info.fee, coin(1, "astro"));

    let err = helper
        .update_config(&owner, None, Some(REWARDS_AMOUNT_LIMITS.end() + 1), None)
        .unwrap_err();
    assert_eq!(
        ContractError::InvalidRewardsLimit {},
        err.downcast().unwrap()
    );

    helper
        .update_config(&owner, None, Some(*REWARDS_AMOUNT_LIMITS.end()), None)
        .unwrap();

    let config = helper.query_config().unwrap();
    assert_eq!(config.rewards_limit, *REWARDS_AMOUNT_LIMITS.end());

    let err = helper
        .update_config(&owner, None, None, Some(TOKEN_TRANSFER_GAS_LIMIT.end() + 1))
        .unwrap_err();
    assert_eq!(
        ContractError::InvalidTokenTransferGasLimit {},
        err.downcast().unwrap()
    );

    helper
        .update_config(&owner, None, None, Some(*TOKEN_TRANSFER_GAS_LIMIT.end()))
        .unwrap();

    let config = helper.query_config().unwrap();
    assert_eq!(
        config.token_transfer_gas_limit,
        *TOKEN_TRANSFER_GAS_LIMIT.end()
    );
}

#[test]
fn test_remove_tribute() {
    let mut helper = Helper::new();
    let owner = helper.owner.clone();

    let lp_token = helper.create_pair("token1", "token2");
    helper.whitelist(&lp_token).unwrap();

    let user = helper.app.api().addr_make("user");

    let tribute = Asset::native("reward", 100_000000u128);
    let funds = [tribute.as_coin().unwrap(), helper.fee.clone()];
    helper.mint_tokens(&user, &funds).unwrap();
    helper
        .add_tribute(&user, &lp_token, &tribute, &funds)
        .unwrap();

    helper.lock(&user, 1_000000).unwrap();
    helper
        .vote(&user, &[(lp_token.clone(), Decimal::one())])
        .unwrap();

    let random = helper.app.api().addr_make("random");
    let err = helper
        .remove_tribute(&random, &lp_token, &tribute.info, &random)
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let dereg_receiver = helper.app.api().addr_make("dereg_receiver");

    let err = helper
        .remove_tribute(
            &owner,
            &lp_token,
            &AssetInfo::native("rnd"),
            &dereg_receiver,
        )
        .unwrap_err();
    assert_eq!(
        ContractError::TributeNotFound {
            lp_token: lp_token.clone(),
            asset_info: "rnd".to_string()
        },
        err.downcast().unwrap()
    );

    let err = helper
        .remove_tribute(&owner, "cosmwasm1lptoken", &tribute.info, &dereg_receiver)
        .unwrap_err();
    assert_eq!(
        ContractError::TributeNotFound {
            lp_token: "cosmwasm1lptoken".to_string(),
            asset_info: tribute.info.to_string()
        },
        err.downcast().unwrap()
    );

    helper
        .remove_tribute(&owner, &lp_token, &tribute.info, &dereg_receiver)
        .unwrap();

    let dereg_bal = helper
        .app
        .wrap()
        .query_balance(&dereg_receiver, "reward")
        .unwrap();
    assert_eq!(dereg_bal.amount, tribute.amount);

    helper.timetravel(EPOCH_LENGTH);

    assert_eq!(helper.simulate_claim(&user).unwrap(), []);

    let err = helper
        .remove_tribute(&owner, &lp_token, &tribute.info, &dereg_receiver)
        .unwrap_err();
    assert_eq!(
        ContractError::TributeNotFound {
            lp_token: lp_token.clone(),
            asset_info: tribute.info.to_string()
        },
        err.downcast().unwrap()
    );
}
