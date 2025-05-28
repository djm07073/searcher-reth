use alloy::{network::EthereumWallet, signers::local::PrivateKeySigner};
use alloy_provider::IpcConnect;
use eyre::Result;
use searcher_reth_config::SearcherConfig;

pub trait IpcWalletConnector {
    fn get_wallets(&self) -> Result<Vec<EthereumWallet>>;
    fn get_ipc(&self) -> IpcConnect<String>;
}

impl IpcWalletConnector for SearcherConfig {
    fn get_wallets(&self) -> Result<Vec<EthereumWallet>> {
        self.relayer
            .private_keys
            .iter()
            .map(|key| {
                let pk_signer: PrivateKeySigner = key.parse()?;
                Ok(EthereumWallet::new(pk_signer))
            })
            .collect()
    }

    fn get_ipc(&self) -> IpcConnect<String> {
        IpcConnect::new(self.network.ipc_path.clone())
    }
}
