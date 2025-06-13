use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use alloy_sol_types::SolCall;
use eyre::Result;
use futures_util::StreamExt;
use std::{future::Future, sync::Arc};

use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::{FullNodeComponents, FullNodeTypes};
use reth_provider::{
    BlockHashReader, DatabaseProviderFactory, LatestStateProviderRef, StateCommitmentProvider,
};
use reth_tracing::tracing;
use reth_transaction_pool::{EthPooledTransaction, TransactionPool};
use tokio::sync::RwLock;

use crate::{
    SearcherExtension,
    relayer_pool::{RelayerMessage, RelayerPool},
    strategy::{
        core::Strategy,
        path_finding::{PathFinder, types::executeCall},
    },
};

pub struct SearcherExEx;

// impl of exex
impl SearcherExEx {
    pub async fn exex<Node>(
        mut ctx: ExExContext<Node>,
        extension: Arc<RwLock<SearcherExtension>>,
        wallet: (EthereumWallet, Vec<Address>),
    ) -> Result<impl Future<Output = Result<()>>>
    where
        Node: FullNodeComponents,
        Node::Pool: TransactionPool<Transaction = EthPooledTransaction>,
        <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider:
            BlockHashReader + StateCommitmentProvider,
    {
        Ok(async move {
            let extension = extension.read().await;
            let vault_address = extension.vault;
            let bytecode = extension.contract.clone();
            let candidates = extension.candidates.clone();

            let relayer_pool = Arc::new(RelayerPool::new(ctx.components.clone(), wallet).await?);
            let relayer_tx = Arc::new(relayer_pool.start().await?);
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
                    let pending_txs = ctx
                        .components
                        .pool()
                        .pending_transactions()
                        .iter()
                        .map(|tx| tx.transaction.clone())
                        .collect();

                    // Filter candidates by path finder based on the latest state and pending
                    // transactions
                    let mut finder = PathFinder::new(latest_state_provider, bytecode.clone());
                    let filtered_candidates = finder.filter_candidates(
                        extension.vault,
                        pending_txs,
                        candidates.clone(),
                        extension.max_profit_ratio,
                        extension.min_profit_ratio,
                    )?;

                    // Send the filtered candidates to the relayer pool for broadcasting
                    // transactions
                    let relayer_channel = relayer_tx.clone();
                    tokio::spawn(async move {
                        let calldata =
                            (executeCall { routes: filtered_candidates.clone() }).abi_encode();
                        let message = RelayerMessage { to: vault_address, calldata };
                        relayer_channel.send(message).await.unwrap_or_else(|e| {
                            tracing::error!(
                                target: "reth-exex",
                                action = "send_candidates_to_relayer_pool",
                                error = ?e,
                                "Failed to send calldata to relayer pool"
                            );
                        });

                        // Log the routes being sent
                        let routes = filtered_candidates
                            .clone()
                            .iter()
                            .map(|route| format!("{:?}", route))
                            .collect::<Vec<String>>();
                        let route_len = routes.len();
                        tracing::info!(
                            target: "reth-exex",
                            action = "send_candidates_to_relayer_pool",
                            route_len = route_len,
                            routes = routes.join(", "),
                            "Sending encoded calldata to socket"
                        );
                    });

                    ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                }
            }

            Ok(())
        })
    }
}
