use std::{ future::Future, sync::Arc };

use eyre::Result;
use futures_util::StreamExt;
use reth::network::NetworkInfo;
use reth_exex::{ ExExContext, ExExEvent, ExExNotification };
use reth_node_api::{ FullNodeComponents, FullNodeTypes };
use reth_provider::{
    AccountReader,
    BlockHashReader,
    BlockReaderIdExt,
    ChainSpecProvider,
    DatabaseProviderFactory,
    LatestStateProviderRef,
    ReceiptProvider,
    StateCommitmentProvider,
};
use reth_tracing::tracing::{ self };
use reth_transaction_pool::{ EthPooledTransaction, TransactionPool };
use searcher_reth_manager::SignalType;
use searcher_reth_strategy::core::strategy::Strategy;
use tokio::sync::broadcast;

use crate::relayer_pool::{ RelayerMessage, RelayerPool, WalletPool };

pub struct SearcherExEx {
    pub wallet: Arc<WalletPool>,
    pub signal_rx: broadcast::Receiver<SignalType>,
}

// impl of exex
impl SearcherExEx {
    pub fn new(wallet: Arc<WalletPool>, signal_rx: broadcast::Receiver<SignalType>) -> Self {
        Self { wallet, signal_rx }
    }

    pub async fn run<Node, S>(
        self,
        mut ctx: ExExContext<Node>,
        mut strategy: S
    )
        -> Result<impl Future<Output = Result<()>>>
        where
            S: Strategy,
            Node: FullNodeComponents,
            Node::Pool: TransactionPool<Transaction = EthPooledTransaction>,
            <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider: BlockHashReader +
                StateCommitmentProvider,
            Node::Provider: BlockReaderIdExt + ReceiptProvider + AccountReader + ChainSpecProvider
    {
        let wallet = self.wallet.clone();
        let signal_rx = self.signal_rx.resubscribe();
        let vault = strategy.get_vault();
        Ok(async move {
            let relayer_pool = Arc::new(
                RelayerPool::new(
                    ctx.components.clone(),
                    wallet,
                    signal_rx,
                    strategy.gas_config()
                ).await?
            );
            let relayer_tx = relayer_pool.start().await?;
            tracing::info!(
                target: "reth-exex",
                event = "relayer_pool_started",
                "Starting Relayer Pool"
            );
            strategy.prepare(ctx.components.network().chain_id());
            while let Some(notification) = ctx.notifications.next().await {
                if let Ok(ExExNotification::ChainCommitted { new: chain }) = notification {
                    let block = chain.tip();
                    let num_hash = block.num_hash();
                    let bytecode = strategy.get_code();

                    // extension is not setup yet, skip
                    if bytecode.is_empty() {
                        ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                        continue;
                    }

                    // 1. Get the pending transactions from the transaction pool
                    let pending_txs: Vec<EthPooledTransaction> = ctx.components
                        .pool()
                        .pending_transactions()
                        .iter()
                        .map(|tx| tx.transaction.clone())
                        .collect();

                    // 2. Create a read-only database provider that we can use to get latest state
                    let database_provider: <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider = ctx
                        .provider()
                        .database_provider_ro()?;
                    let latest_state_provider = LatestStateProviderRef::new(&database_provider);

                    // 3. Filter candidates by path finder based on the latest state and pending
                    // transactions
                    let profitable_candidates = strategy.find_profitable_candidates(
                        num_hash,
                        latest_state_provider,
                        pending_txs
                    )?;

                    tracing::info!(
                        target: "reth-exex",
                        event = "filter_candidates",
                        success = profitable_candidates.is_none(),
                        num_hash = ?num_hash,
                        "Profitable candidates found"
                    );
                    if profitable_candidates.is_none() {
                        ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                        continue;
                    }

                    // 4. Send the filtered candidates to the relayer pool for broadcasting
                    // transactions
                    let (calldata, access_list) = profitable_candidates.unwrap();
                    let relayer_channel = relayer_tx.clone();
                    tokio::spawn(async move {
                        let message = RelayerMessage { to: vault, calldata, access_list };
                        let r = relayer_channel.send(message).await;

                        match r {
                            Ok(_) =>
                                tracing::info!(
                                target: "reth-exex",
                                event = "send_candidates_to_relayer_pool",
                                success = true,
                                num_hash = ?num_hash,
                                "Successfully sent candidates to relayer pool"
                            ),
                            Err(e) =>
                                tracing::error!(
                                target: "reth-exex",
                                event = "send_candidates_to_relayer_pool",
                                success = false,
                                error = ?e,
                                num_hash = ?num_hash,
                                "Failed to send candidates to relayer pool"
                            ),
                        }
                    });

                    ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                }
            }

            Ok(())
        })
    }
}
