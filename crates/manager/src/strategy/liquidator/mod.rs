use serde::{Deserialize, Serialize};
use std::{collections::HashMap};
use rocksdb::{DB, Options, ColumnFamilyDescriptor, ColumnFamily, DBWithThreadMode, SingleThreaded};
use crate::gas::GasConfig;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LiquidatorConfig {
    pub vault: String,
    pub contract: String,

    // gas configuration
    pub gas_config: GasConfig,

    pub max_profit: String,
    pub min_profit: String,


    pub enabled_events: HashMap<String, bool>,
    #[serde(flatten)]
    pub dataset_configs: HashMap<String, serde_yaml::Value>,
}

impl LiquidatorConfig {
    pub fn load() -> eyre::Result<Self> {
        // The path is relative to this file
        // TODO : replace it with toml
        let config_str = include_str!("./config.yaml");
        let config = serde_yaml::from_str(config_str)?;
        Ok(config)
    }

    pub fn is_event_enabled(&self, event_name: &str) -> bool {
        self.enabled_events.get(event_name).cloned().unwrap_or(true)
    }

    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> eyre::Result<T> {
        self.dataset_configs.get(key)
            .ok_or_else(|| eyre::eyre!("No config found for {}", key))
            .and_then(|v| serde_yaml::from_value(v.clone())
                .map_err(|e| eyre::eyre!("Failed to parse config for {}: {}", key, e)))
    }
}