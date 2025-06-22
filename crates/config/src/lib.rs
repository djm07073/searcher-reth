pub mod strategy;

use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use alloy_signer_local::{coins_bip39::English, MnemonicBuilder};

use eyre::Result;

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use strategy::StrategyConfig;

pub trait Config: Sized {
    fn from_file(path: &str) -> eyre::Result<Self>;

    fn reload(&mut self, path: &str) -> eyre::Result<()>;
}

impl Config for SearcherConfig {
    fn from_file(path: &str) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    fn reload(&mut self, path: &str) -> eyre::Result<()> {
        let content = std::fs::read_to_string(path)?;
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
    pub database: DatabaseConfig,
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
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<PathBuf>,
}
