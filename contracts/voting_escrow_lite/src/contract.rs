#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, Uint128};
use cw2::set_contract_version;
use cw20::{Logo, LogoInfo, MarketingInfoResponse};
use cw20_base::state::{TokenInfo, LOGO, MARKETING_INFO, TOKEN_INFO};

use astroport_governance::utils::DEFAULT_UNLOCK_PERIOD;
use astroport_governance::voting_escrow_lite::{Config, InstantiateMsg};

use crate::astroport::asset::{addr_opt_validate, validate_native_denom};
use crate::error::ContractError;
use crate::marketing_validation::{validate_marketing_info, validate_whitelist_links};
use crate::state::{BLACKLIST, CONFIG, VOTING_POWER_HISTORY};

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

    validate_whitelist_links(&msg.logo_urls_whitelist)?;
    let guardian_addr = addr_opt_validate(deps.api, &msg.guardian_addr)?;

    // We only allow either generator controller *or* the outpost to be set
    // If we're on the Hub generator controller should be set
    // If we're on an outpost, then outpost should be set
    if msg.generator_controller_addr.is_some() && msg.outpost_addr.is_some() {
        return Err(StdError::generic_err(
            "Only one of Generator Controller or Outpost can be set",
        )
        .into());
    }

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        guardian_addr,
        deposit_denom: msg.deposit_denom,
        logo_urls_whitelist: msg.logo_urls_whitelist.clone(),
        unlock_period: DEFAULT_UNLOCK_PERIOD,
        generator_controller_addr: addr_opt_validate(deps.api, &msg.generator_controller_addr)?,
        outpost_addr: addr_opt_validate(deps.api, &msg.outpost_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    VOTING_POWER_HISTORY.save(
        deps.storage,
        (env.contract.address, env.block.time.seconds()),
        &Uint128::zero(),
    )?;
    BLACKLIST.save(deps.storage, &vec![])?;

    if let Some(marketing) = msg.marketing {
        if msg.logo_urls_whitelist.is_empty() {
            return Err(StdError::generic_err("Logo URLs whitelist can not be empty").into());
        }

        validate_marketing_info(
            marketing.project.as_ref(),
            marketing.description.as_ref(),
            marketing.logo.as_ref(),
            &config.logo_urls_whitelist,
        )?;

        let logo = if let Some(logo) = marketing.logo {
            LOGO.save(deps.storage, &logo)?;

            match logo {
                Logo::Url(url) => Some(LogoInfo::Url(url)),
                Logo::Embedded(_) => Some(LogoInfo::Embedded),
            }
        } else {
            None
        };

        let data = MarketingInfoResponse {
            project: marketing.project,
            description: marketing.description,
            marketing: addr_opt_validate(deps.api, &marketing.marketing)?,
            logo,
        };
        MARKETING_INFO.save(deps.storage, &data)?;
    }

    // Store token info
    let data = TokenInfo {
        name: "Vote Escrowed xASTRO lite".to_string(),
        symbol: "vxASTRO".to_string(),
        decimals: 6,
        total_supply: Uint128::zero(),
        mint: None,
    };

    TOKEN_INFO.save(deps.storage, &data)?;

    Ok(Response::default())
}
