use cosmwasm_std::{from_binary, DepsMut, Env, StdResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport_governance::voting_escrow::MigrateMsg;

pub(crate) mod v110;

pub(crate) trait Migration<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Clone + PartialEq + JsonSchema,
{
    fn handle_migration(deps: DepsMut, env: Env, params: T) -> StdResult<()>;
    fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> StdResult<()> {
        let params = from_binary::<T>(&msg.params)?;
        Self::handle_migration(deps, env, params)
    }
}
