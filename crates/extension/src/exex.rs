use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use eyre::Result;
use futures_util::StreamExt;
use searcher_reth_util::signal_manager::SignalType;
use std::{ future::Future, sync::Arc };

use reth_exex::{ ExExContext, ExExEvent, ExExNotification };
use reth_node_api::{ FullNodeComponents, FullNodeTypes };
use reth_provider::{
    BlockHashReader,
    DatabaseProviderFactory,
    LatestStateProviderRef,
    StateCommitmentProvider,
};
use reth_tracing::tracing;
use reth_transaction_pool::{ EthPooledTransaction, TransactionPool };
use tokio::sync::{ RwLock, broadcast };

use crate::{
    core::SearcherExtension,
    relayer_pool::{ RelayerMessage, RelayerPool },
    strategy::Strategy,
};

use std::marker::PhantomData;

pub struct SearcherExEx<S> {
    _marker: PhantomData<S>,
}

// impl of exex
impl<S> SearcherExEx<S> {
    pub async fn exex<Node>(
        mut ctx: ExExContext<Node>,
        extension: Arc<RwLock<SearcherExtension<'static, S>>>,
        wallet: (EthereumWallet, Vec<Address>),
        signal_rx: broadcast::Receiver<SignalType>
    )
        -> Result<impl Future<Output = Result<()>>>
        where
            S: for<'a> Strategy<
                'a,
                DB = <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider
            >,
            Node: FullNodeComponents,
            Node::Pool: TransactionPool<Transaction = EthPooledTransaction>,
            <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider: BlockHashReader +
                StateCommitmentProvider
    {
        Ok(async move {
            let extension = extension.read().await;
            let vault_address = extension.vault;
            let bytecode = extension.contract.clone();
            let candidates = extension.candidates.clone();

            let relayer_pool = Arc::new(
                RelayerPool::new(ctx.components.clone(), wallet, signal_rx).await?
            );
            let relayer_tx = relayer_pool.start().await?;
            tracing::info!(
                target: "reth-exex",
                action = "relayer_pool_start",
                "Starting Relayer Pool"
            );

            while let Some(notification) = ctx.notifications.next().await {
                if let Ok(ExExNotification::ChainCommitted { new: chain }) = notification {
                    let block = chain.tip();
                    let num_hash = block.num_hash();
                    // extension is not setup yet, skip
                    if bytecode.clone().is_empty() {
                        ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                        continue;
                    }
                    // Create a read-only database provider that we can use to get latest state
                    let database_provider: <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider = ctx
                        .provider()
                        .database_provider_ro()?;
                    let latest_state_provider = LatestStateProviderRef::new(&database_provider);

                    // Get the pending transactions from the transaction pool
                    let pending_txs = ctx.components
                        .pool()
                        .pending_transactions()
                        .iter()
                        .map(|tx| tx.transaction.clone())
                        .collect();

                    // Filter candidates by path finder based on the latest state and pending
                    // transactions
                    let mut strategy = S::create(latest_state_provider, bytecode.clone());
                    let calldata = strategy.find_profitable_candidates(
                        extension.vault,
                        pending_txs,
                        candidates.clone(),
                        extension.max_profit_ratio,
                        extension.min_profit_ratio
                    )?;

                    // Send the filtered candidates to the relayer pool for broadcasting
                    // transactions
                    let relayer_channel = relayer_tx.clone();
                    tokio::spawn(async move {
                        let message = RelayerMessage { to: vault_address, calldata };
                        relayer_channel.send(message).await.unwrap_or_else(|e| {
                            tracing::error!(
                                target: "reth-exex",
                                action = "send_candidates_to_relayer_pool",
                                error = ?e,
                                "Failed to send calldata to relayer pool"
                            );
                        });
                    });

                    ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                }
            }

            Ok(())
        })
    }
}
