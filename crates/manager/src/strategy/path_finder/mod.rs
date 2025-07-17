use std::{ collections::HashMap, fs::File, io::BufReader, path::PathBuf };

use eyre::eyre;
use serde::{ Deserialize, Serialize };

use crate::{ gas::GasConfig, types::CalldataCandidate };

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
    pub fn load_candidates(&self, chain_id: u64) -> eyre::Result<Vec<CalldataCandidate>> {
        if !self.path.exists() {
            return Err(eyre!("Routes JSON file not found at: {}", self.path.display()));
        }

        let file = File::open(&self.path)?;
        let routes_data: HashMap<String, Vec<CalldataCandidate>> = serde_json
            ::from_reader(BufReader::new(file))
            .map_err(|e| eyre!("Failed to parse routes JSON: {}", e))?;

        let chain_routes = routes_data
            .get(&chain_id.to_string())
            .ok_or_else(|| eyre!("No routes found for chain_id: {}", chain_id))?;

        Ok(chain_routes.clone())
    }
}
