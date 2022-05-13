use crate::error::ContractError;
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;

/// Transfer tokens to another address.
/// ## Params
/// * **contract_addr** is an object of type [`Addr`]. This is the address of the token conract.
///
/// * **recipient** is an object of type [`Addr`]. This is the address of the token recipient.
///
/// * **amount** is an object of type [`Uint128`]. This is the token amount to transfer.
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
