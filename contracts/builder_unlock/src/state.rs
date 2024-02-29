use astroport::common::OwnershipProposal;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Addr, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map, SnapshotItem, SnapshotMap, Strategy};

use astroport_governance::builder_unlock::{
    AllocationParams, AllocationStatus, Config, CreateAllocationParams, Schedule,
    SimulateWithdrawResponse, State,
};

use crate::error::ContractError;

/// Stores the contract configuration
pub const CONFIG: Item<Config> = Item::new("config");
/// Stores global unlock state such as the total amount of ASTRO tokens still to be distributed
pub const STATE: SnapshotItem<State> = SnapshotItem::new(
    "state",
    "state__checkpoint",
    "state__changelog",
    Strategy::EveryBlock,
);
/// Allocation parameters for each unlock recipient
pub const PARAMS: Map<&Addr, AllocationParams> = Map::new("params");
/// The status of each unlock schedule
pub const STATUS: SnapshotMap<&Addr, AllocationStatus> = SnapshotMap::new(
    "status",
    "status__checkpoint",
    "status__changelog",
    Strategy::EveryBlock,
);
/// Contains a proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

#[cw_serde]
pub struct Allocation {
    /// The allocation parameters
    pub params: AllocationParams,
    /// The allocation status
    pub status: AllocationStatus,
    /// Allocation owner
    pub user: Addr,
    /// Current block timestamp
    pub block_ts: u64,
}

impl Allocation {
    pub fn must_load(
        storage: &dyn Storage,
        block_ts: u64,
        user: &Addr,
    ) -> Result<Self, ContractError> {
        let params = PARAMS
            .load(storage, user)
            .map_err(|_| ContractError::NoAllocation {
                address: user.to_string(),
            })?;
        let status = STATUS.may_load(storage, user)?.unwrap_or_default();

        Ok(Self {
            params,
            status,
            user: user.clone(),
            block_ts,
        })
    }

    pub fn save(self, storage: &mut dyn Storage) -> StdResult<()> {
        PARAMS.save(storage, &self.user, &self.params)?;
        STATUS.save(storage, &self.user, &self.status, self.block_ts)
    }

    pub fn new_allocation(
        storage: &mut dyn Storage,
        block_ts: u64,
        user: &Addr,
        params: CreateAllocationParams,
    ) -> Result<Self, ContractError> {
        ensure!(
            !PARAMS.has(storage, user),
            ContractError::AllocationExists {
                user: user.to_string()
            }
        );

        params.validate(user.as_str())?;

        Ok(Self {
            params: AllocationParams {
                unlock_schedule: params.unlock_schedule,
                proposed_receiver: None,
            },
            status: AllocationStatus {
                amount: params.amount,
                astro_withdrawn: Default::default(),
                unlocked_amount_checkpoint: Default::default(),
            },
            user: user.clone(),
            block_ts,
        })
    }

    pub fn withdraw_and_update(&mut self) -> Result<Uint128, ContractError> {
        ensure!(
            self.params.proposed_receiver.is_none(),
            ContractError::WithdrawErrorWhenProposedReceiver {}
        );

        let SimulateWithdrawResponse { astro_to_withdraw } =
            self.compute_withdraw_amount(self.block_ts);

        ensure!(
            !astro_to_withdraw.is_zero(),
            ContractError::NoUnlockedAstro {}
        );

        self.status.astro_withdrawn += astro_to_withdraw;

        Ok(astro_to_withdraw)
    }

    pub fn propose_new_receiver(
        &mut self,
        storage: &dyn Storage,
        new_receiver: &Addr,
    ) -> Result<(), ContractError> {
        match &self.params.proposed_receiver {
            Some(proposed_receiver) => Err(ContractError::ProposedReceiverAlreadySet {
                proposed_receiver: proposed_receiver.clone(),
            }),
            None => {
                ensure!(
                    !PARAMS.has(storage, new_receiver),
                    ContractError::ProposedReceiverAlreadyHasAllocation {}
                );

                self.params.proposed_receiver = Some(new_receiver.clone());

                Ok(())
            }
        }
    }

