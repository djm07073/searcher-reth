use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    rpc::types::TransactionRequest,
};
use alloy_primitives::{Address, ChainId, FixedBytes};
use alloy_provider::{
    Identity, IpcConnect, Provider, ProviderBuilder, RootProvider,
    fillers::{
        BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, WalletFiller,
    },
};
use eyre::Result;
use futures_util::future::try_join_all;
use tokio::sync::Mutex;

use reth_tracing::tracing;

pub(crate) type IpcWalletProvider = FillProvider<
    JoinFill<
        JoinFill<
            Identity,
            JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
        >,
        WalletFiller<EthereumWallet>,
    >,
    RootProvider,
>;

pub(crate) struct Relayer {
    provider: IpcWalletProvider,
    nonce: AtomicU64,
}

pub(crate) struct RelayerPool {
    chain_id: ChainId,
    relayers: Vec<Arc<Mutex<Relayer>>>,
    current: AtomicUsize,
}

impl RelayerPool {
    pub async fn new(
        icp_connect: IpcConnect<String>,
        wallets: Vec<EthereumWallet>,
    ) -> Result<Self> {
        let provider = ProviderBuilder::new().connect_ipc(icp_connect.clone()).await?;
        let chain_id = provider.get_chain_id().await?;

        let relayers = try_join_all(wallets.into_iter().map(|wallet| {
            let icp = icp_connect.clone();
            async move {
                let provider =
                    ProviderBuilder::new().wallet(wallet.clone()).connect_ipc(icp).await?;
                let signer_address = wallet.default_signer().address();
                let account_info = provider.get_account_info(signer_address).await?;
                let nonce = account_info.nonce;

                Ok::<Arc<Mutex<Relayer>>, eyre::Error>(Arc::new(Mutex::new(Relayer {
                    provider,
                    nonce: AtomicU64::new(nonce),
                })))
            }
        }))
        .await?;

        Ok(Self { chain_id, relayers, current: AtomicUsize::new(0) })
    }

    pub async fn send_transaction(&self, to: Address, data: Vec<u8>) -> Result<FixedBytes<32>> {
        let current = self.current.fetch_add(1, Ordering::SeqCst) % self.relayers.len();
        let relayer = &self.relayers[current];
        let relayer = relayer.lock().await;

        let nonce = relayer.nonce.load(Ordering::Acquire);

        // TODO consider using
        // 1. Set access list to reduce gas fees
        // 2. EIP-1559 config : max_fee_per_gas, max_priority_fee_per_gas
        let max_fee_per_gas = 20_000_000_000;
        let max_priority_fee_per_gas = 1_000_000_000;
        let gas_limit = 21_000;
        let tx = TransactionRequest::default()
            .with_to(to)
            .with_chain_id(self.chain_id)
            .with_input(data)
            .with_nonce(nonce)
            .with_gas_limit(gas_limit)
            .with_max_priority_fee_per_gas(max_priority_fee_per_gas)
            .with_max_fee_per_gas(max_fee_per_gas);

        let pending_tx = relayer.provider.send_transaction(tx).await?;

        match pending_tx.get_receipt().await {
            Ok(receipt) => {
                let tx_hash = receipt.transaction_hash;
                let gas_used = receipt.gas_used;
                // TODO: If the transaction is confirmed, query the balance of the relayer's wallet
                match relayer.nonce.compare_exchange(
                    nonce,
                    nonce + 1,
                    Ordering::SeqCst,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        //TODO: handle event logs to get profit
                        tracing::info!(
                            event = "transaction_confirmed",
                            status = "success",
                            tx_hash = ?tx_hash,
                            relayer = current,
                            nonce = nonce,
                            gas_used = gas_used,
                            gas_limit = gas_limit,
                            effective_gas_price = receipt.effective_gas_price,
                            "Transaction confirmed"
                        );
                        Ok(tx_hash)
                    }
                    Err(nonce) => {
                        tracing::error!(
                            event = "old_transaction_confirmed",
                            status = "error",
                            nonce = nonce,
                            relayer = current,
                            "Failed to update nonce"
                        );
                        Err(eyre::eyre!("Failed to update nonce: {}, relayer: {}", nonce, current))
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    event = "transaction_failed",
                    status = "error",
                    nonce = nonce,
                    relayer = current,
                    error = %e,
                    "Transaction failed"
                );
                Err(e.into())
            }
        }
    }
}
