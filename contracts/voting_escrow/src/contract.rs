use astroport::asset::{addr_opt_validate, validate_native_denom};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coins, to_json_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw20::{BalanceResponse, Logo, LogoInfo, MarketingInfoResponse, TokenInfoResponse};
use cw20_base::contract::{execute_update_marketing, query_marketing_info};
use cw20_base::state::{MinterData, TokenInfo, LOGO, MARKETING_INFO, TOKEN_INFO};
use cw_utils::must_pay;

use astroport_governance::voting_escrow::{
    Config, ExecuteMsg, InstantiateMsg, LockInfoResponse, QueryMsg,
};

use crate::error::ContractError;
use crate::state::{get_total_vp, Lock, CONFIG};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    validate_native_denom(&msg.deposit_denom)?;

    let config = Config {
        deposit_denom: msg.deposit_denom.clone(),
    };
    CONFIG.save(deps.storage, &config)?;

    if let Some(marketing) = msg.marketing {
        let logo = match &marketing.logo {
            Some(Logo::Url(url)) => {
                LOGO.save(deps.storage, &marketing.logo.clone().unwrap())?;
                Some(LogoInfo::Url(url.clone()))
            }
            _ => {
                return Err(StdError::generic_err("Logo url must be set").into());
            }
        };

        let data = MarketingInfoResponse {
            project: marketing.project,
            description: marketing.description,
            marketing: addr_opt_validate(deps.api, &marketing.marketing)?,
            logo,
        };
        MARKETING_INFO.save(deps.storage, &data)?;
    } else {
        return Err(StdError::generic_err("Marketing info is required").into());
    }

    // Store token info
    let data = TokenInfo {
        name: "Vote Escrowed xASTRO".to_string(),
        symbol: "vxASTRO".to_string(),
        decimals: 6,
        total_supply: Uint128::zero(),
        mint: Some(MinterData {
            minter: env.contract.address,
            cap: None,
        }),
    };

    TOKEN_INFO.save(deps.storage, &data)?;

    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
///
/// ## Execute messages
/// * **ExecuteMsg::ExtendLockTime { time }** Increase a staker's lock time.
///
/// * **ExecuteMsg::Receive(msg)** Parse incoming messages coming from the xASTRO token contract.
///
/// * **ExecuteMsg::Withdraw {}** Withdraw all xASTRO from a lock position if the lock has expired.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Lock { receiver } => {
            let config = CONFIG.load(deps.storage)?;
            let deposit = must_pay(&info, &config.deposit_denom)?;
            let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or(info.sender);
            let block_ts = env.block.time.seconds();

            let mut position = Lock::load(deps.storage, block_ts, &receiver)?;
            position.lock(deps.storage, deposit)?;

            Ok(Response::default().add_attributes([
                attr("action", "lock"),
                attr("receiver", receiver),
                attr("deposit_amount", deposit),
                attr("new_lock_amount", position.amount),
            ]))
        }
        ExecuteMsg::Unlock {} => {
            let mut position = Lock::load(deps.storage, env.block.time.seconds(), &info.sender)?;
            let unlock_time = position.unlock(deps.storage)?;

            // TODO: kick user from generator controller votes

            Ok(Response::default().add_attributes([
                attr("action", "unlock"),
                attr("receiver", info.sender),
                attr("unlocked_amount", position.amount),
                attr("unlock_time", unlock_time.to_string()),
            ]))
        }
        ExecuteMsg::Relock {} => {
            let mut position = Lock::load(deps.storage, env.block.time.seconds(), &info.sender)?;
            position.relock(deps.storage)?;

            Ok(Response::default()
                .add_attributes([attr("action", "relock"), attr("receiver", info.sender)]))
        }
        ExecuteMsg::Withdraw {} => {
            let mut position = Lock::load(deps.storage, env.block.time.seconds(), &info.sender)?;
            let config = CONFIG.load(deps.storage)?;
            let amount = position.withdraw(deps.storage)?;

            let send_msg = BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: coins(amount.u128(), config.deposit_denom),
            };

            Ok(Response::new().add_message(send_msg).add_attributes([
                attr("action", "withdraw"),
                attr("receiver", info.sender),
                attr("withdrawn_amount", amount),
            ]))
        }
        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => execute_update_marketing(deps, env, info, project, description, marketing)
            .map_err(Into::into),
    }
}

/// Expose available contract queries.
///
/// ## Queries
/// * **QueryMsg::TotalVotingPower {}** Fetch the total voting power (vxASTRO supply) at the current block.
///
/// * **QueryMsg::UserVotingPower { user }** Fetch the user's voting power (vxASTRO balance) at the current block.
///
/// * **QueryMsg::TotalVotingPowerAt { time }** Fetch the total voting power (vxASTRO supply) at a specified timestamp.
///
/// * **QueryMsg::UserVotingPowerAt { time }** Fetch the user's voting power (vxASTRO balance) at a specified timestamp.
///
/// * **QueryMsg::LockInfo { user }** Fetch a user's lock information.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::TotalVotingPower { time } => {
            to_json_binary(&get_total_vp(deps.storage, env.block.time.seconds(), time)?)
        }
        QueryMsg::UserVotingPower { user, time } => {
            to_json_binary(&query_user_voting_power(deps, env, user, time)?)
        }
        QueryMsg::LockInfo { user } => {
            let user = deps.api.addr_validate(&user)?;
            let lock_info_resp: LockInfoResponse =
                Lock::load(deps.storage, env.block.time.seconds(), &user)?.into();
            to_json_binary(&lock_info_resp)
        }
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Balance { address } => {
            let user_vp = query_user_voting_power(deps, env, address, None)?;
            to_json_binary(&BalanceResponse { balance: user_vp })
        }
        QueryMsg::TokenInfo {} => to_json_binary(&query_token_info(deps, env)?),
        QueryMsg::MarketingInfo {} => to_json_binary(&query_marketing_info(deps)?),
    }
}

/// Fetch the vxASTRO token information, such as the token name, symbol, decimals and total supply (total voting power).
pub fn query_token_info(deps: Deps, env: Env) -> StdResult<TokenInfoResponse> {
    let token_info = TOKEN_INFO.load(deps.storage)?;
    let res = TokenInfoResponse {
        name: token_info.name,
        symbol: token_info.symbol,
        decimals: token_info.decimals,
        total_supply: get_total_vp(deps.storage, env.block.time.seconds(), None)?,
    };
    Ok(res)
}

pub fn query_user_voting_power(
    deps: Deps,
    env: Env,
    address: String,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    let address = deps.api.addr_validate(&address)?;
    let voting_power =
        Lock::load_at_ts(deps.storage, env.block.time.seconds(), &address, timestamp)?
            .get_voting_power();
    Ok(voting_power)
}
