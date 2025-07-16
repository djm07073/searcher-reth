use std::{collections::HashMap, sync::RwLock};

use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use alloy_signer_local::{MnemonicBuilder, coins_bip39::English};
use eyre::Result;
use reth_tracing::tracing;
use serde::{Deserialize, Serialize};

use crate::common::StrategyConfig;

pub struct ConfigManager {
    config: RwLock<Config>,
}

impl ConfigManager {
    pub fn from_file(path: &str) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(Self { config: RwLock::new(config) })
    }

    pub fn get_wallet(&self) -> Result<(EthereumWallet, Vec<Address>)> {
        self.config.read().unwrap().relayer.get_wallet()
    }

    pub fn get_strategy(&self, exex_id: &str) -> eyre::Result<StrategyConfig> {
        self.config
            .read()
            .unwrap()
            .strategies
            .get(exex_id)
            .cloned()
            .ok_or_else(|| eyre::eyre!("Strategy not found for exex_id: {}", exex_id))
    }
}

type ExExId = String;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub relayer: TxRelayerConfig,
    #[serde(default, rename = "strategy")]
    pub strategies: HashMap<ExExId, StrategyConfig>,
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,
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
            let signer = MnemonicBuilder::<English>::default()
                .phrase(self.mnemonic.clone())
                .index(i)?
                .build()?;
            wallet.register_signer(signer.clone());
            addresses.push(signer.address());
        }
        tracing::info!("Wallet created with {} signers: {:?}", self.signer_number, addresses);
        Ok((wallet, addresses))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
    #[serde(default = "default_report_interval_secs")]
    pub report_interval_secs: u64,
}

fn default_report_interval_secs() -> u64 {
    600
}

impl ConfigManager {
    pub fn get_telegram(&self) -> Option<TelegramConfig> {
        self.config.read().unwrap().telegram.clone()
    }
}
