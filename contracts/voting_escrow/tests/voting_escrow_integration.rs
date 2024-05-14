use cosmwasm_std::{coin, Addr};
use cw20::{BalanceResponse, LogoInfo, MarketingInfoResponse, TokenInfoResponse};
use cw_multi_test::Executor;
use cw_utils::PaymentError;

use astroport_governance::voting_escrow::{Config, LockInfoResponse, QueryMsg};
use astroport_voting_escrow::error::ContractError;
use astroport_voting_escrow::state::UNLOCK_PERIOD;

use crate::helper::EscrowHelper;

mod helper;
#[test]
fn test_lock() {
    let xastro_denom = "xastro";
    let mut helper = EscrowHelper::new(xastro_denom);

    let user1 = Addr::unchecked("user1");

    // Non existing user has 0 voting power
    assert_eq!(0, helper.user_vp(&user1, None).unwrap().u128());

    // Try to lock non xASTRO tokens
    let non_xastro = coin(100, "non_xastro");
    helper.mint_tokens(&user1, &[non_xastro.clone()]).unwrap();
    let err = helper.lock(&user1, &[non_xastro.clone()]).unwrap_err();
    assert_eq!(
        ContractError::PaymentError(PaymentError::MissingDenom(xastro_denom.to_string())),
        err.downcast().unwrap(),
    );

    // Try to mix xASTRO and non xASTRO tokens
    let xastro_coin = coin(100, xastro_denom);
    helper.mint_tokens(&user1, &[xastro_coin.clone()]).unwrap();

    let err = helper
        .lock(&user1, &[xastro_coin.clone(), non_xastro])
        .unwrap_err();
    assert_eq!(
        ContractError::PaymentError(PaymentError::MultipleDenoms {}),
        err.downcast().unwrap(),
    );

    // Correct lock
    helper.lock(&user1, &[xastro_coin]).unwrap();

    let user_vp = helper.user_vp(&user1, None).unwrap();
    assert_eq!(100, user_vp.u128());

    // Lock more
    let xastro_coin = coin(200, xastro_denom);
    helper.mint_tokens(&user1, &[xastro_coin.clone()]).unwrap();
    helper.lock(&user1, &[xastro_coin]).unwrap();

    let user_vp = helper.user_vp(&user1, None).unwrap();
    assert_eq!(300, user_vp.u128());

    // Voting power 1 second before is 0
    let user_vp = helper
        .user_vp(&user1, Some(helper.app.block_info().time.seconds() - 1))
        .unwrap();
    assert_eq!(0, user_vp.u128());
    let total_vp = helper
        .total_vp(Some(helper.app.block_info().time.seconds() - 1))
        .unwrap();
    assert_eq!(0, total_vp.u128());

    helper.timetravel(10000);

    // lock more
    let xastro_coin = coin(1000, xastro_denom);
    helper.mint_tokens(&user1, &[xastro_coin.clone()]).unwrap();
    helper.lock(&user1, &[xastro_coin]).unwrap();
    assert_eq!(1300, helper.user_vp(&user1, None).unwrap().u128());
}

