use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// T = String (unchecked) or Addr (checked)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Account who can create new allocations
    pub owner: Addr,
    /// Address of ASTRO token
    pub astro_token: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    /// ASTRO Tokens deposited into the contract
    pub total_astro_deposited: Uint128,
    /// Currently available ASTRO Tokens
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

// Parameters describing a typical unlocking schedule
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Schedule {
    /// Timestamp of when unlocking is to be started (in seconds)
    pub start_time: u64,
    /// Number of seconds starting unlocking during which no tokens can be withdrawn
    pub cliff: u64,
    /// Number of seconds taken since unlocking beginning for tokens to be fully unlocked
    pub duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AllocationParams {
    /// Total amount of ASTRO token allocated to this account
    pub amount: Uint128,
    /// Parameters controlling the unlocking process
    pub unlock_schedule: Schedule,
    /// proposed new_receiver who will get the allocation
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

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct InstantiateMsg {
        /// Account who can create new allocations
        pub owner: String,
        /// Address of ASTRO token
        pub astro_token: String,
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum ExecuteMsg {
        /// Implementation of cw20 receive msg
        Receive(Cw20ReceiveMsg),
        /// Claim withdrawable ASTRO
        Withdraw {},
        /// Update addresses of owner
        TransferOwnership { new_owner: Option<String> },
        /// Allows users to change the receiver address of their allocations etc
        ProposeNewReceiver { new_receiver: String },
        /// Allows users to remove the previously proposed new receiver for their allocations
        DropNewReceiver {},
        /// Allows new receivers to claim the allocations
        ClaimReceiver { prev_receiver: String },
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum ReceiveMsg {
        /// Create new allocations
        CreateAllocations {
            allocations: Vec<(String, AllocationParams)>,
        },
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum QueryMsg {
        // Config of this contract
        Config {},
        // State of this contract
        State {},
        // Parameters and current status of an allocation
        Allocation {
            account: String,
        },
        // Tokens unlocked for an allocation (may not be withdrawable because of cliff)
        UnlockedTokens {
            account: String,
        },
        // Simulate how many ASTRO will be released if a withdrawal is attempted
        SimulateWithdraw {
            account: String,
            timestamp: Option<u64>,
        },
    }

    pub type ConfigResponse = Config;

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct AllocationResponse {
        pub params: AllocationParams,
        pub status: AllocationStatus,
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct SimulateWithdrawResponse {
        /// Amount of ASTRO to receive
        pub astro_to_withdraw: Uint128,
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct StateResponse {
        /// ASTRO Tokens deposited into the contract
        pub total_astro_deposited: Uint128,
        /// Currently available ASTRO Tokens
        pub remaining_astro_tokens: Uint128,
    }
}