    pub fn drop_proposed_receiver(&mut self) -> Result<Addr, ContractError> {
        match self.params.proposed_receiver.clone() {
            Some(proposed_receiver) => {
                self.params.proposed_receiver = None;
                Ok(proposed_receiver)
            }
            None => Err(ContractError::ProposedReceiverNotSet {}),
        }
    }

    /// Produces new allocation object for new receiver. Old allocation is removed from state.
    pub fn claim_allocation(
        self,
        storage: &mut dyn Storage,
        new_receiver: &Addr,
    ) -> Result<Self, ContractError> {
        PARAMS.remove(storage, &self.user);
        STATUS.remove(storage, &self.user, self.block_ts)?;

        Ok(Self {
            user: new_receiver.clone(),
            params: AllocationParams {
                proposed_receiver: None,
                ..self.params
            },
            ..self
        })
    }

    /// Computes number of tokens that are now unlocked for a given allocation
    pub fn compute_unlocked_amount(&self, timestamp: u64) -> Uint128 {
        let (schedule, unlock_checkpoint, total_amount) = (
            &self.params.unlock_schedule,
            self.status.unlocked_amount_checkpoint,
            self.status.amount,
        );

        // Tokens haven't begun unlocking
        if timestamp < schedule.start_time + schedule.cliff {
            unlock_checkpoint
        } else if (timestamp < schedule.start_time + schedule.duration) && schedule.duration != 0 {
            // If percent_at_cliff is set, then this amount should be unlocked at cliff.
            // The rest of tokens are vested linearly between cliff and end_time
            let unlocked_amount = if let Some(percent_at_cliff) = schedule.percent_at_cliff {
                let amount_at_cliff = total_amount * percent_at_cliff;

                amount_at_cliff
                    + total_amount.saturating_sub(amount_at_cliff).multiply_ratio(
                        timestamp - schedule.start_time - schedule.cliff,
                        schedule.duration - schedule.cliff,
                    )
            } else {
                // Tokens unlock linearly between start time and end time
                total_amount.multiply_ratio(timestamp - schedule.start_time, schedule.duration)
            };

            if unlocked_amount > unlock_checkpoint {
                unlocked_amount
            } else {
                unlock_checkpoint
            }
        }
        // After end time, all tokens are fully unlocked
        else {
            total_amount
        }
    }

    /// Computes number of tokens that are withdrawable for a given allocation
    pub fn compute_withdraw_amount(&self, timestamp: u64) -> SimulateWithdrawResponse {
        let astro_unlocked = self.compute_unlocked_amount(timestamp);

        // Withdrawal amount is unlocked amount minus the amount already withdrawn
        SimulateWithdrawResponse {
            astro_to_withdraw: astro_unlocked - self.status.astro_withdrawn,
        }
    }

    pub fn decrease_allocation(&mut self, amount: Uint128) -> Result<(), ContractError> {
        let unlocked_amount = self.compute_unlocked_amount(self.block_ts);
        let locked_amount = self.status.amount - unlocked_amount;

        ensure!(
            locked_amount >= amount,
            ContractError::InsufficientLockedAmount { locked_amount }
        );

        self.status.amount = self.status.amount.checked_sub(amount)?;
        self.status.unlocked_amount_checkpoint = unlocked_amount;

        Ok(())
    }

    pub fn increase_allocation(&mut self, amount: Uint128) -> Result<(), ContractError> {
        self.status.amount += amount;
        Ok(())
    }

    pub fn update_unlock_schedule(&mut self, new_schedule: &Schedule) -> StdResult<()> {
        let unlocked_amount_checkpoint = self.compute_unlocked_amount(self.block_ts);

        if unlocked_amount_checkpoint > self.status.unlocked_amount_checkpoint {
            self.status.unlocked_amount_checkpoint = unlocked_amount_checkpoint;
        }

        self.params
            .update_schedule(new_schedule.clone(), self.user.as_str())
    }
}
