use astroport::common::OwnershipProposal;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Addr, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, SnapshotItem, SnapshotMap, Strategy};

use astroport_governance::voting_escrow::{Config, LockInfoResponse};

use crate::error::ContractError;

pub const UNLOCK_PERIOD: u64 = 86400 * 14; // 2 weeks

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

fn default_addr() -> Addr {
    Addr::unchecked("")
}

#[cw_serde]
pub struct Lock {
    /// The total amount of xASTRO tokens that were deposited in the vxASTRO position
    pub amount: Uint128,
    /// The timestamp when a lock will be unlocked. None for positions in Locked state
    pub end: Option<u64>,
    /// NOTE: The fields below are not stored in the state, it is used only in the contract logic
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
            end: None,
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
        ensure!(self.end.is_none(), ContractError::PositionUnlocking {});

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
        ensure!(self.end.is_none(), ContractError::PositionUnlocking {});

        let end = self.block_time + UNLOCK_PERIOD;
        self.end = Some(end);
        LOCKED.save(storage, &self.user, self, self.block_time)?;

        // Remove user's voting power from the total
        TOTAL_POWER.update(storage, self.block_time, |total| -> StdResult<_> {
            Ok(total.unwrap_or_default().checked_sub(self.amount)?)
        })?;

        Ok(end)
    }

    pub fn relock(&mut self, storage: &mut dyn Storage) -> Result<(), ContractError> {
        ensure!(self.end.is_some(), ContractError::NotInUnlockingState {});

        self.end = None;
        LOCKED.save(storage, &self.user, self, self.block_time)?;

        // Add user's voting power back to the total
        TOTAL_POWER
            .update(storage, self.block_time, |total| {
                Ok(total.unwrap_or_default() + self.amount)
            })
            .map(|_| ())
    }

    pub fn withdraw(&mut self, storage: &mut dyn Storage) -> Result<Uint128, ContractError> {
        if let Some(end) = self.end {
            ensure!(
                self.block_time >= end,
                ContractError::UnlockPeriodNotExpired(end)
            );

            LOCKED.remove(storage, &self.user, self.block_time)?;

            Ok(self.amount)
        } else {
            Err(ContractError::NotInUnlockingState {})
        }
    }

    pub fn get_voting_power(&self) -> Uint128 {
        if self.end.is_some() {
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
            end: lock.end,
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

/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
