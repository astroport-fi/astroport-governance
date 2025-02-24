use cosmwasm_std::StdError;
use cw_utils::PaymentError;
use thiserror::Error;

/// This enum describes contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid rewards limit")]
    InvalidRewardsLimit {},

    #[error("Invalid tribute fee amount")]
    InvalidTributeFeeAmount {},

    #[error("Sent insufficient tribute token: {reward}")]
    InsuffiicientTributeToken { reward: String },

    #[error("Tribute fee expected: {fee}")]
    TributeFeeExpected { fee: String },

    #[error("Lp token not whitelisted")]
    LpTokenNotWhitelisted {},

    #[error("Invalid token transfer gas limit")]
    InvalidTokenTransferGasLimit {},

    #[error("Tribute {asset_info} not found on {lp_token}")]
    TributeNotFound {
        lp_token: String,
        asset_info: String,
    },

    #[error("Rewards limit exceeded: {limit}")]
    RewardsLimitExceeded { limit: u8 },
}
