pub trait SearcherExEx {
    pub async fn init<Node: FullNodeComponents>(
        ctx: ExExContext<Node>,
        bytecode: Bytecode,
        route_paths: Vec<RoutePath> // swap_router, src token ,dst token
    ) -> Result<impl Future<Output = Result<()>>>;

    async fn exex<Node: FullNodeComponents>(
        &self,
        mut ctx: ExExContext<Node>,
        bytecode: Bytecode,
        route_paths: Vec<RoutePath> // swap_router, src token ,dst token
    ) -> Result<()>;
}

// impl of exex
impl SearcherExEx for SearcherExtenstion {
    /// The initialization logic of the ExEx is just an async function.
    pub async fn init<Node: FullNodeComponents>(
        ctx: ExExContext<Node>,
        bytecode: Bytecode,
        route_paths: Vec<RoutePath> // swap_router, src token ,dst token
    ) -> Result<impl Future<Output = Result<()>>> {
        Ok(async { self.exex(ctx, bytecode, route_paths).await })
    }

    ///
    async fn exex<Node: FullNodeComponents>(
        &self,
        mut ctx: ExExContext<Node>,
        bytecode: Bytecode,
        route_paths: Vec<RoutePath> // swap_router, src token ,dst token
    ) -> Result<()> {
        while let Some(notification) = ctx.notifications.recv().await {
            match notification {
                ExExNotification::ChainCommitted { new: chain } => {
                    // TODO if bytecode is zero. skipped
                    // Create a read-only database provider that we can use to get lastest state
                    let database_provider = ctx.provider().database_provider_ro()?;
                    let provider = LatestStateProviderRef::new(
                        database_provider.tx_ref(),
                        database_provider.static_file_provider().clone()
                    );
                    // create a task to simulate contract execution in searcher executor parallel (1000)
                    // transfer the top paths to trading bot
                    ctx.events.send(ExExEvent::FinishedHeight(chain.tip().number))?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}
