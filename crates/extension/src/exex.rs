use std::{ future::Future, sync::Arc };

use eyre::Result;
use futures_util::StreamExt;

use reth_exex::{ ExExContext, ExExEvent, ExExNotification };
use reth_node_api::{ FullNodeComponents, FullNodeTypes };
use alloy_sol_types::SolValue;
use reth_provider::{
    BlockHashReader,
    DatabaseProviderFactory,
    LatestStateProviderRef,
    StateCommitmentProvider,
};
use tokio::sync::RwLock;
use tokio::net::UnixDatagram;
use crate::{ strategy::path_finder::{ PathFinder, searcher::Strategy }, SearcherExtension };

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
            let route_paths = extension.route_paths.clone();

            while let Some(notification) = ctx.notifications.next().await {
                match notification {
                    Ok(ExExNotification::ChainCommitted { new: chain }) => {
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
                        let optimal_paths = finder.filter_candidates(
                            route_paths.clone(),
                            extension.max_profit_ratio,
                            extension.min_profit_ratio
                        )?;

                        let encoded_paths = optimal_paths.abi_encode();

                        let sock = sock.clone();
                        tokio::spawn(async move {
                            sock.send(&encoded_paths).await.unwrap();
                        });
                        // transfer optimal_paths to the socket
                        ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                    }
                    _ => {}
                }
            }

            Ok(())
        })
    }
}
