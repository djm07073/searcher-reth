use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use alloy_network::EthereumWallet;
use alloy_primitives::{hex, Address};
use alloy_signer_local::{coins_bip39::English, MnemonicBuilder};
use eyre::{eyre, Result};
use serde::{Deserialize, Serialize};

use crate::{
    strategy::StrategyConfig,
    types::{Candidate, Route, RouteElement, RoutesMap},
};

pub struct ConfigManager {
    /// Path to the configuration file. Used for reloads
    config_path: PathBuf,
    /// Configuration loaded from `config_path`
    config: SearcherConfig,
    /// Flag indicating if the configuration has changed since the last reload
    config_changed: Arc<AtomicBool>,
}

impl ConfigManager {
    pub fn from_file(path: &str) -> eyre::Result<Self> {
        let config = SearcherConfig::from_file(path)?;
        Ok(Self {
            config_path: PathBuf::from(path),
            config,
            config_changed: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn reload(&mut self) -> eyre::Result<()> {
        let new_config = SearcherConfig::from_file(self.config_path.to_str().unwrap())?;
        self.config = new_config;
        self.config_changed.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn get_wallet(&self) -> Result<(EthereumWallet, Vec<Address>)> {
        self.config.relayer.get_wallet()
    }

    pub fn get_strategy(&self, exex_id: &str) -> eyre::Result<StrategyConfig> {
        self.config
            .strategies
            .get(exex_id)
            .cloned()
            .ok_or_else(|| eyre::eyre!("Strategy not found for exex_id: {}", exex_id))
    }

}

impl SearcherConfig {
    fn from_file(path: &str) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}

type ExExId = String;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SearcherConfig {
    pub relayer: TxRelayerConfig,
    pub logging: LoggingConfig,
    #[serde(default, rename = "strategy")]
    pub strategies: HashMap<ExExId, StrategyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TxRelayerConfig {
    pub mnemonic: String,
}

impl TxRelayerConfig {
    pub fn get_wallet(&self) -> Result<(EthereumWallet, Vec<Address>)> {
        let mut wallet = EthereumWallet::default();
        let mut addresses = Vec::new();
        for i in 0..10 {
            let signer = MnemonicBuilder::<English>::default()
                .phrase(self.mnemonic.clone())
                .index(i)?
                .build()?;
            wallet.register_signer(signer.clone());
            addresses.push(signer.address());
        }
        Ok((wallet, addresses))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DataConfig {
    pub path: PathBuf,
}

impl DataConfig {
    pub fn get_candidates(&self, chain_id: u64) -> eyre::Result<Vec<Candidate>> {
        if !self.path.exists() {
            return Err(eyre!("Routes JSON file not found at: {}", self.path.display()));
        }

        let file = File::open(&self.path)?;
        let routes_data: RoutesMap = serde_json::from_reader(BufReader::new(file))
            .map_err(|e| eyre!("Failed to parse routes JSON: {}", e))?;

        let chain_routes = routes_data
            .get(&chain_id.to_string())
            .ok_or_else(|| eyre!("No routes found for chain_id: {}", chain_id))?;

        self.build_all_paths(chain_routes)
    }

    fn build_all_paths(&self, chain_routes: &Route) -> eyre::Result<Vec<Candidate>> {
        let token_map: HashMap<&String, Vec<&RouteElement>> =
            chain_routes.elements.iter().fold(HashMap::new(), |mut acc, element| {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::{PATH_FINDER_EXEX_ID, StrategyConfig};
    use std::fs;

    const CONFIG_V1: &str = r#"
        [relayer]
        mnemonic = "mnemonic1"

        [logging]
        level = "info"
        file = "/tmp/log1.log"

        [strategy]
        [strategy.path-finder]
        type = "path-finder"
        vault = "0x0000000000000000000000000000000000000000"
        max-liquidity = "1000"
        min-liquidity = "100"
        contract = "0x00"
        max-profit-ratio = "0.005"
        min-profit-ratio = "0.001"
        [strategy.path-finder.data]
        path = "/tmp/db1.json"
    "#;

    #[test]
    fn test_from_file_and_reload() {
        let path: PathBuf = std::env::temp_dir().join("searcher_config_test.toml");
        fs::write(&path, CONFIG_V1).unwrap();

        let mut manager = ConfigManager::from_file(path.to_str().unwrap()).unwrap();
        assert_eq!(manager.get_wallet().is_ok(), true);
        assert_eq!(manager.get_strategy(PATH_FINDER_EXEX_ID).is_ok(), true);

        // write new config
        const CONFIG_V2: &str = r#"
        [relayer]
        mnemonic = "mnemonic2"

        [logging]
        level = "debug"

        [strategy]
        [strategy.path-finder]
        type = "path-finder"
        vault = "0x0000000000000000000000000000000000000000"
        max-liquidity = "1000"
        min-liquidity = "100"
        contract = "0x00"
        max-profit-ratio = "0.005"
        min-profit-ratio = "0.001"
        [strategy.path-finder.data]
        path = "/tmp/db1.json"
        "#;
        fs::write(&path, CONFIG_V2).unwrap();

        manager.reload().unwrap();

        let strategy = manager.get_strategy(PATH_FINDER_EXEX_ID).unwrap();
        assert_eq!(manager.config.relayer.mnemonic, "mnemonic2");
        assert_eq!(manager.config.logging.level, "debug");
        match strategy {
            StrategyConfig::PathFinder(cfg) => {
                assert_eq!(cfg.data.path, PathBuf::from("/tmp/db1.json"));
            }
        }

        let _ = fs::remove_file(&path);
    }
}
