use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// contract owner
    pub owner: String,
    /// the vxASTRO token contract address
    pub escrow_addr: String,
    /// generator contract address
    pub generator_addr: String,
    /// a max number of pools that can receive an ASTRO allocation
    pub pools_limit: u64,
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Vote { votes: Vec<(String, u16)> },
    GaugePools {},
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    UserInfo { user: String },
    GaugeInfo,
    Config,
    PoolInfo { pool_addr: String },
    PoolInfoAtPeriod { pool_addr: String, period: u64 },
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
