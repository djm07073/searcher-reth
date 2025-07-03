use alloy_network::EthereumWallet;
use alloy_primitives::{ hex, Address };
use alloy_signer_local::{ coins_bip39::English, MnemonicBuilder };

use eyre::{ eyre, Result };

use serde::{ Deserialize, Serialize };
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::PathBuf,
    sync::{ atomic::{ AtomicBool, Ordering }, Arc },
};

use crate::{ strategy::StrategyConfig, types::{ Candidate, Route, RoutesMap } };

pub struct ConfigManager {
    // configuration from the file
    config: SearcherConfig,
    // flag to indicate if the configuration has changed
    config_changed: Arc<AtomicBool>,
    // candidates loaded from the data file
    candidates: Option<Vec<Candidate>>,
}

impl ConfigManager {
    pub fn from_file(path: &str) -> eyre::Result<Self> {
        let config = SearcherConfig::from_file(path)?;
        Ok(Self { config, config_changed: Arc::new(AtomicBool::new(false)), candidates: None })
    }

    pub fn reload(&mut self) -> eyre::Result<()> {
        self.config.reload()?;
        self.config_changed.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn get_wallet(&self) -> Result<(EthereumWallet, Vec<Address>)> {
        self.config.relayer.get_wallet()
    }

    pub fn get_strategy(&self, exex_id: &str) -> eyre::Result<StrategyConfig> {
        self.config.strategies
            .get(exex_id)
            .cloned()
            .ok_or_else(|| eyre::eyre!("Strategy not found for exex_id: {}", exex_id))
    }

    pub fn get_candidates(&mut self, chain_id: u64) -> eyre::Result<Vec<Candidate>> {
        let candidates = if
            self.candidates.is_none() ||
            self.config_changed.load(Ordering::Relaxed)
        {
            self.config_changed.store(false, Ordering::Relaxed);
            let candidates = self.config.data.get_candidates(chain_id)?;
            self.candidates = Some(candidates.clone());
            candidates
        } else {
            self.candidates.clone().unwrap()
        };
        Ok(candidates)
    }
}

impl SearcherConfig {
    fn from_file(path: &str) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    fn reload(&mut self) -> eyre::Result<()> {
        let content = std::fs::read_to_string(&self.data.path)?;
        let new_config: SearcherConfig = toml::from_str(&content)?;
        *self = new_config;
        Ok(())
    }
}

type ExExId = String;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SearcherConfig {
    pub relayer: TxRelayerConfig,
    pub data: DataConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
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
            let signer = MnemonicBuilder::<English>
                ::default()
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
        let routes_data: RoutesMap = serde_json
            ::from_reader(BufReader::new(file))
            .map_err(|e| eyre!("Failed to parse routes JSON: {}", e))?;

        let chain_routes = routes_data
            .get(&chain_id.to_string())
            .ok_or_else(|| eyre!("No routes found for chain_id: {}", chain_id))?;

        self.build_all_paths(chain_routes)
    }

    fn build_all_paths(&self, chain_routes: &Route) -> eyre::Result<Vec<Candidate>> {
        let token_map: HashMap<_, _> = chain_routes.elements
            .iter()
            .fold(HashMap::new(), |mut acc, element| {
                acc.entry(&element.src_token).or_insert_with(Vec::new).push(element);
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
    use std::fs;

    const CONFIG_V1: &str =
        r#"
        [relayer]
        mnemonic = "mnemonic1"

        [data]
        path = "/tmp/db1.json"

        [logging]
        level = "info"
        file = "/tmp/log1.log"

        [strategy]
        [path-finder]
        vault = "0x0000000000000000000000000000000000000000"
        contract = "0x00"
        max_profit_ratio = "0.005"
        min_profit_ratio = "0.001"
    "#;

    const CONFIG_V2: &str =
        r#"
        [relayer]
        mnemonic = "mnemonic2"

        [data]
        path = "/tmp/db2.json"

        [logging]
        level = "debug"
        file = "/tmp/log2.log"

        [strategy]
        [path-finder]
        vault = "0x0000000000000000000000000000000000000001"
        contract = "0x01"
        max_profit_ratio = "0.010"
        min_profit_ratio = "0.002"
    "#;

    #[test]
    fn test_from_file_and_reload() {
        let path = std::env::temp_dir().join("searcher_config_test.toml");
        fs::write(&path, CONFIG_V1).unwrap();

        let mut cfg = SearcherConfig::from_file(path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.relayer.mnemonic, "mnemonic1");
        assert_eq!(cfg.logging.level, "info");

        fs::write(&path, CONFIG_V2).unwrap();
        cfg.reload().unwrap();

        assert_eq!(cfg.relayer.mnemonic, "mnemonic2");
        assert_eq!(cfg.logging.level, "debug");

        let _ = fs::remove_file(&path);
    }
}
