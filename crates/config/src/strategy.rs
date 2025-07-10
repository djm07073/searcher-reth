use alloy_primitives::{Address, Bytes, U256};
use reth_revm::state::Bytecode;
use serde::{Deserialize, Serialize};

use crate::gas::GasConfig;

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
}

const ONE_ETHER: u128 = 1_000_000_000_000_000_000;

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
                Bytecode::new_raw_checked(Bytes(config.contract.clone().into())).unwrap()
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
}

/// Configuration for the PathFinder strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PathFinderConfig {
    pub vault: String,
    pub contract: String,
    pub max_liquidity: String,
    /* Maximum liquidity to use for path finding ex. 1000 *
     * 1ether(1000 USDC) */
    pub min_liquidity: String,
    /* Minimum liquidity to use for path finding ex. 100 *
     * 1ether(100 USDC) */
    pub max_profit: String,
    pub min_profit: String,

    // gas configuration
    pub gas_config: GasConfig,
}

// TODO: Add other strategy configurations

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_strategy_functions() {
        let cfg = PathFinderConfig {
            vault: "0x0000000000000000000000000000000000000000".to_string(),
            contract: "00".to_string(),
            max_liquidity: "1000".to_string(), // 1000 USDC
            min_liquidity: "100".to_string(),  // 100 USDC
            max_profit: "100".to_string(),     // 100 USDC
            min_profit: "0.001".to_string(),   // 0.001 USDC
            gas_config: GasConfig {
                priority_fee: 1_000_000_000, // 1 Gwei
                gas_limit: 1_000_000,        // 1 million gas
            },
        };
        let strategy = StrategyConfig::PathFinder(cfg.clone());

        assert_eq!(strategy.get_exex_id(), PATH_FINDER_EXEX_ID);
        assert_eq!(strategy.get_vault(), cfg.vault.parse::<Address>().unwrap());

        let (max, min) = strategy.get_profit_range();
        let expected_max = (((100.0 * 1_000_000.0) as u128) * ONE_ETHER) / 1_000_000u128;
        let expected_min = (((0.001_f64 * 1_000_000.0) as u128) * ONE_ETHER) / 1_000_000u128;
        assert_eq!(max, U256::from(expected_max));
        assert_eq!(min, U256::from(expected_min));

        let (min_liquidity, max_liquidity) = strategy.get_liquidity_range();
        let expected_max_liquidity = U256::from(1000 * ONE_ETHER);
        let expected_min_liquidity = U256::from(100 * ONE_ETHER);

        assert_eq!(min_liquidity, expected_min_liquidity);
        assert_eq!(max_liquidity, expected_max_liquidity);
    }
}
