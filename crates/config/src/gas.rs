use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GasConfig {
    pub priority_fee: u128,
    pub gas_limit: u64,
}
