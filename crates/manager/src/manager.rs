use std::{ collections::HashMap, sync::{ atomic::{ AtomicBool, Ordering }, Arc, RwLock } };

use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use alloy_signer_local::{ coins_bip39::English, MnemonicBuilder };
use eyre::{ Result };
use serde::{ Deserialize, Serialize };

use crate::{ common::{ CommonStrategyConfig, StrategyConfig }, types::Candidate };

pub struct ConfigManager {
    config_path: String,
    // configuration from the file
    config: RwLock<Config>,
    // flag to indicate if the configuration has changed
    config_changed: Arc<AtomicBool>,
    // candidates loaded from the data file
    candidates_map: HashMap<String, Vec<Candidate>>,
}

impl ConfigManager {
    pub fn from_file(path: &str) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self {
            config_path: path.to_string(),
            config: RwLock::new(toml::from_str(&content)?),
            config_changed: Arc::new(AtomicBool::new(false)),
            candidates_map: HashMap::new(),
        })
    }

    pub fn reload(&mut self) -> eyre::Result<()> {
        let content = std::fs::read_to_string(&self.config_path)?;
        let new_config: Config = toml::from_str(&content)?;
        *self.config.write().unwrap() = new_config;
        self.config_changed.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn get_wallet(&self) -> Result<(EthereumWallet, Vec<Address>)> {
        self.config.read().unwrap().relayer.get_wallet()
    }

    pub fn get_strategy(&self, exex_id: &str) -> eyre::Result<StrategyConfig> {
        self.config
            .read()
            .unwrap()
            .strategies.get(exex_id)
            .cloned()
            .ok_or_else(|| eyre::eyre!("Strategy not found for exex_id: {}", exex_id))
    }

    pub fn get_or_load_candidates(
        &mut self,
        exex_id: &str,
        chain_id: u64
    ) -> eyre::Result<Vec<Candidate>> {
        let config_has_changed = self.config_changed.load(Ordering::Relaxed);
        let cached_candidates = self.candidates_map.get(exex_id);
        let candidates = match cached_candidates {
            Some(candidates) if !config_has_changed => { candidates.clone() }
            _ => {
                self.config_changed.store(false, Ordering::Relaxed);
                let strategy = self.get_strategy(exex_id)?;
                let new_candidates = strategy.load_candidates(chain_id)?;
                self.candidates_map.insert(exex_id.to_string(), new_candidates.clone());
                new_candidates
            }
        };
        Ok(candidates)
    }
}

type ExExId = String;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub relayer: TxRelayerConfig,
    #[serde(default, rename = "strategy")]
    pub strategies: HashMap<ExExId, StrategyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TxRelayerConfig {
    pub mnemonic: String,
    pub signer_number: u32,
}

impl TxRelayerConfig {
    pub fn get_wallet(&self) -> Result<(EthereumWallet, Vec<Address>)> {
        let mut wallet = EthereumWallet::default();
        let mut addresses = Vec::new();
        for i in 0..self.signer_number {
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
