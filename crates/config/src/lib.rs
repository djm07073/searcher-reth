use alloy::signers::local::{ coins_bip39::English, MnemonicBuilder };
use alloy_signer_local::PrivateKeySigner;
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
    pub fn get_signers(&self) -> Result<Vec<PrivateKeySigner>> {
        let mut signers = Vec::with_capacity(10);
        for i in 0..10 {
            let wallet = MnemonicBuilder::<English>
                ::default()
                .phrase(self.mnemonic.clone())
                .index(i)?
                .build()?;
            signers.push(wallet);
        }
        Ok(signers)
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
