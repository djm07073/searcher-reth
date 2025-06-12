use alloy_signer_local::{ coins_bip39::English, MnemonicBuilder };
use alloy_network::EthereumWallet;
use alloy_primitives::Address;

use eyre::Result;
use serde::{ Deserialize, Serialize };
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SearcherConfig {
    pub relayer: TxRelayerConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TxRelayerConfig {
    pub vault_address: String,
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
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<PathBuf>,
}

impl SearcherConfig {
    pub fn from_file(path: &str) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}
