use alloy_consensus::BlockHeader;
use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use eyre::Result;
use futures_util::StreamExt;
use reth::network::NetworkInfo;
use searcher_reth_repository::{
    SearcherRepository, config::strategy::StrategyConfig, core::strategy::Strategy,
    path_finding::PathFinder,
};
use searcher_reth_util::signal_manager::SignalType;
use std::{future::Future, sync::Arc};

use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::{FullNodeComponents, FullNodeTypes};
use reth_provider::{
    BlockHashReader, DatabaseProviderFactory, LatestStateProviderRef, StateCommitmentProvider,
};
use reth_tracing::tracing;
use reth_transaction_pool::{EthPooledTransaction, TransactionPool};
use tokio::sync::broadcast;

use crate::relayer_pool::{RelayerMessage, RelayerPool};

pub struct SearcherExEx {
    pub wallet: (EthereumWallet, Vec<Address>),
    pub signal_rx: broadcast::Receiver<SignalType>,
    pub repository: Arc<SearcherRepository>,
}

// impl of exex
impl SearcherExEx {
    pub fn new(
        wallet: (EthereumWallet, Vec<Address>),
        signal_rx: broadcast::Receiver<SignalType>,
        repository: Arc<SearcherRepository>,
    ) -> Self {
        Self { wallet, signal_rx, repository }
    }

    pub async fn exex<Node>(
        self,
        mut ctx: ExExContext<Node>,
        config: StrategyConfig,
    ) -> Result<impl Future<Output = Result<()>>>
    where
        Node: FullNodeComponents,
        Node::Pool: TransactionPool<Transaction = EthPooledTransaction>,
        <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider:
            BlockHashReader + StateCommitmentProvider,
    {
        let wallet = self.wallet.clone();
        let signal_rx = self.signal_rx.resubscribe();
        let repository = self.repository.clone();

        Ok(async move {
            let relayer_pool =
                Arc::new(RelayerPool::new(ctx.components.clone(), wallet, signal_rx).await?);
            let relayer_tx = relayer_pool.start().await?;
            tracing::info!(
                target: "reth-exex",
                action = "relayer_pool_start",
                "Starting Relayer Pool"
            );

            let chain_id: u64 = ctx.components.network().chain_id();
            while let Some(notification) = ctx.notifications.next().await {
                if let Ok(ExExNotification::ChainCommitted { new: chain }) = notification {
                    let block = chain.tip();
                    let block_num = block.number();
                    let num_hash = block.num_hash();
                    let mut path_finder = PathFinder::new(&config);
                    let bytecode = path_finder.get_code();
                    let vault = path_finder.get_vault();

                    // extension is not setup yet, skip
                    if bytecode.is_empty() {
                        ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                        continue;
                    }
                    // 1. Create a read-only database provider that we can use to get latest state
                    let database_provider: <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider = ctx
                        .provider()
                        .database_provider_ro()?;
                    let latest_state_provider = LatestStateProviderRef::new(&database_provider);

                    // 2. Get the pending transactions from the transaction pool
                    let pending_txs = ctx
                        .components
                        .pool()
                        .pending_transactions()
                        .iter()
                        .map(|tx| tx.transaction.clone())
                        .collect();

                    // 3. Filter candidates by path finder based on the latest state and pending
                    // transactions
                    path_finder.set_last_state(latest_state_provider);
                    let candidates = repository.get_candidates(chain_id)?;
                    let profitable_candidates =
                        path_finder.find_profitable_candidates(pending_txs, candidates.clone())?;

                    if profitable_candidates.is_none() {
                        tracing::info!(
                            target: "reth-exex",
                            action = "no_profitable_candidates",
                            height = block_num,
                            "No profitable candidates found"
                        );
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
                            Ok(_) => tracing::info!(
                                target: "reth-exex",
                                action = "send_candidates_to_relayer_pool",
                                height = block_num,
                                "Successfully sent candidates to relayer pool"
                            ),
                            Err(e) => tracing::error!(
                                target: "reth-exex",
                                action = "send_candidates_to_relayer_pool",
                                error = ?e,
                                height = block_num,
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
