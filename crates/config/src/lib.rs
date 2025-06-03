use alloy::{ network::EthereumWallet, signers::local::PrivateKeySigner };
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
    pub private_keys: Vec<String>,
}

impl TxRelayerConfig {
    pub fn get_wallets(&self) -> Result<Vec<EthereumWallet>> {
        self.private_keys
            .iter()
            .map(|key| {
                let pk_signer: PrivateKeySigner = key.parse()?;
                Ok(EthereumWallet::new(pk_signer))
            })
            .collect()
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
