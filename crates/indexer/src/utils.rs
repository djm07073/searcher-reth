// use alloy_eips::eip1559::ETHEREUM_BLOCK_GAS_LIMIT;
// use reth::builder::NodeTypes;
// use reth_node_api::FullNodeComponents;
// use reth_rpc::{EthApi, TraceApi};
// use reth_rpc_eth_api::helpers::{Call, LoadPendingBlock};
// use reth_rpc_eth_types::{EthStateCache, GasPriceOracle, FeeHistoryCache, FeeHistoryCacheConfig};
// use reth_rpc_server_types::constants::{
//     DEFAULT_ETH_PROOF_WINDOW,
//     DEFAULT_MAX_SIMULATE_BLOCKS,
//     DEFAULT_PROOF_PERMITS,
// };
// use reth_tasks::pool::{BlockingTaskGuard, BlockingTaskPool};
use reth_tracing::tracing::info;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, collections::HashMap, env};
use rocksdb::{DB, Options, ColumnFamilyDescriptor, ColumnFamily, DBWithThreadMode, SingleThreaded};
use std::collections::HashSet;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub enabled_events: HashMap<String, bool>,
    #[serde(flatten)]
    pub dataset_configs: HashMap<String, serde_yaml::Value>,
}

impl Config {
    pub fn load() -> eyre::Result<Self> {
        // The path is relative to this file
        let config_str = include_str!("../config.yaml");
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

// Create a trace API instance with all the required components and trait bounds
// pub fn create_trace_api<Node>(
//     provider: Node::Provider,
//     evm_config: Node::Evm,
//     pool: Node::Pool,
//     network: Node::Network,
// ) -> Arc<TraceApi<EthApi<Node::Provider, Node::Pool, Node::Network, Node::Evm>>>
// where
//     Node: FullNodeComponents,
//     Node::Types: NodeTypes,
//     EthApi<Node::Provider, Node::Pool, Node::Network, Node::Evm>: Call + LoadPendingBlock,
// {
//     let cache = EthStateCache::spawn(
//         provider.clone(),
//         Default::default(),
//     );

//     let fee_history_cache = FeeHistoryCache::new(FeeHistoryCacheConfig::default());

//     let gas_oracle = GasPriceOracle::new(
//         provider.clone(),
//         Default::default(),
//         cache.clone()
//     );

//     let eth_api = EthApi::new(
//         provider.clone(),
//         pool,
//         network,
//         cache.clone(),
//         gas_oracle,
//         ETHEREUM_BLOCK_GAS_LIMIT,
//         DEFAULT_MAX_SIMULATE_BLOCKS,
//         DEFAULT_ETH_PROOF_WINDOW,
//         BlockingTaskPool::build().expect("failed to build tracing pool"),
//         fee_history_cache,
//         evm_config.clone(),
//         DEFAULT_PROOF_PERMITS,
//     );

//     Arc::new(TraceApi::new(
//         eth_api,
//         BlockingTaskGuard::new(10),
//     ))
// }

// pub(crate) fn create_eth_api<Node>(
//     provider: Node::Provider,
//     evm_config: Node::Evm,
//     pool: Node::Pool,
//     network: Node::Network,
// ) -> Arc<EthApi<Node::Provider, Node::Pool, Node::Network, Node::Evm>>
// where
//     Node: FullNodeComponents,
//     Node::Types: NodeTypes,
//     EthApi<Node::Provider, Node::Pool, Node::Network, Node::Evm>: Call + LoadPendingBlock,
// {
//     let cache = EthStateCache::spawn(
//         provider.clone(),
//         Default::default(),
//     );

//     let fee_history_cache = FeeHistoryCache::new(FeeHistoryCacheConfig::default());

//     let gas_oracle = GasPriceOracle::new(
//         provider.clone(),
//         Default::default(),
//         cache.clone()
//     );

//     Arc::new(EthApi::new(
//         provider.clone(),
//         pool.clone(),
//         network.clone(),
//         cache.clone(),
//         gas_oracle,
//         ETHEREUM_BLOCK_GAS_LIMIT,
//         DEFAULT_MAX_SIMULATE_BLOCKS,
//         DEFAULT_ETH_PROOF_WINDOW,
//         BlockingTaskPool::build().expect("failed to build tracing pool"),
//         fee_history_cache,
//         evm_config.clone(),
//         DEFAULT_PROOF_PERMITS,
//     ))
// }