#[test]
fn test_unlok() {
    let xastro_denom = "xastro";
    let mut helper = EscrowHelper::new(xastro_denom);

    let user1 = Addr::unchecked("user1");

    // Try to unlock without locking
    let err = helper.unlock(&user1).unwrap_err();
    assert_eq!(ContractError::ZeroBalance {}, err.downcast().unwrap());

    // Try to relock without locking
    let err = helper.relock(&user1).unwrap_err();
    assert_eq!(
        ContractError::NotInUnlockingState {},
        err.downcast().unwrap()
    );

    // Try to withdraw without locking
    let err = helper.withdraw(&user1).unwrap_err();
    assert_eq!(
        ContractError::NotInUnlockingState {},
        err.downcast().unwrap()
    );

    // Create lock
    let xastro_coin = coin(100, xastro_denom);
    helper.mint_tokens(&user1, &[xastro_coin.clone()]).unwrap();
    helper.lock(&user1, &[xastro_coin.clone()]).unwrap();
    assert_eq!(100, helper.user_vp(&user1, None).unwrap().u128());
    assert_eq!(100, helper.total_vp(None).unwrap().u128());

    // Try to relock not unlocked position
    let err = helper.relock(&user1).unwrap_err();
    assert_eq!(
        ContractError::NotInUnlockingState {},
        err.downcast().unwrap()
    );
    // Withdraw still does nothing
    let err = helper.withdraw(&user1).unwrap_err();
    assert_eq!(
        ContractError::NotInUnlockingState {},
        err.downcast().unwrap()
    );

    // Start unlocking
    let start_ts = helper.app.block_info().time.seconds();
    helper.unlock(&user1).unwrap();

    // User lost voting power immediately. His contribution is removed from total vp
    assert_eq!(0, helper.user_vp(&user1, None).unwrap().u128());
    assert_eq!(0, helper.total_vp(None).unwrap().u128());

    helper.timetravel(10000);

    // Try to unlock again
    let err = helper.unlock(&user1).unwrap_err();
    assert_eq!(ContractError::PositionUnlocking {}, err.downcast().unwrap());

    let lock = helper.lock_info(&user1).unwrap();
    assert_eq!(
        lock,
        LockInfoResponse {
            amount: xastro_coin.amount,
            end: Some(start_ts + UNLOCK_PERIOD)
        }
    );

    // Try to withdraw before unlock
    let err = helper.withdraw(&user1).unwrap_err();
    assert_eq!(
        ContractError::UnlockPeriodNotExpired(start_ts + UNLOCK_PERIOD),
        err.downcast().unwrap()
    );

    // Cant lock while in unlocking state
    helper.mint_tokens(&user1, &[xastro_coin.clone()]).unwrap();
    let err = helper.lock(&user1, &[xastro_coin.clone()]).unwrap_err();
    assert_eq!(ContractError::PositionUnlocking {}, err.downcast().unwrap());

    // Relock works
    helper.relock(&user1).unwrap();
    // Voting power is recovered
    assert_eq!(100, helper.user_vp(&user1, None).unwrap().u128());
    assert_eq!(100, helper.total_vp(None).unwrap().u128());

    let lock = helper.lock_info(&user1).unwrap();
    assert_eq!(
        lock,
        LockInfoResponse {
            amount: xastro_coin.amount,
            end: None,
        }
    );

    // Normal unlocking flow
    let bal_before = helper
        .app
        .wrap()
        .query_balance(&user1, xastro_denom)
        .unwrap()
        .amount;
    helper.unlock(&user1).unwrap();
    helper.timetravel(UNLOCK_PERIOD);
    helper.withdraw(&user1).unwrap();

    assert_eq!(0, helper.user_vp(&user1, None).unwrap().u128());
    let bal_after = helper
        .app
        .wrap()
        .query_balance(&user1, xastro_denom)
        .unwrap()
        .amount;
    assert_eq!(xastro_coin.amount, bal_after - bal_before);
}

#[test]
fn test_general_queries() {
    let xastro_denom = "xastro";
    let mut helper = EscrowHelper::new(xastro_denom);

    let user1 = Addr::unchecked("user1");
    let xastro_coin = coin(100, xastro_denom);
    helper.mint_tokens(&user1, &[xastro_coin.clone()]).unwrap();
    helper.lock(&user1, &[xastro_coin.clone()]).unwrap();

    let config: Config = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.vxastro_contract, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        config,
        Config {
            deposit_denom: xastro_denom.to_string(),
        }
    );

    let token_info: TokenInfoResponse = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.vxastro_contract, &QueryMsg::TokenInfo {})
        .unwrap();
    let total_vp = helper.total_vp(None).unwrap();

    assert_eq!(
        token_info,
        TokenInfoResponse {
            name: "Vote Escrowed xASTRO".to_string(),
            symbol: "vxASTRO".to_string(),
            decimals: 6,
            total_supply: total_vp,
        }
    );

    let cw20_bal_resp: BalanceResponse = helper
        .app
        .wrap()
        .query_wasm_smart(
            &helper.vxastro_contract,
            &QueryMsg::Balance {
                address: user1.to_string(),
            },
        )
        .unwrap();
    let user_vp = helper.user_vp(&user1, None).unwrap();
    assert_eq!(user_vp, cw20_bal_resp.balance);

    let marketing_info: MarketingInfoResponse = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.vxastro_contract, &QueryMsg::MarketingInfo {})
        .unwrap();
    assert_eq!(marketing_info.marketing, Some(helper.owner.clone()));

    // Update marketing
    let new_marketing = Addr::unchecked("new_marketing");
    let update_msg = astroport_governance::voting_escrow::ExecuteMsg::UpdateMarketing {
        project: Some("new_project".to_string()),
        description: Some("new_description".to_string()),
        marketing: Some(new_marketing.to_string()),
    };
    let err = helper
        .app
        .execute_contract(
            Addr::unchecked("random"),
            helper.vxastro_contract.clone(),
            &update_msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(
        ContractError::Cw20Base(cw20_base::ContractError::Unauthorized {}),
        err.downcast().unwrap(),
    );

    let owner = helper.owner.clone();
    helper
        .app
        .execute_contract(owner, helper.vxastro_contract.clone(), &update_msg, &[])
        .unwrap();

    let marketing_info: MarketingInfoResponse = helper
        .app
        .wrap()
        .query_wasm_smart(&helper.vxastro_contract, &QueryMsg::MarketingInfo {})
        .unwrap();
    assert_eq!(
        marketing_info,
        MarketingInfoResponse {
            project: Some("new_project".to_string()),
            description: Some("new_description".to_string()),
            logo: Some(LogoInfo::Url("https://example.com".to_string())),
            marketing: Some(new_marketing),
        }
    );
}
