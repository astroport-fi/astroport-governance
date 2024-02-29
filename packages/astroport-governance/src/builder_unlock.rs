use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Decimal, StdError, StdResult, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    /// Account that can create new allocations
    pub owner: String,
    /// ASTRO token denom
    pub astro_denom: String,
    /// Max ASTRO tokens to allocate
    pub max_allocations_amount: Uint128,
}

/// This enum describes all the execute functions available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// CreateAllocations creates new ASTRO allocations
    CreateAllocations {
        allocations: Vec<(String, CreateAllocationParams)>,
    },
    /// Withdraw claims withdrawable ASTRO
    Withdraw {},
    /// ProposeNewReceiver allows a user to change the receiver address for their ASTRO allocation
    ProposeNewReceiver { new_receiver: String },
    /// DropNewReceiver allows a user to remove the previously proposed new receiver for their ASTRO allocation
    DropNewReceiver {},
    /// ClaimReceiver allows newly proposed receivers to claim ASTRO allocations ownership
    ClaimReceiver { prev_receiver: String },
    /// Increase the ASTRO allocation of a receiver
    IncreaseAllocation { receiver: String, amount: Uint128 },
    /// Decrease the ASTRO allocation of a receiver
    DecreaseAllocation { receiver: String, amount: Uint128 },
    /// Transfer unallocated tokens (only accessible to the owner)
    TransferUnallocated {
        amount: Uint128,
        recipient: Option<String>,
    },
    /// Propose a new owner for the contract
    ProposeNewOwner { new_owner: String, expires_in: u64 },
    /// Remove the ownership transfer proposal
    DropOwnershipProposal {},
    /// Claim contract ownership
    ClaimOwnership {},
    /// Update parameters in the contract configuration
    UpdateConfig { new_max_allocations_amount: Uint128 },
    /// Update a schedule of allocation for specified accounts
    UpdateUnlockSchedules {
        new_unlock_schedules: Vec<(String, Schedule)>,
    },
}

/// This enum describes all the queries available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Config returns the configuration for this contract
    #[returns(Config)]
    Config {},
    /// State returns the state of this contract
    #[returns(State)]
    State {
        // Timestamp at which we query. If none uses current block timestamp
        timestamp: Option<u64>,
    },
    /// Allocation returns the parameters and current status of an allocation
    #[returns(AllocationResponse)]
    Allocation {
        /// Account whose allocation status we query
        account: String,
        // Timestamp at which we query. If none uses current block timestamp
        timestamp: Option<u64>,
    },
    /// Allocations returns a vector that contains builder unlock allocations by specified
    /// parameters
    #[returns(Vec<(String, AllocationParams)>)]
    Allocations {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(Uint128)]
    /// UnlockedTokens returns the unlocked tokens from an allocation
    UnlockedTokens {
        /// Account whose amount of unlocked ASTRO we query for
        account: String,
    },
    /// SimulateWithdraw simulates how many ASTRO will be released if a withdrawal is attempted
    #[returns(SimulateWithdrawResponse)]
    SimulateWithdraw {
        /// Account for which we simulate a withdrawal
        account: String,
        /// Timestamp used to simulate how much ASTRO the account can withdraw
        timestamp: Option<u64>,
    },
}

/// This structure stores the parameters used to return the response when querying for an allocation data.
#[cw_serde]
pub struct AllocationResponse {
    /// The allocation parameters
    pub params: AllocationParams,
    /// The allocation status
    pub status: AllocationStatus,
}

/// This structure stores the parameters used to return a response when simulating a withdrawal.
#[cw_serde]
pub struct SimulateWithdrawResponse {
    /// Amount of ASTRO to receive
    pub astro_to_withdraw: Uint128,
}

/// This structure stores general parameters for the builder unlock contract.
#[cw_serde]
pub struct Config {
    /// Account that can create new unlock schedules
    pub owner: Addr,
    /// ASTRO token denom
    pub astro_denom: String,
    /// Max ASTRO tokens to allocate
    pub max_allocations_amount: Uint128,
}

/// This structure stores the total and the remaining amount of ASTRO to be unlocked by all accounts.
#[cw_serde]
#[derive(Default)]
pub struct State {
    /// Amount of ASTRO tokens deposited into the contract
    pub total_astro_deposited: Uint128,
    /// Currently available ASTRO tokens that still need to be unlocked and/or withdrawn
    pub remaining_astro_tokens: Uint128,
    /// Amount of ASTRO tokens deposited into the contract but not assigned to an allocation
    pub unallocated_astro_tokens: Uint128,
}

/// This structure stores the parameters describing a typical unlock schedule.
#[cw_serde]
#[derive(Default)]
pub struct Schedule {
    /// Timestamp for the start of the unlock schedule (in seconds)
    pub start_time: u64,
    /// Cliff period during which no tokens can be withdrawn out of the contract
    pub cliff: u64,
    /// Time after the cliff during which the remaining tokens linearly unlock
    pub duration: u64,
    /// Percentage of tokens unlocked at the cliff
    pub percent_at_cliff: Option<Decimal>,
}

/// This structure stores the parameters used to describe an ASTRO allocation.
#[cw_serde]
pub struct CreateAllocationParams {
    /// Total amount of ASTRO tokens allocated to a specific account
    pub amount: Uint128,
    /// Parameters controlling the unlocking process
    pub unlock_schedule: Schedule,
}

impl CreateAllocationParams {
    pub fn validate(&self, account: &str) -> StdResult<()> {
        if self.unlock_schedule.cliff >= self.unlock_schedule.duration {
            return Err(StdError::generic_err(format!(
                "The new cliff value must be less than the duration: {} < {}. Account: {account}",
                self.unlock_schedule.cliff, self.unlock_schedule.duration
            )));
        };

        if self.amount.is_zero() {
            return Err(StdError::generic_err(format!(
                "Amount must not be zero. Account: {account}"
            )));
        }

        Ok(())
    }
}

#[cw_serde]
#[derive(Default)]
pub struct AllocationParams {
    /// Parameters controlling the unlocking process
    pub unlock_schedule: Schedule,
    /// Proposed new receiver who will get the ASTRO allocation
    pub proposed_receiver: Option<Addr>,
}

impl AllocationParams {
    pub fn update_schedule(&mut self, new_schedule: Schedule, account: &str) -> StdResult<()> {
        if new_schedule.cliff < self.unlock_schedule.cliff {
            return Err(StdError::generic_err(format!(
                "The new cliff value should be greater than or equal to the old one: {} >= {}. Account error: {}",
                new_schedule.cliff, self.unlock_schedule.cliff, account
            )));
        }

        if new_schedule.start_time < self.unlock_schedule.start_time {
            return Err(StdError::generic_err(format!(
                "The new start time should be later than or equal to the old one: {} >= {}. Account error: {}",
                new_schedule.start_time, self.unlock_schedule.start_time, account
            )));
        }

        if new_schedule.duration < self.unlock_schedule.duration {
            return Err(StdError::generic_err(format!(
                "The new duration value should be greater than or equal to the old one: {} >= {}. Account error: {}",
                new_schedule.duration, self.unlock_schedule.duration, account
            )));
        }

        self.unlock_schedule = new_schedule;
        Ok(())
    }
}

/// This structure stores the parameters used to describe the status of an allocation.
#[cw_serde]
#[derive(Default)]
pub struct AllocationStatus {
    /// Total amount of ASTRO tokens allocated to a specific account
    pub amount: Uint128,
    /// Amount of ASTRO already withdrawn
    pub astro_withdrawn: Uint128,
    /// Already unlocked amount after decreasing
    pub unlocked_amount_checkpoint: Uint128,
}
