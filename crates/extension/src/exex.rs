use std::future::Future;

use eyre::Result;
use futures_util::StreamExt;

use reth_exex::{ ExExContext, ExExEvent, ExExNotification };
use reth_node_api::{ FullNodeComponents, FullNodeTypes };

use reth_provider::{
    BlockHashReader,
    DatabaseProviderFactory,
    LatestStateProviderRef,
    StateCommitmentProvider,
};
use revm::state::Bytecode;
use searcher_reth_path_finder::{ PathFinder, RoutePath };

use crate::SearcherExtension;

pub trait SearcherExEx<Node>
    where
        Node: FullNodeComponents,
        <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider: BlockHashReader +
            StateCommitmentProvider
{
    async fn init(&self, ctx: ExExContext<Node>) -> Result<impl Future<Output = Result<()>>>;

    async fn exex(
        &self,
        ctx: ExExContext<Node>,
        bytecode: Bytecode,
        route_paths: Vec<RoutePath> // swap_router, src token ,dst token
    ) -> Result<()>;
}

// impl of exex
impl<Node> SearcherExEx<Node>
    for SearcherExtension
    where
        Node: FullNodeComponents,
        <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider: BlockHashReader +
            StateCommitmentProvider
{
    /// The initialization logic of the ExEx is just an async function.
    async fn init(
        &self,
        ctx: ExExContext<Node> // dex type, swap_router, src token ,dst token
    ) -> Result<impl Future<Output = Result<()>>> {
        Ok(async { self.exex(ctx, self.contract.clone(), self.route_paths.clone()).await })
    }

    ///
    async fn exex(
        &self,
        mut ctx: ExExContext<Node>,
        bytecode: Bytecode,
        route_paths: Vec<RoutePath> // swap_router, src token ,dst token
    ) -> Result<()> {
        while let Some(notification) = ctx.notifications.next().await {
            match notification {
                Ok(ExExNotification::ChainCommitted { new: chain }) => {
                    let block = chain.tip();
                    let num_hash = block.num_hash();
                    // TODO if bytecode is zero. skipped
                    if bytecode.is_empty() {
                        ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                        continue;
                    }
                    // Create a read-only database provider that we can use to get lastest state
                    let database_provider: <<Node as FullNodeTypes>::Provider as DatabaseProviderFactory>::Provider = ctx
                        .provider()
                        .database_provider_ro()?;
                    let latest_state_provider = LatestStateProviderRef::new(&database_provider);
                    // create a task to simulate contract execution in searcher executor parallel
                    let mut finder = PathFinder::new(latest_state_provider, self.contract.clone());
                    let optimal_paths = finder.find_optimal_paths(
                        route_paths.clone(),
                        self.max_profit_ratio,
                        self.min_profit_ratio
                    )?;
                    // transfer the optimal paths to trading bot in non-blocking way

                    ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
