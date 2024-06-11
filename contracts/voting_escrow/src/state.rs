use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Addr, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, SnapshotItem, SnapshotMap, Strategy};

use astroport_governance::voting_escrow::{Config, LockInfoResponse, UnlockStatus};

use crate::error::ContractError;

pub const UNLOCK_PERIOD: u64 = 86400 * 14; // 2 weeks

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

fn default_addr() -> Addr {
    Addr::unchecked("")
}

#[cw_serde]
pub struct Lock {
    /// The total number of xASTRO tokens that were deposited in the vxASTRO position
    pub amount: Uint128,
    /// Unlocking status. None for positions in Locked state
    pub unlock_status: Option<UnlockStatus>,
    /// NOTE: The fields below are not stored in the state, they are used only in the contract logic
    #[serde(default = "default_addr", skip)]
    pub user: Addr,
    /// Current block timestamp.
    #[serde(skip)]
    pub block_time: u64,
}

impl Default for Lock {
    fn default() -> Self {
        Lock {
            amount: Uint128::zero(),
            unlock_status: None,
            user: default_addr(),
            block_time: 0,
        }
    }
}

impl Lock {
    pub fn load_at_ts(
        storage: &dyn Storage,
        block_time: u64,
        user: &Addr,
        timestamp: Option<u64>,
    ) -> StdResult<Self> {
        let lock = match timestamp.unwrap_or(block_time) {
            timestamp if timestamp == block_time => LOCKED.may_load(storage, user),
            timestamp => LOCKED.may_load_at_height(storage, user, timestamp),
        }?
        .unwrap_or_default();

        Ok(Lock {
            user: user.clone(),
            block_time,
            ..lock
        })
    }

    pub fn load(storage: &dyn Storage, block_time: u64, user: &Addr) -> StdResult<Self> {
        Self::load_at_ts(storage, block_time, user, None)
    }

    pub fn lock(
        &mut self,
        storage: &mut dyn Storage,
        amount: Uint128,
    ) -> Result<(), ContractError> {
        ensure!(
            self.unlock_status.is_none(),
            ContractError::PositionUnlocking {}
        );

        self.amount += amount;
        LOCKED.save(storage, &self.user, self, self.block_time)?;
        TOTAL_POWER
            .update(storage, self.block_time, |total| {
                Ok(total.unwrap_or_default() + amount)
            })
            .map(|_| ())
    }

    pub fn unlock(&mut self, storage: &mut dyn Storage) -> Result<u64, ContractError> {
        ensure!(!self.amount.is_zero(), ContractError::ZeroBalance {});
        ensure!(
            self.unlock_status.is_none(),
            ContractError::PositionUnlocking {}
        );

        let end = self.block_time + UNLOCK_PERIOD;
        self.unlock_status = Some(UnlockStatus {
            end,
            hub_confirmed: false,
        });
        LOCKED.save(storage, &self.user, self, self.block_time)?;

        // Remove user's voting power from the total
        TOTAL_POWER.update(storage, self.block_time, |total| -> StdResult<_> {
            Ok(total.unwrap_or_default().checked_sub(self.amount)?)
        })?;

        Ok(end)
    }

    pub fn confirm_unlock(&mut self, storage: &mut dyn Storage) -> StdResult<()> {
        // If for some reason the unlock status is not set,
        // we skip it silently so relayer can finish IBC transaction.
        if let Some(unlock_status) = self.unlock_status.as_mut() {
            unlock_status.hub_confirmed = true;
            LOCKED.save(storage, &self.user, self, self.block_time)?;
        }

        Ok(())
    }

    pub fn relock(&mut self, storage: &mut dyn Storage) -> Result<(), ContractError> {
        ensure!(
            self.unlock_status.is_some(),
            ContractError::NotInUnlockingState {}
        );

        self.unlock_status = None;
        LOCKED.save(storage, &self.user, self, self.block_time)?;

        // Add user's voting power back to the total
        TOTAL_POWER
            .update(storage, self.block_time, |total| {
                Ok(total.unwrap_or_default() + self.amount)
            })
            .map(|_| ())
    }

    pub fn withdraw(&mut self, storage: &mut dyn Storage) -> Result<Uint128, ContractError> {
        if let Some(unlock_status) = self.unlock_status {
            ensure!(
                self.block_time >= unlock_status.end,
                ContractError::UnlockPeriodNotExpired(unlock_status.end)
            );

            ensure!(
                unlock_status.hub_confirmed,
                ContractError::HubNotConfirmed {}
            );

            LOCKED.remove(storage, &self.user, self.block_time)?;

            Ok(self.amount)
        } else {
            Err(ContractError::NotInUnlockingState {})
        }
    }

    pub fn get_voting_power(&self) -> Uint128 {
        if self.unlock_status.is_some() {
            Uint128::zero()
        } else {
            self.amount
        }
    }
}

impl From<Lock> for LockInfoResponse {
    fn from(lock: Lock) -> Self {
        LockInfoResponse {
            amount: lock.amount,
            unlock_status: lock.unlock_status,
        }
    }
}

pub fn get_total_vp(
    storage: &dyn Storage,
    block_time: u64,
    timestamp: Option<u64>,
) -> StdResult<Uint128> {
    match timestamp.unwrap_or(block_time) {
        timestamp if timestamp == block_time => TOTAL_POWER.may_load(storage),
        timestamp => TOTAL_POWER.may_load_at_height(storage, timestamp),
    }
    .map(Option::unwrap_or_default)
}

/// Stores historical balances for each account
pub const LOCKED: SnapshotMap<&Addr, Lock> = SnapshotMap::new(
    "locked",
    "locked__checkpoints",
    "locked__changelog",
    Strategy::EveryBlock,
);

pub const TOTAL_POWER: SnapshotItem<Uint128> = SnapshotItem::new(
    "total_power",
    "total_power__checkpoints",
    "total_power__changelog",
    Strategy::EveryBlock,
);
