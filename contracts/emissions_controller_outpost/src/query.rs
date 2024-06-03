#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, StdResult};

use astroport_governance::emissions_controller::outpost::{QueryMsg, UserIbcStatus};

use crate::state::{CONFIG, PENDING_MESSAGES, USER_IBC_ERROR};

/// Expose available contract queries.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::QueryUserIbcStatus { user } => to_json_binary(&UserIbcStatus {
            pending_msg: PENDING_MESSAGES.may_load(deps.storage, &user)?,
            error: USER_IBC_ERROR.may_load(deps.storage, &user)?,
        }),
    }
}
