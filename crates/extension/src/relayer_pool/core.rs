use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use alloy_consensus::TxReceipt;
use alloy_eips::Encodable2718;
use alloy_network::{Ethereum, EthereumWallet, NetworkWallet, TransactionBuilder};
use alloy_primitives::{Address, ChainId, FixedBytes};
use alloy_sol_types::SolEvent;
use eyre::Result;
use futures_util::StreamExt;
use reth::{
    api::FullNodeComponents,
    network::NetworkInfo,
    rpc::types::{AccessList, TransactionRequest},
    transaction_pool::{TransactionEvent, TransactionOrigin, TransactionPool},
};
use reth_primitives::{LogData, Recovered, TransactionSigned};
use reth_provider::{AccountReader, ReceiptProvider};
use reth_tracing::tracing;
use reth_transaction_pool::EthPooledTransaction;
use searcher_reth_manager::SignalType;
use tokio::sync::{
    broadcast,
    mpsc::{self, Receiver, Sender},
};

use crate::relayer_pool::types::Profit;

use super::wallet::RelayerWallet;

pub struct RelayerPool<FC: FullNodeComponents> {
    fnc: FC,
    wallet: RelayerWallet,
    chain_id: ChainId,
    signal_rx: broadcast::Receiver<SignalType>,
}

#[derive(Debug)]
pub struct RelayerMessage {
    pub to: Address,
    pub calldata: Vec<u8>,
    pub access_list: AccessList,
}

impl<FC> RelayerPool<FC>
where
    FC: FullNodeComponents,
    FC::Pool: TransactionPool<Transaction = EthPooledTransaction>,
{
    pub async fn new(
        fnc: FC,
        wallet: (EthereumWallet, Vec<Address>),
        signal_rx: broadcast::Receiver<SignalType>,
    ) -> Result<Self> {
        let chain_id = fnc.network().chain_id();
        let wallet = RelayerWallet::new(wallet, |address| {
            let account = fnc.provider().basic_account(&address).unwrap_or_default();
            Arc::new(AtomicU64::new(account.unwrap_or_default().nonce))
        });
        Ok(Self { fnc, chain_id, wallet, signal_rx })
    }

    pub async fn start(self: Arc<Self>) -> Result<Sender<RelayerMessage>> {
        let (message_tx, message_rx) = mpsc::channel::<RelayerMessage>(100);

        // Spawn message handling task
        let this = self.clone();
        let signal_rx = self.signal_rx.resubscribe();
        tokio::spawn(async move {
            this.handle_relayer_messages(message_rx, signal_rx).await;
        });
        Ok(message_tx)
    }

    async fn send_and_subscribe_transaction(
        &self,
        message: RelayerMessage,
    ) -> Result<FixedBytes<32>> {
        let (from, atomic_nonce) = self.wallet.next_signer();
        let nonce = atomic_nonce.fetch_add(1, Ordering::SeqCst);

        if message.to == Address::ZERO {
            tracing::info!(
                target: "reth-exex",
                "Vault is zero, skipping transaction sending"
            );
            return Ok(FixedBytes::default());
        }
        tracing::info!(
            event = "transaction_send",
            nonce = nonce,
            to = ?message.to,
            from = ?from,
            "Sending and subscribing to transaction"
        );

        // TODO: fix gas price and limit
        const MAX_FEE_PER_GAS: u128 = 20_000_000_000;
        const MAX_PRIORITY_FEE_PER_GAS: u128 = 1_000_000_000;
        const GAS_LIMIT: u64 = 21_000;
        let request = TransactionRequest::default()
            .with_to(message.to)
            .with_input(message.calldata)
            .with_access_list(message.access_list)
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
                    // TODO: get events from transaction receipt
                    let receipt = self
                        .fnc
                        .provider()
                        .receipt_by_hash(tx_hash)
                        .map_err(|e| eyre::eyre!("Failed to get transaction: {:?}", e))?;

                    if let Some(receipt) = receipt {
                        for log in receipt.logs() {
                            if let Some(sig) = LogData::topics(log).first() {
                                if sig == &Profit::SIGNATURE_HASH {
                                    let parsed = Profit::decode_log(log)?;
                                    tracing::info!(
                                        event = "real_profit",
                                        token = ?parsed.token,
                                        profit = ?parsed.profit,
                                        "Get Profit"
                                    );
                                }
                            }
                        }
                        tracing::info!(
                            event = "real_profit",
                            tx_hash = ?tx_hash,
                            "Transaction receipt retrieved"
                        );
                    }

                    return Ok(tx_hash);
                }
                TransactionEvent::Propagated(kind) => {
                    tracing::info!(
                        event = "transaction_propagated",
                        relayer = ?from,
                        nonce = nonce,
                        kind = ?kind,
                        "Transaction propagated to peers"
                    );
                }
                TransactionEvent::Pending => {
                    tracing::info!(
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
                // Replaced, Discarded, Invalid
                other => {
                    atomic_nonce.fetch_sub(1, Ordering::SeqCst);
                    tracing::error!(
                        event = "transaction_dropped",
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
        mut message_rx: Receiver<RelayerMessage>,
        mut signal_rx: broadcast::Receiver<SignalType>,
    ) {
        let mut is_paused = true;

        loop {
            tokio::select! {
                signal = signal_rx.recv() => {
                    match signal {
                        Ok(SignalType::Pause) => {
                            is_paused = true;
                            tracing::info!(event = "status_change", status = "paused", "Relayer paused");
                        }
                        Ok(SignalType::Resume) => {
                            is_paused = false;
                            tracing::info!(event = "status_change", status = "running", "Relayer resumed");
                        }
                        Ok(SignalType::Shutdown) => {
                            tracing::info!(event = "shutdown", "Relayer received shutdown signal");
                            return;
                        }
                        _ => {
                            tracing::warn!(event = "signal_error", "Signal channel closed");
                            return;
                        }
                    }
                }

                message = message_rx.recv() => {
                    match message {
                        Some(msg) => {
                            if is_paused {
                                tracing::info!(
                                    event = "message_dropped",
                                    "Dropping message due to paused state"
                                );
                                continue;
                            }

                            let this = self.clone();
                            tokio::spawn(async move {
                                if let Ok(hash) = this.send_and_subscribe_transaction(msg).await {
                                    tracing::info!(
                                        event = "transaction_sent",
                                        tx_hash = ?hash,
                                        "Transaction sent successfully"
                                    );
                                }
                            });
                        }
                        None => {
                            tracing::info!(event = "channel_closed", "Message channel closed");
                            break;
                        }
                    }
                }
            }
        }

        tracing::info!(event = "shutdown", "Message handler stopped");
    }
}
