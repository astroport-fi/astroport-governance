use crate::error::ContractError;
use astroport_governance::escrow_fee_distributor::Point;
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Deps, StdError, StdResult, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;

/// ## Description
/// Transfer amount of token.
///
/// ## Params
/// * **contract_addr** is the object of type [`Addr`].
///
/// * **recipient** is the object of type [`Addr`].
///
/// * **amount** is the object of type [`Uint128`].
///
pub fn transfer_token_amount(
    contract_addr: Addr,
    recipient: Addr,
    amount: Uint128,
) -> Result<Vec<CosmosMsg>, ContractError> {
    let messages = if !amount.is_zero() {
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: recipient.to_string(),
                amount,
            })?,
            funds: vec![],
        })]
    } else {
        vec![]
    };

    Ok(messages)
}

/// ## Description
/// Find timestamp period.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **voting_escrow** is the object of type [`Addr`].
///
/// * **timestamp** is the object of type [`u64`].
///
pub fn find_timestamp_period(_deps: Deps, _voting_escrow: Addr, timestamp: u64) -> StdResult<u64> {
    let mut min: u64 = 0;
    let mut max = 100; // TODO: use query below when it will be created
                       // let mut max: u64 = deps
                       //     .querier
                       //     .query_wasm_smart(&voting_escrow, &VotingQueryMsg::epoch {})?;
    for _i in 1..128 {
        if min >= max {
            break;
        }

        let mid = (min + max + 2)
            .checked_div(2)
            .ok_or_else(|| StdError::generic_err("Calculation error."))?;

        let pt = Point {
            bias: 0,
            slope: 0,
            ts: Default::default(),
            blk: Default::default(),
        }; // TODO: use query below when it will be created
           // let pt: Point = deps
           //     .querier
           //     .query_wasm_smart(&voting_escrow, &VotingQueryMsg::PointHistory { mid })?;

        if pt.ts <= timestamp {
            min = mid;
        } else {
            max = mid - 1;
        }
    }

    Ok(min)
}

/// ## Description
/// Find timestamp user period.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **voting_escrow** is the object of type [`Addr`].
///
/// * **user** is the object of type [`Addr`].
///
/// * **timestamp** is the object of type [`u64`].
///
/// * **max_user_epoch** is the object of type [`u64`].
///
pub fn find_timestamp_user_period(
    _deps: Deps,
    _voting_escrow: Addr,
    _user: Addr,
    timestamp: u64,
    max_user_epoch: u64,
) -> StdResult<u64> {
    let mut min: u64 = 0;
    let mut max: u64 = max_user_epoch;

    for _i in 1..128 {
        if min >= max {
            break;
        }

        let mid = (min + max + 2) / 2;

        let pt = Point {
            bias: 0,
            slope: 0,
            ts: Default::default(),
            blk: Default::default(),
        }; // TODO: use query below when it will be created
           // let pt: Point = deps.querier.query_wasm_smart(
           //     &voting_escrow,
           //     &VotingQueryMsg::UserPointHistory {
           //         user: user.to_string(),
           //         mid,
           //     },
           // )?;

        if pt.ts <= timestamp {
            min = mid;
        } else {
            max = mid - 1;
        }
    }

    Ok(min)
}
