pub mod types;

use std::{ collections::HashMap, fs::File, io::BufReader, path::PathBuf };

use alloy_primitives::hex;
use eyre::eyre;
use serde::{ Deserialize, Serialize };

use crate::{ gas::GasConfig, strategy::path_finder::types::{ Route, RouteElement, RoutesMap }, types::Candidate };


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

    // data source
    pub path: PathBuf,
}

impl PathFinderConfig {
    pub fn load_candidates(&self, chain_id: u64) -> eyre::Result<Vec<Candidate>> {
        if !self.path.exists() {
            return Err(eyre!("Routes JSON file not found at: {}", self.path.display()));
        }

        let file = File::open(&self.path)?;
        let routes_data: RoutesMap = serde_json
            ::from_reader(BufReader::new(file))
            .map_err(|e| eyre!("Failed to parse routes JSON: {}", e))?;

        let chain_routes = routes_data
            .get(&chain_id.to_string())
            .ok_or_else(|| eyre!("No routes found for chain_id: {}", chain_id))?;

        self.build_all_paths(chain_routes)
    }

    fn build_all_paths(&self, chain_routes: &Route) -> eyre::Result<Vec<Candidate>> {
        let token_map: HashMap<&String, Vec<&RouteElement>> = chain_routes.elements
            .iter()
            .fold(HashMap::new(), |mut acc, element| {
                acc.entry(&element.src_token).or_default().push(element);
                acc
            });

        let mut candidates = Vec::new();

        for initial_token in &chain_routes.initial_tokens {
            if let Some(first_hops) = token_map.get(initial_token) {
                for first_hop in first_hops {
                    let parse_hex = |data: &str| -> eyre::Result<Vec<u8>> {
                        let hex_str = data.strip_prefix("0x").unwrap_or(data);
                        hex::decode(hex_str).map_err(|e| eyre!("Decode error: {}", e))
                    };

                    let first_encoded = parse_hex(&first_hop.encoded_data)?;

                    candidates.push(vec![first_encoded.clone()]);

                    if let Some(second_hops) = token_map.get(&first_hop.dst_token) {
                        for second_hop in second_hops {
                            if second_hop.dst_token != *initial_token {
                                let second_encoded = parse_hex(&second_hop.encoded_data)?;
                                candidates.push(vec![first_encoded.clone(), second_encoded]);
                            }
                        }
                    }
                }
            }
        }

        Ok(candidates)
    }
}

// TODO: Add other strategy configurations

#[cfg(test)]
mod tests {
    use alloy_primitives::{ Address, U256 };

    use crate::common::{ CommonStrategyConfig, StrategyConfig, ONE_ETHER, PATH_FINDER_EXEX_ID };

    use super::*;

    #[test]
    fn test_common_strategy_functions() {
        let cfg = PathFinderConfig {
            vault: "0x0000000000000000000000000000000000000000".to_string(),
            contract: "00".to_string(),
            max_liquidity: "1000".to_string(), // 1000 USDC
            min_liquidity: "100".to_string(), // 100 USDC
            max_profit: "100".to_string(), // 100 USDC
            min_profit: "0.001".to_string(), // 0.001 USDC
            gas_config: GasConfig {
                priority_fee: 1_000_000_000, // 1 Gwei
                gas_limit: 1_000_000, // 1 million gas
            },
            path: PathBuf::from("path/to/data"),
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
