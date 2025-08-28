use crate::{gas::GasConfig, types::ProcessorEntry};
use eyre::eyre;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs::File, io::BufReader, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LiquidatorConfig {
    pub vault: String,
    pub contract: String,
    pub gas_config: GasConfig,
    pub path: PathBuf,

    pub max_liquidity: String,
    /* Maximum liquidity to use for path finding ex. 1000 *
     * 1ether(1000 USDC) */
    pub min_liquidity: String,
    /* Minimum liquidity to use for path finding ex. 100 *
     * 1ether(100 USDC) */
    pub max_profit: String,
    pub min_profit: String,
}

// {
//     "1": [  // chain_id = 1
//       {
//         "table": "aave_execute_borrow",
//         "processor": "AaveExecuteBorrow"
//       },
//       {
//         "table": "another_table",
//         "processor": "AnotherProcessor"
//       }
//     ],
//     "42161": [  // chain_id = 42161(Arbitrum)
//       {
//         "table": "dolomite_borrow",
//         "processor": "DolomiteBorrow"
//       }
//     ]
//   }

impl LiquidatorConfig {
    pub fn load_processors(&self, chain_id: u64) -> eyre::Result<Vec<ProcessorEntry>> {
        if !self.path.exists() {
            return Err(eyre!("Routes JSON file not found at: {}", self.path.display()));
        }

        let file = File::open(&self.path)?;
        let processors: HashMap<String, Vec<ProcessorEntry>> =
            serde_json::from_reader(BufReader::new(file))
                .map_err(|e| eyre!("Failed to parse routes JSON: {}", e))?;

        let chain_processors = processors
            .get(&chain_id.to_string())
            .ok_or_else(|| eyre!("No processors found for chain_id: {}", chain_id))?;

        Ok(chain_processors.clone())
    }
}
