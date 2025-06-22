use alloy_primitives::{Address, Bytes, U256};
use reth_revm::state::Bytecode;
use serde::{Deserialize, Serialize};

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
    fn get_contract(&self) -> Bytecode;
    fn get_profit_ratios(&self) -> (U256, U256);
}

const ONE_ETHER: u128 = 1_000_000_000_000_000_000;

impl CommonStrategyConfig for StrategyConfig {
    fn get_exex_id(&self) -> &'static str {
        match self {
            StrategyConfig::PathFinder(_) => PATH_FINDER_EXEX_ID,
            // TODO: Add other strategy configurations
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
                Bytecode::new_raw_checked(Bytes(config.contract.clone().into())).unwrap()
            } // TODO: Add other strategy configurations
        }
    }

    fn get_profit_ratios(&self) -> (U256, U256) {
        match self {
            StrategyConfig::PathFinder(config) => (
                U256::from(
                    (((config.max_profit_ratio.parse::<f64>().unwrap() * 1_000_000.0) as u128)
                        * ONE_ETHER)
                        / 1_000_000,
                ),
                U256::from(
                    (((config.min_profit_ratio.parse::<f64>().unwrap() * 1_000_000.0) as u128)
                        * ONE_ETHER)
                        / 1_000_000,
                ),
            ),
        }
    }
}

/// Configuration for the PathFinder strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PathFinderConfig {
    pub vault: String,
    pub contract: String,
    pub max_profit_ratio: String,
    pub min_profit_ratio: String,
}

// TODO: Add other strategy configurations
