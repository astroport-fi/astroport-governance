use crate::{error::ContractError, state::VOTING_POWER_HISTORY};

use cosmwasm_std::{Addr, Order, StdResult, Storage, Uint128};
use cw_storage_plus::Bound;

use crate::state::{BLACKLIST, CONFIG};

/// Checks that the sender is the xASTRO token.
pub(crate) fn xastro_token_check(storage: &dyn Storage, sender: Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(storage)?;
    if sender != config.deposit_token_addr {
        Err(ContractError::Unauthorized {})
    } else {
        Ok(())
    }
}

/// Checks if the blacklist contains a specific address.
pub(crate) fn blacklist_check(storage: &dyn Storage, addr: &Addr) -> Result<(), ContractError> {
    let blacklist = BLACKLIST.load(storage)?;
    if blacklist.contains(addr) {
        Err(ContractError::AddressBlacklisted(addr.to_string()))
    } else {
        Ok(())
    }
}

/// Fetches the last known voting power in [`VOTING_POWER_HISTORY`] for the given address.
pub(crate) fn fetch_last_checkpoint(
    storage: &dyn Storage,
    addr: &Addr,
    timestamp: u64,
) -> StdResult<Option<(u64, Uint128)>> {
    VOTING_POWER_HISTORY
        .prefix(addr.clone())
        .range(
            storage,
            None,
            Some(Bound::inclusive(timestamp)),
            Order::Descending,
        )
        .next()
        .transpose()
}
