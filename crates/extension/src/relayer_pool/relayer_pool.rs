use std::sync::{ Arc, atomic::{ AtomicU8, AtomicU64, AtomicUsize, Ordering } };

use alloy_network::{ Ethereum, EthereumWallet, NetworkWallet, TransactionBuilder };
use alloy_primitives::{ map::foldhash::HashMap, Address, ChainId, FixedBytes };
use eyre::Result;
use futures_util::StreamExt;
use reth::{
    api::FullNodeComponents,
    network::NetworkInfo,
    rpc::types::TransactionRequest,
    transaction_pool::{ TransactionEvent, TransactionOrigin, TransactionPool },
};
use reth_primitives::{ TransactionSigned, Recovered };
use reth_provider::AccountReader;
use reth_tracing::tracing;
use reth_transaction_pool::PoolTransaction;
pub struct RelayerPool<FC: FullNodeComponents> {
    fnc: FC,
    wallet: EthereumWallet,
    idx: AtomicUsize,
    addresses: Vec<Address>,
    nonce_map: HashMap<Address, Arc<AtomicU64>>,
    chain_id: ChainId,
}

use tokio::sync::mpsc::{ self, Receiver, Sender };

use crate::relayer_pool::signals::{ Status, handle_signals };

#[derive(Debug)]
pub struct RelayerMessage {
    pub to: Address,
    pub data: Vec<u8>,
}

impl<FC> RelayerPool<FC>
    where FC: FullNodeComponents, <FC::Pool as TransactionPool>::Transaction: PoolTransaction
{
    pub async fn new(fnc: FC, wallet: (EthereumWallet, Vec<Address>)) -> Result<Self> {
        let chain_id = fnc.network().chain_id();
        let nonce_map = wallet.1
            .clone()
            .into_iter()
            .map(|address| {
                let fnc = fnc.clone();
                let account = fnc.provider().basic_account(&address).unwrap().unwrap();
                let nonce = account.nonce;
                (address, Arc::new(AtomicU64::new(nonce)))
            })
            .collect();

        Ok(Self {
            fnc,
            chain_id,
            wallet: wallet.0,
            idx: AtomicUsize::new(0),
            nonce_map,
            addresses: wallet.1,
        })
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
        let current_idx = self.idx.fetch_add(1, Ordering::SeqCst) % self.addresses.len();
        let from = self.addresses[current_idx];
        let nonce_atomic = self.nonce_map
            .get(&from)
            .ok_or_else(|| eyre::eyre!("Address not found in nonce map: {:?}", from))?;
        let nonce = nonce_atomic.load(Ordering::SeqCst);

        // 1. Set access list to reduce gas fees
        // TODO: set access list if needed
        // 2. EIP-1559 config : max_fee_per_gas, max_priority_fee_per_gas
        let max_fee_per_gas = 20_000_000_000;
        let max_priority_fee_per_gas = 1_000_000_000;
        let gas_limit = 21_000;
        tracing::info!(
            event = "transaction_send",
            idx = current_idx,
            nonce = nonce,
            to = ?to,
            from = ?from,
            "Sending transaction"
        );

        // let mut request = TransactionRequest::default()
        //     .with_to(to)
        //     .with_input(data)
        //     .with_nonce(nonce)
        //     .with_chain_id(self.chain_id)
        //     .with_gas_limit(gas_limit)
        //     .with_max_priority_fee_per_gas(max_priority_fee_per_gas)
        //     .with_max_fee_per_gas(max_fee_per_gas);

        // let transaction = NetworkWallet::<Ethereum>
        //     ::sign_request(&self.wallet, request.clone()).await
        //     .map_err(|e| eyre::eyre!("Failed to sign transaction: {:?}", e))?
        //     .with_signer(from);

        // let pool_transaction = EthPooledTransaction::new(
        //     transaction.clone(),
        //     transaction.eip2718_encoded_length()
        // );

        // let mut tx_events = self.fnc
        //     .pool()
        //     .add_transaction_and_subscribe(TransactionOrigin::Local, pool_transaction).await?;

        let request = TransactionRequest::default()
            .with_to(to)
            .with_input(data)
            .with_nonce(nonce)
            .with_chain_id(self.chain_id)
            .with_gas_limit(gas_limit)
            .with_max_priority_fee_per_gas(max_priority_fee_per_gas)
            .with_max_fee_per_gas(max_fee_per_gas);

        // let alloy_signed_tx = NetworkWallet::<Ethereum>
        //     ::sign_request(&self.wallet, request.clone()).await
        //     .map_err(|e| eyre::eyre!("Failed to sign transaction: {:?}", e))?;

        // let reth_signed_tx: TransactionSigned = alloy_signed_tx
        //     .try_into()
        //     .map_err(|e| eyre::eyre!("Failed to convert Alloy tx to Reth tx: {:?}", e))?;

        // let recovered_tx = Recovered::new_unchecked(reth_signed_tx, from);

        // let pool_transaction = <FC::Pool as TransactionPool>::Transaction
        //     ::try_from_consensus(todo!("Implement consensus logic"))
        //     .map_err(|e| eyre::eyre!("Failed to convert to pool transaction: {:?}", e))?;

        let mut tx_events = self.fnc
            .pool()
            .add_transaction_and_subscribe(
                TransactionOrigin::Local,
                todo!("Implement consensus logic")
            ).await?;

        let tx_hash = tx_events.hash();
        while let Some(event) = tx_events.next().await {
            match event {
                TransactionEvent::Mined(block_hash) => {
                    tracing::info!(
                        event = "transaction_mined",
                        block_hash = ?block_hash,
                        relayer = ?from,
                        nonce = nonce,
                        "Transaction mined successfully"
                    );
                    nonce_atomic.fetch_add(1, Ordering::SeqCst);
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
                        relayer = ?from,
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
