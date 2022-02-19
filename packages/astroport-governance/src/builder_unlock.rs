use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// # Description
/// This structure stores general parameters for the builder unlock contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Account that can create new unlock schedules
    pub owner: Addr,
    /// Address of ASTRO token
    pub astro_token: Addr,
}

/// # Description
/// This structure stores the total and the remaining amount of ASTRO to be unlocked by all accounts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    /// Amount of ASTRO tokens deposited into the contract
    pub total_astro_deposited: Uint128,
    /// Currently available ASTRO tokens that still need to be unlocked and/or withdrawn
    pub remaining_astro_tokens: Uint128,
}

impl Default for State {
    fn default() -> Self {
        State {
            total_astro_deposited: Uint128::zero(),
            remaining_astro_tokens: Uint128::zero(),
        }
    }
}

/// # Description
/// This structure stores the parameters describing a typical unlock schedule.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Schedule {
    /// Timestamp for the start of the unlock schedule (in seconds)
    pub start_time: u64,
    /// Cliff period during which no tokens can be withdrawn out of the contract
    pub cliff: u64,
    /// Time after the cliff during which the remaining tokens linearly unlock
    pub duration: u64,
}

/// # Description
/// This structure stores the parameters used to describe an ASTRO allocation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AllocationParams {
    /// Total amount of ASTRO tokens allocated to a specific account
    pub amount: Uint128,
    /// Parameters controlling the unlocking process
    pub unlock_schedule: Schedule,
    /// Proposed new receiver who will get the ASTRO allocation
    pub proposed_receiver: Option<Addr>,
}

impl Default for AllocationParams {
    fn default() -> Self {
        AllocationParams {
            amount: Uint128::zero(),
            unlock_schedule: Schedule {
                start_time: 0u64,
                cliff: 0u64,
                duration: 0u64,
            },
            proposed_receiver: None,
        }
    }
}

/// # Description
/// This structure stores the parameters used to describe the status of an allocation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AllocationStatus {
    /// Amount of ASTRO already withdrawn
    pub astro_withdrawn: Uint128,
}

impl Default for AllocationStatus {
    fn default() -> Self {
        AllocationStatus {
            astro_withdrawn: Uint128::zero(),
        }
    }
}

impl AllocationStatus {
    pub const fn new() -> Self {
        Self {
            astro_withdrawn: Uint128::zero(),
        }
    }
}

pub mod msg {
    use cosmwasm_std::Uint128;
    use cw20::Cw20ReceiveMsg;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use super::{AllocationParams, AllocationStatus, Config};

    /// # Description
    /// This structure holds the initial parameters used to instantiate the contract.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct InstantiateMsg {
        /// Account that can create new allocations
        pub owner: String,
        /// ASTRO token address
        pub astro_token: String,
    }

    /// # Description
    /// This enum describes all execute functions available in the contract.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum ExecuteMsg {
        /// Implementation for the cw20 receive msg
        Receive(Cw20ReceiveMsg),
        /// Claim withdrawable ASTRO
        Withdraw {},
        /// Transfer contract ownership
        TransferOwnership { new_owner: Option<String> },
        /// Allows a user to change the receiver address for their ASTRO allocation
        ProposeNewReceiver { new_receiver: String },
        /// Allows a user to remove the previously proposed new receiver for their ASTRO allocation
        DropNewReceiver {},
        /// Allows newly proposed receivers to claim ASTRO allocations ownership
        ClaimReceiver { prev_receiver: String },
    }

    /// # Description
    /// This enum describes the receive msg templates.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum ReceiveMsg {
        /// Create new ASTRO allocations
        CreateAllocations {
            allocations: Vec<(String, AllocationParams)>,
        },
    }

    /// # Description
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

    /// # Description
    /// This structure stores the parameters used to return the response when querying for an allocation data.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct AllocationResponse {
        /// The allocation parameters
        pub params: AllocationParams,
        /// The allocation status
        pub status: AllocationStatus,
    }

    /// # Description
    /// This structure stores the parameters used to return the response when simulating a withdrawal.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct SimulateWithdrawResponse {
        /// Amount of ASTRO to receive
        pub astro_to_withdraw: Uint128,
    }

    /// # Description
    /// This structure stores the parameters used to return the response when querying for the contract state.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct StateResponse {
        /// ASTRO tokens deposited into the contract and that are meant to unlock
        pub total_astro_deposited: Uint128,
        /// Currently available ASTRO tokens that weren't yet withdrawn from the contract
        pub remaining_astro_tokens: Uint128,
    }
}
