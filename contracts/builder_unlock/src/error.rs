use cosmwasm_std::{Addr, OverflowError, StdError, Uint128};
use cw_utils::PaymentError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("The total allocation for all recipients cannot exceed total ASTRO amount allocated to unlock (currently {0} ASTRO)")]
    TotalAllocationExceedsAmount(Uint128),

    #[error("Insufficient unallocated ASTRO to increase allocation. Contract has: {0} unallocated ASTRO")]
    UnallocatedTokensExceedsTotalDeposited(Uint128),

    #[error("Proposed receiver not set")]
    ProposedReceiverNotSet {},

    #[error("Only contract owner can transfer unallocated ASTRO")]
    UnallocatedTransferUnauthorized {},

    #[error("Insufficient unallocated ASTRO to transfer. Contract has: {0} unallocated ASTRO")]
    InsufficientUnallocatedTokens(Uint128),

    #[error("ASTRO deposit amount mismatch. Expected: {expected}, got: {got}")]
    DepositAmountMismatch { expected: Uint128, got: Uint128 },

    #[error("Allocation (params) already exists for {user}")]
    AllocationExists { user: String },

    #[error("You may not withdraw once you proposed new receiver!")]
    WithdrawErrorWhenProposedReceiver {},

    #[error("No unlocked ASTRO to be withdrawn")]
    NoUnlockedAstro {},

    #[error("Proposed receiver already set to {proposed_receiver}")]
    ProposedReceiverAlreadySet { proposed_receiver: Addr },

    #[error("Invalid new_receiver. Proposed receiver already has an ASTRO allocation")]
    ProposedReceiverAlreadyHasAllocation {},

    #[error("Only the contract owner can decrease allocations")]
    UnauthorizedDecreaseAllocation {},

    #[error(
        "Insufficient amount of lock to decrease allocation, user has locked {locked_amount} ASTRO"
    )]
    InsufficientLockedAmount { locked_amount: Uint128 },

    #[error("Proposed receiver is either not set or doesn't match the message sender")]
    ProposedReceiverMismatch {},

    #[error("{address} doesn't have allocation")]
    NoAllocation { address: String },
}
