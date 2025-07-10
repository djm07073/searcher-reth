use std::sync::Arc;

use alloy_consensus::TxReceipt;
use alloy_eips::{Encodable2718, calc_next_block_base_fee};
use alloy_network::{Ethereum, EthereumWallet, NetworkWallet};
use alloy_primitives::{Address, ChainId, FixedBytes, TxKind};
use alloy_sol_types::SolEvent;
use eyre::Result;
use futures_util::StreamExt;
use reth::{
    api::FullNodeComponents,
    core::primitives::AlloyBlockHeader,
    network::NetworkInfo,
    rpc::types::{AccessList, TransactionInput, TransactionRequest},
    transaction_pool::{TransactionEvent, TransactionOrigin, TransactionPool},
};
use reth_chainspec::EthChainSpec;
use reth_primitives::{LogData, Recovered, TransactionSigned};
use reth_provider::{AccountReader, BlockReaderIdExt, ChainSpecProvider, ReceiptProvider};
use reth_tracing::tracing;
use reth_transaction_pool::EthPooledTransaction;
use searcher_reth_manager::{gas::GasConfig, SignalType};
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
    gas_config: GasConfig,
}

#[derive(Debug)]
pub struct RelayerMessage {
    pub to: Address,
    pub calldata: Vec<u8>,
    pub access_list: AccessList,
}

const BASE_FEE: u64 = 100_000_000;

impl<FC> RelayerPool<FC>
where
    FC: FullNodeComponents,
    FC::Pool: TransactionPool<Transaction = EthPooledTransaction>,
    FC::Provider: BlockReaderIdExt + ReceiptProvider + AccountReader + ChainSpecProvider,
    FC::Network: NetworkInfo,
{
    pub async fn new(
        fnc: FC,
        wallet: (EthereumWallet, Vec<Address>),
        signal_rx: broadcast::Receiver<SignalType>,
        gas_config: GasConfig,
    ) -> Result<Self> {
        let chain_id = fnc.network().chain_id();
        let wallet = RelayerWallet::new(wallet);
        Ok(Self { fnc, chain_id, wallet, signal_rx, gas_config })
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
        // 0. if message.to is zero or calldata is empty, skip sending
        if message.to == Address::ZERO || message.calldata.is_empty() {
            tracing::info!(
                target: "reth-exex",
                "Vault is zero or calldata is empty, skipping transaction sending"
            );
            return Ok(FixedBytes::default());
        }

        // 1. Get nonce for the next transaction
        let provider = self.fnc.provider();
        let from = self.wallet.next_signer();
        let account = provider
            .basic_account(&from)
            .map_err(|e| eyre::eyre!("Failed to get nonce for {}: {:?}", from, e))?
            .unwrap_or_default();
        let nonce = account.nonce;

        // 2. Calculate base fee for the next block
        let GasConfig { gas_limit, priority_fee } = self.gas_config;
        let block_header = match provider.pending_header() {
            Ok(Some(header)) => header,
            Ok(None) => match provider.latest_header() {
                Ok(Some(header)) => header,
                Ok(None) => {
                    return Err(eyre::eyre!(
                        "Failed to get latest block header; chain must be initialized (no header found)"
                    ));
                }
                Err(e) => {
                    return Err(eyre::eyre!("Failed to get latest block header: {:?}", e));
                }
            },
            Err(e) => match provider.latest_header() {
                Ok(Some(header)) => header,
                Ok(None) => {
                    return Err(eyre::eyre!(
                        "Failed to get latest block header; chain must be initialized (no header found), pending_header error: {:?}",
                        e
                    ));
                }
                Err(e2) => {
                    return Err(eyre::eyre!(
                        "Failed to get latest and pending block header: pending_header error: {:?}, latest_header error: {:?}",
                        e,
                        e2
                    ));
                }
            },
        };

        let num_hash = block_header.num_hash();
        let chain_spec = provider.chain_spec();
        let base_fee_params = chain_spec.base_fee_params_at_block(num_hash.number);
        let parent_gas_used = block_header.gas_used();
        let parent_block_gas_limit = block_header.gas_limit();
        let parent_block_base_fee = block_header.base_fee_per_gas().unwrap_or(BASE_FEE);
        let base_fee = calc_next_block_base_fee(
            parent_gas_used,
            parent_block_gas_limit,
            parent_block_base_fee,
            base_fee_params,
        );
        // TODO: reasonable formula?
        let max_fee_per_gas = (base_fee as u128) * 2 + priority_fee;
        tracing::info!("Sending and subscribing to transaction");

        // 3. Create and sign the transaction request
        let request = TransactionRequest {
            to: Some(TxKind::Call(message.to)),
            input: TransactionInput::new(message.calldata.into()),
            access_list: Some(message.access_list),
            nonce: Some(nonce),
            chain_id: Some(self.chain_id),
            gas: Some(gas_limit),
            max_priority_fee_per_gas: Some(priority_fee),
            max_fee_per_gas: Some(max_fee_per_gas),
            ..Default::default()
        };

        let tx_envelope = NetworkWallet::<Ethereum>::sign_request(self.wallet.wallet(), request)
            .await
            .map_err(|e| eyre::eyre!("Failed to sign transaction: {:?}", e))?;

        let reth_signed_tx: TransactionSigned = tx_envelope.into();
        let recovered_tx = Recovered::new_unchecked(reth_signed_tx, from);
        let len = recovered_tx.encode_2718_len();
        // eth pool transaction
        let eth_pooled_tx = EthPooledTransaction::new(recovered_tx, len);

        // 4. Add the transaction to the pool and subscribe to its events while mined or dropped
        let mut tx_events = self
            .fnc
            .pool()
            .add_transaction_and_subscribe(TransactionOrigin::Local, eth_pooled_tx)
            .await
            .map_err(|e| eyre::eyre!("Failed to add transaction to pool: {:?}", e))?;
        let tx_hash = tx_events.hash();

        while let Some(event) = tx_events.next().await {
            match event {
                TransactionEvent::Mined(block_hash) => {
                    tracing::info!(
                        event = "transaction_mined",
                        block_hash = %block_hash,
                        relayer = %from,
                        nonce = nonce,
                        tx_hash = %tx_hash,
                        "Transaction successfully mined"
                    );

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
                                        event = "profit_realized",
                                        token = %parsed.token,
                                        profit = %parsed.profit,
                                        "profit event received"
                                    );
                                }
                            }
                        }
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
                        relayer = %from,
                        nonce = nonce,
                        "Transaction is queued (low gas price?)"
                    );
                }
                // Replaced, Discarded, Invalid
                other => {
                    tracing::error!(
                        event = "transaction_dropped",
                        relayer = %from,
                        nonce = nonce,
                        dropped_event = ?other,
                        "Transaction encountered an error - rolling back nonce"
                    );
                    return Err(eyre::eyre!("Transaction became invalid: {:?}", other));
                }
            }
        }

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
                                match this.send_and_subscribe_transaction(msg).await {
                                    Ok(hash) => {
                                        tracing::info!(
                                            event = "transaction_mined",
                                            tx_hash = %hash,
                                            "Transaction mined successfully"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            event = "transaction_failed",
                                            error = ?e,
                                            "Transaction failed to send or confirm"
                                        );
                                    }
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
