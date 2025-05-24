use std::{ future::Future, sync::Arc };

use eyre::Result;
use futures_util::StreamExt;

use crate::{
    SearcherExtension,
    strategy::path_finding::{ PathFinder, strategy::Strategy, types::executeCall },
};
use alloy_sol_types::SolCall;
use reth_exex::{ ExExContext, ExExEvent, ExExNotification };
use reth_node_api::{ FullNodeComponents, FullNodeTypes };
use reth_provider::{
    BlockHashReader,
    DatabaseProviderFactory,
    LatestStateProviderRef,
    StateCommitmentProvider,
};
use tokio::{ net::UnixDatagram, sync::RwLock };

pub struct SearcherExEx;

// impl of exex
impl SearcherExEx {
    pub async fn exex<Node>(
        mut ctx: ExExContext<Node>,
        extension: Arc<RwLock<SearcherExtension>>,
        sock: Arc<UnixDatagram>
    )
        -> Result<impl Future<Output = Result<()>>>
        where
            Node: FullNodeComponents,
            <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider: BlockHashReader +
                StateCommitmentProvider
    {
        Ok(async move {
            let extension = extension.read().await;
            let bytecode = extension.contract.clone();
            let candidates = extension.candidates.clone();

            while let Some(notification) = ctx.notifications.next().await {
                if let Ok(ExExNotification::ChainCommitted { new: chain }) = notification {
                    let block = chain.tip();
                    let num_hash = block.num_hash();
                    if bytecode.clone().is_empty() {
                        ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                        continue;
                    }
                    // Create a read-only database provider that we can use to get lastest state
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
                        extension.min_profit_ratio
                    )?;

                    let calldata = (executeCall { routes: filtered_candidates }).abi_encode();

                    // send the encoded data to the socket
                    let sock = sock.clone();
                    tokio::spawn(async move {
                        reth_tracing::tracing::info!(target: "reth-exex", "Sending data to socket");
                        sock.send(&calldata).await.unwrap();
                    });

                    ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                }
            }

            Ok(())
        })
    }
}
