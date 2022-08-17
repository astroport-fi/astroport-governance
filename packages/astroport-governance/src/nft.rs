use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MigrateMsg {}
