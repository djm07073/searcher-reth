use alloy::{network::EthereumWallet, signers::local::PrivateKeySigner};
use alloy_provider::IpcConnect;
use serde::Deserialize;
use std::fs;
use eyre::Result;

#[derive(Deserialize)]
pub struct SearcherConfig {
    pub vault_address: String,
    pub private_keys: Vec<String>,
}

#[derive(Deserialize)]
pub struct NetworkConfig {
    pub ipc_path: String,
    pub socket_path: String,
}

#[derive(Deserialize)]
pub struct Config {
    pub searcher: SearcherConfig,
    pub network: NetworkConfig,
}

impl Config {
    pub fn new() -> Result<Self> {
        // First try to load from config file
        let config_path = std::env::var("CONFIG_PATH")
            .unwrap_or_else(|_| "./env.toml".to_string());
        
        let config_str = fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&config_str)?;

        // Allow environment variables to override config file
        Ok(Self {
            searcher: SearcherConfig {
                vault_address: std::env::var("VAULT_ADDRESS")
                    .unwrap_or(config.searcher.vault_address),
                private_keys: std::env::var("PRIVATE_KEYS")
                    .map(|keys| keys.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or(config.searcher.private_keys),
            },
            network: NetworkConfig {
                ipc_path: std::env::var("IPC_PATH")
                    .unwrap_or(config.network.ipc_path),
                socket_path: std::env::var("SOCKET_PATH")
                    .unwrap_or(config.network.socket_path),
            },
        })
    }

    pub fn get_wallets(&self) -> Result<Vec<EthereumWallet>> {
        self.searcher.private_keys
            .iter()
            .map(|key| {
                let pk_signer: PrivateKeySigner = key.parse()?;
                Ok(EthereumWallet::new(pk_signer))
            })
            .collect()
    }

    pub fn get_ipc(&self) -> IpcConnect<String> {
        IpcConnect::new(self.network.ipc_path.clone())
    }
}