use alloy_primitives::{Address, U256, hex};
use reth_revm::state::Bytecode;
use reth_tracing::tracing;
use serde::{Deserialize, Serialize};

use crate::{gas::GasConfig, strategy::path_finder::PathFinderConfig, types::Candidate};

// Strategy configuration
pub const PATH_FINDER_EXEX_ID: &str = "path-finder";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum StrategyConfig {
    #[serde(rename = "path-finder")]
    PathFinder(PathFinderConfig),
    // #[serde(rename = "liquidation")] Liquidation(LiquidationConfig),
    // #[serde(rename = "arbitrage")] Arbitrage(ArbitrageConfig),
}

pub trait CommonStrategyConfig {
    fn get_exex_id(&self) -> &'static str;
    fn get_vault(&self) -> Address;
    fn get_liquidity_range(&self) -> (U256, U256);
    fn get_contract(&self) -> Bytecode;
    fn get_profit_range(&self) -> (U256, U256);
    fn get_gas_config(&self) -> GasConfig;
    fn load_candidates(&self, chain_id: u64) -> eyre::Result<Vec<Candidate>>;
}

pub const ONE_ETHER: u128 = 1_000_000_000_000_000_000;

impl CommonStrategyConfig for StrategyConfig {
    fn get_exex_id(&self) -> &'static str {
        match self {
            StrategyConfig::PathFinder(_) => PATH_FINDER_EXEX_ID,
            // TODO: Add other strategy configurations
        }
    }

    fn get_liquidity_range(&self) -> (U256, U256) {
        match self {
            StrategyConfig::PathFinder(config) => (
                U256::from(config.min_liquidity.parse::<u128>().unwrap() * ONE_ETHER),
                U256::from(config.max_liquidity.parse::<u128>().unwrap() * ONE_ETHER),
            ),
        }
    }

    fn get_vault(&self) -> Address {
        match self {
            StrategyConfig::PathFinder(config) => config.vault.parse().unwrap(),
            // TODO: Add other strategy configurations
        }
    }

    fn get_contract(&self) -> Bytecode {
        match self {
            StrategyConfig::PathFinder(config) => {
                tracing::info!("Loading strategy contract for PathFinder: {:?}", config.contract);
                let bytes = match hex::decode(config.contract.clone()) {
                    Ok(vec) => alloy_primitives::Bytes::from(vec),
                    Err(e) => {
                        tracing::error!("Failed to decode contract hex: {}", e);
                        return Bytecode::default();
                    }
                };
                match Bytecode::new_raw_checked(bytes) {
                    Ok(code) => {
                        tracing::info!("Loaded strategy contract: {:?}", code);
                        code
                    }
                    Err(e) => {
                        tracing::error!("Failed to load strategy contract: {}", e);
                        Bytecode::default() // Return an empty bytecode on error
                    }
                }
            } // TODO: Add other strategy configurations
        }
    }

    fn get_profit_range(&self) -> (U256, U256) {
        match self {
            StrategyConfig::PathFinder(config) => {
                let max_profit = config.max_profit.parse::<f64>().unwrap();
                let min_profit = config.min_profit.parse::<f64>().unwrap();
                (
                    U256::from((max_profit * (ONE_ETHER as f64)) as u128),
                    U256::from((min_profit * (ONE_ETHER as f64)) as u128),
                )
            } // TODO: Add other strategy configurations
        }
    }

    fn get_gas_config(&self) -> GasConfig {
        match self {
            StrategyConfig::PathFinder(config) => config.gas_config.clone(),
        }
    }

    fn load_candidates(&self, chain_id: u64) -> eyre::Result<Vec<Candidate>> {
        match self {
            StrategyConfig::PathFinder(config) => config.load_candidates(chain_id),
        }
    }
}
