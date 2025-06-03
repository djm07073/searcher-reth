mod signals;

use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
};

use alloy::{
    consensus::TxEip1559,
    eips::Encodable2718,
    network::{EthereumWallet, NetworkWallet, TransactionBuilder, TxSignerSync},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
};
use alloy_primitives::{Address, ChainId, FixedBytes, TxKind};
use eyre::Result;
use futures_util::{StreamExt, future::try_join_all};
use reth::{
    api::FullNodeComponents,
    network::NetworkInfo,
    transaction_pool::{TransactionEvent, TransactionOrigin, TransactionPool},
};
use reth_provider::AccountReader;
use reth_tracing::tracing;
use tokio::sync::Mutex;

pub(crate) struct Relayer {
    signer: PrivateKeySigner,
    nonce: AtomicU64,
}

pub struct RelayerPool<FC: FullNodeComponents> {
    fnc: FC,
    chain_id: ChainId,
    relayers: Vec<Arc<Mutex<Relayer>>>,
    current: AtomicUsize,
}

use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::signals::{Status, handle_signals};

#[derive(Debug)]
pub struct RelayerMessage {
    pub to: Address,
    pub data: Vec<u8>,
}

impl<FC: FullNodeComponents> RelayerPool<FC> {
    pub async fn new(fnc: FC, signers: Vec<PrivateKeySigner>) -> Result<Self> {
        let chain_id = fnc.network().chain_id();
        let relayer_futures = signers.into_iter().map(|signer| {
            let fnc = fnc.clone();
            async move {
                let address = signer.address();
                let account = fnc.provider().basic_account(&address).unwrap().unwrap();
                Ok::<_, eyre::Report>(Arc::new(Mutex::new(Relayer {
                    signer,
                    nonce: AtomicU64::new(account.nonce),
                })))
            }
        });
        let relayers = try_join_all(relayer_futures).await?;

        Ok(Self { fnc, chain_id, relayers, current: AtomicUsize::new(0) })
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
        let relayer_pool = self.clone();
        tokio::spawn(async move {
            relayer_pool.handle_messages(rx, status_for_messages).await;
        });

        Ok(tx)
    }

    async fn send_transaction(&self, to: Address, data: Vec<u8>) -> Result<FixedBytes<32>> {
        let current = self.current.fetch_add(1, Ordering::SeqCst) % self.relayers.len();
        let relayer: Arc<Mutex<Relayer>> = self.relayers[current].clone();
        let relayer = relayer.lock().await;
        let nonce = relayer.nonce.load(Ordering::Acquire);
        // 1. Set access list to reduce gas fees
        // TODO: set access list if needed
        // 2. EIP-1559 config : max_fee_per_gas, max_priority_fee_per_gas
        let max_fee_per_gas = 20_000_000_000;
        let max_priority_fee_per_gas = 1_000_000_000;
        let gas_limit = 21_000;
        let from = relayer.signer.address();
        tracing::info!(
            event = "transaction_send",
            relayer = current,
            nonce = nonce,
            to = ?to,
            from = ?from,
            "Sending transaction"
        );

        let tx = TxEip1559 {
            to: TxKind::Call(to),
            chain_id: self.chain_id,
            nonce,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            gas_limit,
            input: data.into(),
            ..Default::default()
        };
        // let tx = TransactionRequest::default()
        //     .with_to(to)
        //     .with_nonce(nonce)
        //     .with_chain_id(self.chain_id)
        //     .with_gas_limit(21_000)
        //     .with_max_priority_fee_per_gas(max_priority_fee_per_gas)
        //     .with_max_fee_per_gas(max_fee_per_gas);
        // // TODO: Sign the transaction using the relayer's wallet
        // let _wallet = EthereumWallet::from(relayer.signer);
        // let tx_envelope = tx.build(&_wallet).await?;
        let tx_events = self
            .fnc
            .pool()
            .add_transaction_and_subscribe(TransactionOrigin::Local, todo!())
            .await?;
        let tx_hash = tx_events.hash();
        while let Some(event) = tx_events.next().await {
            match event {
                TransactionEvent::Mined(block_hash) => {
                    tracing::info!(
                        event = "transaction_mined",
                        status = "success",
                        block_hash = ?block_hash,
                        relayer = current,
                        nonce = nonce,
                        "Transaction mined successfully"
                    );
                    return Ok(tx_hash);
                }
                TransactionEvent::Propagated(kind) => {
                    tracing::info!(
                        event = "transaction_propagated",
                        status = "success",
                        relayer = current,
                        nonce = nonce,
                        kind = ?kind,
                        "Transaction propagated to peers"
                    );
                }
                TransactionEvent::Invalid => {
                    return Err(eyre::eyre!("Transaction became invalid"));
                }
                TransactionEvent::Discarded => {
                    return Err(eyre::eyre!("Transaction was discarded"));
                }
                // Log other events but continue waiting
                other_event => {
                    tracing::debug!(
                        event = ?other_event,
                        relayer = current,
                        nonce = nonce,
                        "Received transaction event"
                    );
                }
            }
        }
        Err(eyre::eyre!("Transaction event stream ended without mining confirmation"))
    }

    async fn handle_messages(&self, mut rx: Receiver<RelayerMessage>, status: Arc<AtomicU8>) {
        while let Some(message) = rx.recv().await {
            if status.load(Ordering::SeqCst) == (Status::Stopped as u8) {
                tracing::info!(
                    event = "shutdown",
                    "Stopping message processing due to shutdown signal"
                );
                break;
            }

            match self.send_transaction(message.to, message.data).await {
                Ok(hash) => {
                    tracing::info!(
                        event = "transaction_sent",
                        tx_hash = ?hash,
                        "Transaction sent successfully"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        event = "transaction_failed",
                        error = %e,
                        "Failed to send transaction"
                    );
                }
            }
        }
        tracing::info!(event = "shutdown", "Message handler stopped");
    }
}
