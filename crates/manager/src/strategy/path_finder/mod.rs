pub mod types;

use std::{collections::HashMap, fs::File, io::BufReader, path::PathBuf};

use alloy_primitives::hex;
use eyre::eyre;
use serde::{Deserialize, Serialize};

use crate::{
    gas::GasConfig,
    strategy::path_finder::types::{Route, RouteElement, RoutesMap},
    types::Candidate,
};

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
        let routes_data: RoutesMap = serde_json::from_reader(BufReader::new(file))
            .map_err(|e| eyre!("Failed to parse routes JSON: {}", e))?;

        let chain_routes = routes_data
            .get(&chain_id.to_string())
            .ok_or_else(|| eyre!("No routes found for chain_id: {}", chain_id))?;

        // Save the loaded chain_routes to a new JSON file for verification
        let output_path = format!("loaded_routes_{}.json", chain_id);
        let output_file = File::create(&output_path)
            .map_err(|e| eyre!("Failed to create output file {}: {}", output_path, e))?;
        serde_json::to_writer_pretty(output_file, chain_routes)
            .map_err(|e| eyre!("Failed to write routes to {}: {}", output_path, e))?;

        self.build_cyclic_paths(chain_routes)
    }

    fn build_cyclic_paths(&self, chain_routes: &Route) -> eyre::Result<Vec<Candidate>> {
        let token_map: HashMap<&String, Vec<&RouteElement>> =
            chain_routes.elements.iter().fold(HashMap::new(), |mut acc, element| {
                acc.entry(&element.src_token).or_default().push(element);
                acc
            });

        let mut candidates = Vec::new();
        let parse_hex = |data: &str| -> eyre::Result<Vec<u8>> {
            let hex_str = data.strip_prefix("0x").unwrap_or(data);
            hex::decode(hex_str).map_err(|e| eyre!("Decode error: {}", e))
        };

        for initial_token in &chain_routes.initial_tokens {
            if let Some(first_hops) = token_map.get(initial_token) {
                for first_hop in first_hops {
                    let first_encoded = parse_hex(&first_hop.encoded_data)?;

                    if let Some(second_hops) = token_map.get(&first_hop.dst_token) {
                        for second_hop in second_hops {
                            let second_encoded = parse_hex(&second_hop.encoded_data)?;

                            if second_hop.dst_token == *initial_token {
                                // For 2-hop paths, filter out if dex_type and metadata are the
                                // same.
                                if first_hop.dex_type == second_hop.dex_type
                                    && first_hop.metadata == second_hop.metadata
                                {
                                    continue;
                                }
                                candidates
                                    .push(vec![first_encoded.clone(), second_encoded.clone()]);
                            }

                            if let Some(third_hops) = token_map.get(&second_hop.dst_token) {
                                for third_hop in third_hops {
                                    if third_hop.dst_token == *initial_token {
                                        let third_encoded = parse_hex(&third_hop.encoded_data)?;
                                        candidates.push(vec![
                                            first_encoded.clone(),
                                            second_encoded.clone(),
                                            third_encoded,
                                        ]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, U256};

    use crate::common::{CommonStrategyConfig, ONE_ETHER, PATH_FINDER_EXEX_ID, StrategyConfig};

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

    #[test]
    fn test_build_cyclic_paths() {
        let config = PathFinderConfig {
            vault: String::new(),
            contract: String::new(),
            max_liquidity: String::new(),
            min_liquidity: String::new(),
            max_profit: String::new(),
            min_profit: String::new(),
            gas_config: GasConfig { priority_fee: 0, gas_limit: 0 },
            path: PathBuf::new(),
        };

        let token_a = "TOKEN_A".to_string();
        let token_b = "TOKEN_B".to_string();
        let token_c = "TOKEN_C".to_string();

        let chain_routes = Route {
            initial_tokens: vec![token_a.clone(), token_b.clone()],
            elements: vec![
                RouteElement {
                    src_token: token_a.clone(),
                    dst_token: token_b.clone(),
                    encoded_data: "0xab".to_string(), // A -> B
                    address: String::new(),
                    dex_type: 1,
                    metadata: String::new(),
                },
                RouteElement {
                    src_token: token_b.clone(),
                    dst_token: token_a.clone(),
                    encoded_data: "0xba".to_string(), // B -> A
                    address: String::new(),
                    dex_type: 1,
                    metadata: String::new(),
                },
                RouteElement {
                    src_token: token_b.clone(),
                    dst_token: token_a.clone(),
                    encoded_data: "0xbada".to_string(), // B -> A with metadata
                    address: String::new(),
                    dex_type: 1,
                    metadata: "metadata".to_string(),
                },
                RouteElement {
                    src_token: token_b.clone(),
                    dst_token: token_c.clone(),
                    encoded_data: "0xbc".to_string(), // B -> C
                    address: String::new(),
                    dex_type: 2,
                    metadata: String::new(),
                },
                RouteElement {
                    src_token: token_c.clone(),
                    dst_token: token_a.clone(),
                    encoded_data: "0xca".to_string(), // C -> A
                    address: String::new(),
                    dex_type: 3,
                    metadata: String::new(),
                },
                RouteElement {
                    src_token: token_a.clone(),
                    dst_token: token_c.clone(),
                    encoded_data: "0xac".to_string(),
                    address: String::new(),
                    dex_type: 1,
                    metadata: String::new(),
                },
            ],
        };

        let candidates = config.build_cyclic_paths(&chain_routes).unwrap();
        assert_eq!(candidates.len(), 5, "Should find three 2-hop and two 3-hop paths");

        let expected_3_hop_path_abca: Vec<Vec<u8>> = vec![
            hex::decode("ab").unwrap(),
            hex::decode("bc").unwrap(),
            hex::decode("ca").unwrap(),
        ];
        let expected_2_hop_path_aca: Vec<Vec<u8>> =
            vec![hex::decode("ac").unwrap(), hex::decode("ca").unwrap()];
        let expected_3_hop_path_bcab: Vec<Vec<u8>> = vec![
            hex::decode("bc").unwrap(),
            hex::decode("ca").unwrap(),
            hex::decode("ab").unwrap(),
        ];
        let expected_2_hop_path_ab_bada: Vec<Vec<u8>> =
            vec![hex::decode("ab").unwrap(), hex::decode("bada").unwrap()];
        let expected_2_hop_path_bada_ab: Vec<Vec<u8>> =
            vec![hex::decode("bada").unwrap(), hex::decode("ab").unwrap()];

        assert!(
            candidates.contains(&expected_3_hop_path_abca),
            "Did not find the 3-hop path A->B->C->A"
        );
        assert!(
            candidates.contains(&expected_2_hop_path_aca),
            "Did not find the 2-hop path A->C->A"
        );
        assert!(
            candidates.contains(&expected_3_hop_path_bcab),
            "Did not find the 3-hop path B->C->A->B"
        );
        assert!(
            candidates.contains(&expected_2_hop_path_ab_bada),
            "Did not find the 2-hop path A->B->A with metadata"
        );
        assert!(
            candidates.contains(&expected_2_hop_path_bada_ab),
            "Did not find the 2-hop path B->A->B with metadata"
        );
    }
}
