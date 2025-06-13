use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

use alloy_eips::Encodable2718;
use alloy_network::{Ethereum, EthereumWallet, NetworkWallet, TransactionBuilder};
use alloy_primitives::{Address, ChainId, FixedBytes};
use eyre::Result;
use futures_util::StreamExt;
use reth::{
    api::FullNodeComponents,
    network::NetworkInfo,
    rpc::types::TransactionRequest,
    transaction_pool::{TransactionEvent, TransactionOrigin, TransactionPool},
};
use reth_primitives::{Recovered, TransactionSigned};
use reth_provider::AccountReader;
use reth_tracing::tracing;
use reth_transaction_pool::EthPooledTransaction;

use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::relayer_pool::signals::{Status, handle_signals};

use super::wallet::RelayerWallet;

pub struct RelayerPool<FC: FullNodeComponents> {
    fnc: FC,
    wallet: RelayerWallet,
    chain_id: ChainId,
}

#[derive(Debug)]
pub struct RelayerMessage {
    pub to: Address,
    pub calldata: Vec<u8>,
}

impl<FC> RelayerPool<FC>
where
    FC: FullNodeComponents,
    FC::Pool: TransactionPool<Transaction = EthPooledTransaction>,
{
    pub async fn new(fnc: FC, wallet: (EthereumWallet, Vec<Address>)) -> Result<Self> {
        let chain_id = fnc.network().chain_id();
        let wallet = RelayerWallet::new(wallet, |address| {
            let account = fnc.provider().basic_account(&address).unwrap_or_default();
            Arc::new(AtomicU64::new(account.unwrap_or_default().nonce))
        });
        Ok(Self { fnc, chain_id, wallet })
    }

    pub async fn start(self: Arc<Self>) -> Result<Sender<RelayerMessage>> {
        let (tx, rx) = mpsc::channel::<RelayerMessage>(100);
        let status = Arc::new(AtomicU8::new(Status::Running as u8));
        let status_for_signals = status.clone();
        let status_for_messages = status.clone();
        // Spawn signal handler
        tokio::spawn(async move {
            if let Err(e) = handle_signals(status_for_signals).await {
                tracing::error!("Signal handler failed: {}", e);
            }
        });
        // Spawn message handling task
        let this = self.clone();
        tokio::spawn(async move {
            this.handle_relayer_messages(rx, status_for_messages).await;
        });

        Ok(tx)
    }

    async fn send_and_subscribe_transaction(
        &self,
        to: Address,
        calldata: Vec<u8>,
    ) -> Result<FixedBytes<32>> {
        let (from, atomic_nonce) = self.wallet.next_signer();
        let nonce = atomic_nonce.fetch_add(1, Ordering::SeqCst);

        tracing::info!(
            event = "transaction_send",
            nonce = nonce,
            to = ?to,
            from = ?from,
            "Sending and subscribing to transaction"
        );

        const MAX_FEE_PER_GAS: u128 = 20_000_000_000;
        const MAX_PRIORITY_FEE_PER_GAS: u128 = 1_000_000_000;
        const GAS_LIMIT: u64 = 21_000;
        let request = TransactionRequest::default()
            .with_to(to)
            .with_input(calldata)
            .with_nonce(nonce)
            .with_chain_id(self.chain_id)
            .with_gas_limit(GAS_LIMIT)
            .with_max_priority_fee_per_gas(MAX_PRIORITY_FEE_PER_GAS)
            .with_max_fee_per_gas(MAX_FEE_PER_GAS);

        let tx_envelope = NetworkWallet::<Ethereum>::sign_request(self.wallet.wallet(), request)
            .await
            .map_err(|e| {
                atomic_nonce.fetch_sub(1, Ordering::SeqCst);
                eyre::eyre!("Failed to sign transaction: {:?}", e)
            })?;

        let reth_signed_tx: TransactionSigned = tx_envelope.into();
        let recovered_tx = Recovered::new_unchecked(reth_signed_tx, from);
        let len = recovered_tx.encode_2718_len();
        // eth pool transaction
        let eth_pooled_tx = EthPooledTransaction::new(recovered_tx, len);

        let mut tx_events = self
            .fnc
            .pool()
            .add_transaction_and_subscribe(TransactionOrigin::Local, eth_pooled_tx)
            .await
            .map_err(|e| {
                atomic_nonce.fetch_sub(1, Ordering::SeqCst);
                eyre::eyre!("Failed to add transaction to pool: {:?}", e)
            })?;

        let tx_hash = tx_events.hash();

        while let Some(event) = tx_events.next().await {
            match event {
                TransactionEvent::Mined(block_hash) => {
                    tracing::info!(
                        event = "transaction_mined",
                        block_hash = ?block_hash,
                        relayer = ?from,
                        nonce = nonce,
                        tx_hash = ?tx_hash,
                        "Transaction successfully mined"
                    );
                    return Ok(tx_hash);
                }
                TransactionEvent::Propagated(kind) => {
                    tracing::debug!(
                        event = "transaction_propagated",
                        relayer = ?from,
                        nonce = nonce,
                        kind = ?kind,
                        "Transaction propagated to peers"
                    );
                }
                TransactionEvent::Pending => {
                    tracing::debug!(
                        event = "transaction_pending",
                        nonce = nonce,
                        "Transaction is pending in mempool"
                    );
                }
                TransactionEvent::Queued => {
                    tracing::warn!(
                        event = "transaction_queued",
                        relayer = ?from,
                        nonce = nonce,
                        "Transaction is queued (low gas price?)"
                    );
                }
                other => {
                    atomic_nonce.fetch_sub(1, Ordering::SeqCst);
                    tracing::error!(
                        event = "transaction_failed",
                        relayer = ?from,
                        nonce = nonce,
                        error = ?other,
                        "Transaction encountered an error - rolling back nonce"
                    );
                    return Err(eyre::eyre!("Transaction became invalid: {:?}", other));
                }
            }
        }

        atomic_nonce.fetch_sub(1, Ordering::SeqCst);
        Err(eyre::eyre!("Transaction event stream ended without mining confirmation"))
    }

    async fn handle_relayer_messages(
        self: Arc<Self>,
        mut rx: Receiver<RelayerMessage>,
        status: Arc<AtomicU8>,
    ) {
        while let Some(message) = rx.recv().await {
            if status.load(Ordering::SeqCst) == (Status::Stopped as u8) {
                tracing::info!(
                    event = "shutdown",
                    "Stopping message processing due to shutdown signal"
                );
                break;
            }

            let this = self.clone();
            tokio::spawn(async move {
                if let Ok(hash) =
                    this.send_and_subscribe_transaction(message.to, message.calldata).await
                {
                    tracing::info!(
                        event = "transaction_sent",
                        tx_hash = ?hash,
                        "Transaction sent successfully"
                    );
                }
            });
        }
        tracing::info!(event = "shutdown", "Message handler stopped");
    }
}
