use std::sync::Arc;

use eyre::eyre;
use clap::Parser;
use reth::chainspec::EthereumChainSpecParser;
use reth_node_ethereum::EthereumNode;
use searcher_reth_extension::{
    exex::SearcherExEx,
    rpc::{ SearcherRpc, SearcherRpcApiServer },
    SearcherExtension,
    SetupArgs,
};

fn main() -> eyre::Result<()> {
    // database
    reth::cli::Cli::<EthereumChainSpecParser, SetupArgs>::parse().run(|builder, args| async move {
        let extension = Arc::new(SearcherExtension::new(args).unwrap());
        let extension_for_rpc = extension.clone();
        let extension_for_exex = extension.clone();
        let handle = builder
            .node(EthereumNode::default())
            .extend_rpc_modules(move |ctx| {
                let provider = ctx.provider().clone();
                let searcher_rpc = std::thread
                    ::spawn(move || SearcherRpc::new(provider, extension_for_rpc))
                    .join()
                    .map_err(|_| eyre!("failed to join SearcherRpc thread"))?;
                ctx.modules
                    .merge_configured(searcher_rpc.into_rpc())
                    .map_err(|e| eyre!("failed to extend w/ SearcherRpc: {e}"))?;
                Ok(())
            })
            .install_exex("SearcherExEx", {
                move |ctx| { SearcherExEx::exex(ctx, extension_for_exex) }
            })
            .launch().await?;

        handle.wait_for_node_exit().await
    })
}
