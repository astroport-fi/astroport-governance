use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure stores general parameters for the builder unlock contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Account that can create new unlock schedules
    pub owner: Addr,
    /// Address of ASTRO token
    pub astro_token: Addr,
    /// Max ASTRO tokens to allocate
    pub max_allocations_amount: Uint128,
}

/// This structure stores the total and the remaining amount of ASTRO to be unlocked by all accounts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct State {
    /// Amount of ASTRO tokens deposited into the contract
    pub total_astro_deposited: Uint128,
    /// Currently available ASTRO tokens that still need to be unlocked and/or withdrawn
    pub remaining_astro_tokens: Uint128,
    /// Amount of ASTRO tokens deposited into contract but without allocation
    pub unallocated_tokens: Uint128,
}

/// This structure stores the parameters describing a typical unlock schedule.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct Schedule {
    /// Timestamp for the start of the unlock schedule (in seconds)
    pub start_time: u64,
    /// Cliff period during which no tokens can be withdrawn out of the contract
    pub cliff: u64,
    /// Time after the cliff during which the remaining tokens linearly unlock
    pub duration: u64,
}

/// This structure stores the parameters used to describe an ASTRO allocation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct AllocationParams {
    /// Total amount of ASTRO tokens allocated to a specific account
    pub amount: Uint128,
    /// Parameters controlling the unlocking process
    pub unlock_schedule: Schedule,
    /// Proposed new receiver who will get the ASTRO allocation
    pub proposed_receiver: Option<Addr>,
}

/// This structure stores the parameters used to describe the status of an allocation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct AllocationStatus {
    /// Amount of ASTRO already withdrawn
    pub astro_withdrawn: Uint128,
    /// Already unlocked amount after decreasing
    pub unlocked_amount_checkpoint: Uint128,
}

impl AllocationStatus {
    pub const fn new() -> Self {
        Self {
            astro_withdrawn: Uint128::zero(),
            unlocked_amount_checkpoint: Uint128::zero(),
        }
    }
}

pub mod msg {
    use cosmwasm_std::Uint128;
    use cw20::Cw20ReceiveMsg;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use super::{AllocationParams, AllocationStatus, Config};

    /// This structure holds the initial parameters used to instantiate the contract.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct InstantiateMsg {
        /// Account that can create new allocations
        pub owner: String,
        /// ASTRO token address
        pub astro_token: String,
        /// Max ASTRO tokens to allocate
        pub max_allocations_amount: Uint128,
    }

    /// This enum describes all execute functions available in the contract.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum ExecuteMsg {
        /// Implementation for the cw20 receive msg
        Receive(Cw20ReceiveMsg),
        /// Claim withdrawable ASTRO
        Withdraw {},
        /// Allows a user to change the receiver address for their ASTRO allocation
        ProposeNewReceiver { new_receiver: String },
        /// Allows a user to remove the previously proposed new receiver for their ASTRO allocation
        DropNewReceiver {},
        /// Allows newly proposed receivers to claim ASTRO allocations ownership
        ClaimReceiver { prev_receiver: String },
        /// Increase ASTRO allocation for receiver
        IncreaseAllocation { receiver: String, amount: Uint128 },
        /// Decrease ASTRO allocation of the receiver
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
    }

    /// This enum describes the receive msg templates.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum ReceiveMsg {
        /// Create new ASTRO allocations
        CreateAllocations {
            allocations: Vec<(String, AllocationParams)>,
        },
        /// Increase ASTRO allocation
        IncreaseAllocation { user: String, amount: Uint128 },
    }

    /// Thie enum describes all the queries available in the contract.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum QueryMsg {
        // Config for this contract
        Config {},
        // State of this contract
        State {},
        // Parameters and current status of an allocation
        Allocation {
            /// Account whose allocation status we query
            account: String,
        },
        // Unlocked tokens from an allocation
        UnlockedTokens {
            /// Account whose amount of unlocked ASTRO we query for
            account: String,
        },
        // Simulate how many ASTRO will be released if a withdrawal is attempted
        SimulateWithdraw {
            /// Account for which we simulate a withdrawal
            account: String,
            /// Timestmpa used to simulate how many ASTRO the account can withdraw
            timestamp: Option<u64>,
        },
    }

    pub type ConfigResponse = Config;

    /// This structure stores the parameters used to return the response when querying for an allocation data.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct AllocationResponse {
        /// The allocation parameters
        pub params: AllocationParams,
        /// The allocation status
        pub status: AllocationStatus,
    }

    /// This structure stores the parameters used to return the response when simulating a withdrawal.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct SimulateWithdrawResponse {
        /// Amount of ASTRO to receive
        pub astro_to_withdraw: Uint128,
    }

    /// This structure stores the parameters used to return the response when querying for the contract state.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct StateResponse {
        /// ASTRO tokens deposited into the contract and that are meant to unlock
        pub total_astro_deposited: Uint128,
        /// Currently available ASTRO tokens that weren't yet withdrawn from the contract
        pub remaining_astro_tokens: Uint128,
        /// Currently available ASTRO tokens to withdraw or increase allocations by the owner
        pub unallocated_astro_tokens: Uint128,
    }
}
