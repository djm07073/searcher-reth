use std::{future::Future, sync::Arc};

use alloy::{network::EthereumWallet, signers::local::PrivateKeySigner};
use eyre::Result;
use futures_util::StreamExt;

use crate::{
    SearcherExtension,
    strategy::path_finding::{PathFinder, strategy::Strategy, types::executeCall},
};
use alloy_sol_types::SolCall;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::{FullNodeComponents, FullNodeTypes};
use reth_provider::{
    BlockHashReader, DatabaseProviderFactory, LatestStateProviderRef, StateCommitmentProvider,
};
use searcher_reth_relayer_pool::{RelayerMessage, RelayerPool};
use tokio::sync::RwLock;

use reth_tracing::tracing;
pub struct SearcherExEx;

// impl of exex
impl SearcherExEx {
    pub async fn exex<Node>(
        mut ctx: ExExContext<Node>,
        extension: Arc<RwLock<SearcherExtension>>,
        signers: Vec<PrivateKeySigner>,
    ) -> Result<impl Future<Output = Result<()>>>
    where
        Node: FullNodeComponents,
        <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider:
            BlockHashReader + StateCommitmentProvider,
    {
        Ok(async move {
            let extension = extension.read().await;
            let vault_address = extension.vault;
            let bytecode = extension.contract.clone();
            let candidates = extension.candidates.clone();

            let relayer_pool = Arc::new(RelayerPool::new(ctx.components.clone(), signers).await?);
            let channel = Arc::new(relayer_pool.start().await?);
            tracing::info!(
                target: "reth-exex",
                action = "relayer_pool_start",
                "Starting Relayer Pool"
            );
            // TODO: check mempool for filtered candidates
            // ctx.components.pool().pending_transactions_listener(origin, transaction);
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

                    // create a task to simulate contract execution in searcher executor parallel
                    let mut finder = PathFinder::new(latest_state_provider, bytecode.clone());
                    let filtered_candidates = finder.filter_candidates(
                        extension.vault,
                        candidates.clone(),
                        extension.max_profit_ratio,
                        extension.min_profit_ratio,
                    )?;

                    let channel = channel.clone();
                    tokio::spawn(async move {
                        let routes = filtered_candidates
                            .iter()
                            .map(|route| format!("{:?}", route))
                            .collect::<Vec<String>>()
                            .join(", ");
                        let calldata = (executeCall { routes: filtered_candidates }).abi_encode();
                        let result = channel
                            .send(RelayerMessage { to: vault_address, data: calldata })
                            .await;
                        if let Err(e) = result {
                            tracing::error!(
                                target: "reth-exex",
                                action = "send_calldata_to_relayer_pool",
                                error = ?e,
                                "Failed to send calldata to relayer pool"
                            );
                            return;
                        }
                        tracing::info!(
                            target: "reth-exex",
                            action = "send_calldata_to_relayer_pool",
                            routes = routes,
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
