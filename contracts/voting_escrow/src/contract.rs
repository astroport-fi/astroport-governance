use astroport::asset::{addr_opt_validate, validate_native_denom};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coins, ensure, ensure_eq, to_json_binary, wasm_execute, BankMsg, Binary, CosmosMsg, Deps,
    DepsMut, Empty, Env, MessageInfo, Order, Response, StdError, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw20::{BalanceResponse, Logo, LogoInfo, MarketingInfoResponse, TokenInfoResponse};
use cw20_base::contract::{execute_update_marketing, query_marketing_info};
use cw20_base::state::{MinterData, TokenInfo, LOGO, MARKETING_INFO, TOKEN_INFO};
use cw_storage_plus::Bound;
use cw_utils::must_pay;

use astroport_governance::emissions_controller;
use astroport_governance::emissions_controller::consts::MAX_PAGE_LIMIT;
use astroport_governance::voting_escrow::{
    Config, ExecuteMsg, InstantiateMsg, LockInfoResponse, QueryMsg,
};

use crate::error::ContractError;
use crate::state::{get_total_vp, Lock, CONFIG, LOCKED, PRIVILEGED};

/// Contract name that is used for migration.
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

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
        deposit_denom: msg.deposit_denom,
        emissions_controller: deps.api.addr_validate(&msg.emissions_controller)?,
    };
    CONFIG.save(deps.storage, &config)?;

    let logo = match &msg.marketing.logo {
        Logo::Url(url) => {
            LOGO.save(deps.storage, &msg.marketing.logo)?;
            Some(LogoInfo::Url(url.clone()))
        }
        _ => {
            return Err(StdError::generic_err("Logo url must be set").into());
        }
    };

    let data = MarketingInfoResponse {
        project: msg.marketing.project,
        description: msg.marketing.description,
        marketing: addr_opt_validate(deps.api, &msg.marketing.marketing)?,
        logo,
    };
    MARKETING_INFO.save(deps.storage, &data)?;

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

    PRIVILEGED.save(deps.storage, &vec![])?;

    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
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

            // Update user votes in emissions controller
            let update_votes_msg = wasm_execute(
                &config.emissions_controller,
                &emissions_controller::msg::ExecuteMsg::<Empty>::UpdateUserVotes {
                    user: receiver.to_string(),
                    is_unlock: false,
                },
                vec![],
            )?;

            Ok(Response::default()
                .add_message(update_votes_msg)
                .add_attributes([
                    attr("action", "lock"),
                    attr("receiver", receiver),
                    attr("deposit_amount", deposit),
                    attr("new_lock_amount", position.amount),
                ]))
        }
        ExecuteMsg::Unlock {} => {
            let mut position = Lock::load(deps.storage, env.block.time.seconds(), &info.sender)?;
            let unlock_time = position.unlock(deps.storage)?;

            // Update user votes in emissions controller
            let config = CONFIG.load(deps.storage)?;
            let update_votes_msg = wasm_execute(
                config.emissions_controller,
                &emissions_controller::msg::ExecuteMsg::<Empty>::UpdateUserVotes {
                    user: info.sender.to_string(),
                    is_unlock: true,
                },
                vec![],
            )?;

            Ok(Response::default()
                .add_message(update_votes_msg)
                .add_attributes([
                    attr("action", "unlock"),
                    attr("receiver", info.sender),
                    attr("unlocked_amount", position.amount),
                    attr("unlock_time", unlock_time.to_string()),
                ]))
        }
        ExecuteMsg::InstantUnlock { amount } => {
            let privileged = PRIVILEGED.load(deps.storage)?;
            ensure!(
                privileged.contains(&info.sender),
                ContractError::Unauthorized {}
            );

            let mut position = Lock::load(deps.storage, env.block.time.seconds(), &info.sender)?;
            position.instant_unlock(deps.storage, amount)?;

            // Update user votes in emissions controller
            let config = CONFIG.load(deps.storage)?;
            let update_votes_msg: CosmosMsg = wasm_execute(
                config.emissions_controller,
                &emissions_controller::msg::ExecuteMsg::<Empty>::UpdateUserVotes {
                    user: info.sender.to_string(),
                    // In this context, we don't need confirmation from emissions controller
                    // as xASTRO is instantly withdrawn
                    is_unlock: false,
                },
                vec![],
            )?
            .into();

            let send_msg: CosmosMsg = BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: coins(amount.u128(), config.deposit_denom),
            }
            .into();

            Ok(Response::default()
                .add_messages([update_votes_msg, send_msg])
                .add_attributes([
                    attr("action", "instant_unlock"),
                    attr("receiver", info.sender),
                    attr("unlocked_amount", amount),
                ]))
        }
        ExecuteMsg::Relock {} => {
            let mut position = Lock::load(deps.storage, env.block.time.seconds(), &info.sender)?;
            position.relock(deps.storage)?;

            // Update user votes in emissions controller
            let config = CONFIG.load(deps.storage)?;
            let update_votes_msg = wasm_execute(
                config.emissions_controller,
                &emissions_controller::msg::ExecuteMsg::<Empty>::UpdateUserVotes {
                    user: info.sender.to_string(),
                    is_unlock: false,
                },
                vec![],
            )?;

            Ok(Response::default()
                .add_message(update_votes_msg)
                .add_attributes([attr("action", "relock"), attr("receiver", info.sender)]))
        }
        ExecuteMsg::ForceRelock { user } => {
            let config = CONFIG.load(deps.storage)?;
            ensure!(
                info.sender == config.emissions_controller,
                ContractError::Unauthorized {}
            );

            let user = deps.api.addr_validate(&user)?;
            let mut position = Lock::load(deps.storage, env.block.time.seconds(), &user)?;
            position.relock(deps.storage)?;

            Ok(Response::default()
                .add_attributes([attr("action", "force_relock"), attr("receiver", user)]))
        }
        ExecuteMsg::ConfirmUnlock { user } => {
            let config = CONFIG.load(deps.storage)?;
            ensure!(
                info.sender == config.emissions_controller,
                ContractError::Unauthorized {}
            );

            let user = deps.api.addr_validate(&user)?;
            let mut position = Lock::load(deps.storage, env.block.time.seconds(), &user)?;
            position.confirm_unlock(deps.storage)?;

            Ok(Response::default()
                .add_attributes([attr("action", "confirm_unlock"), attr("receiver", user)]))
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
        ExecuteMsg::SetPrivilegedList { list } => {
            let config = CONFIG.load(deps.storage)?;

            // Query result deserialization into hub::Config
            // ensures we can call this endpoint only on the Hub
            let emissions_owner = deps
                .querier
                .query_wasm_smart::<emissions_controller::hub::Config>(
                    &config.emissions_controller,
                    &emissions_controller::hub::QueryMsg::Config {},
                )?
                .owner;
            ensure_eq!(info.sender, emissions_owner, ContractError::Unauthorized {});

            let privileged = list
                .iter()
                .map(|addr| deps.api.addr_validate(addr))
                .collect::<StdResult<Vec<_>>>()?;

            PRIVILEGED.save(deps.storage, &privileged)?;

            Ok(Response::default().add_attribute("action", "set_privileged_list"))
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
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::TotalVotingPower { timestamp } => to_json_binary(&get_total_vp(
            deps.storage,
            env.block.time.seconds(),
            timestamp,
        )?),
        QueryMsg::UserVotingPower { user, timestamp } => {
            to_json_binary(&query_user_voting_power(deps, env, user, timestamp)?)
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
        QueryMsg::PrivilegedList {} => to_json_binary(&PRIVILEGED.load(deps.storage)?),
        QueryMsg::UsersLockInfo {
            limit,
            start_after,
            timestamp,
        } => {
            let limit = limit.unwrap_or(MAX_PAGE_LIMIT) as usize;
            let start_after = addr_opt_validate(deps.api, &start_after)?;
            let user_infos = LOCKED
                .keys(
                    deps.storage,
                    start_after.as_ref().map(Bound::exclusive),
                    None,
                    Order::Ascending,
                )
                .take(limit)
                .map(|user| {
                    user.and_then(|user| {
                        let lock_info_resp: LockInfoResponse = Lock::load_at_ts(
                            deps.storage,
                            env.block.time.seconds(),
                            &user,
                            timestamp,
                        )?
                        .into();
                        Ok((user, lock_info_resp))
                    })
                })
                .collect::<StdResult<Vec<_>>>()?;
            Ok(to_json_binary(&user_infos)?)
        }
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
