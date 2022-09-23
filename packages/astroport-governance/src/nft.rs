use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes a migration message.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MigrateMsg {}
