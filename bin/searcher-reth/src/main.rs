use std::sync::Arc;

use clap::Parser;
use eyre::eyre;
use reth::chainspec::EthereumChainSpecParser;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::info;
use searcher_reth_extension::{SearcherExtension, SetupArgs, exex::SearcherExEx};
use searcher_reth_repository::SearcherRepository;
use searcher_reth_rpc::{SearcherRpc, SearcherRpcApiServer};
use tokio::{net::UnixDatagram, sync::RwLock};

fn main() -> eyre::Result<()> {
    // database
    reth::cli::Cli::<EthereumChainSpecParser, SetupArgs>::parse().run(|builder, args| async move {
        let sock = Arc::new(UnixDatagram::unbound()?);
        let socket_path = std::env::var("SOCKET_PATH").unwrap_or("tmp/searcher.sock".to_string());
        let vault_address = std::env::var("VAULT_ADDRESS").expect("VAULT_ADDRESS must be set");
        sock.connect(socket_path)?;

        let db_path = builder.config().datadir().db().join("searcher.db");
        let chain_id = builder.config().chain.chain.id();
        let repository = Arc::new(SearcherRepository::new(db_path.to_str().unwrap()).await?);
        let extension = Arc::new(RwLock::new(SearcherExtension::new(vault_address, args).unwrap()));
        let extension_for_rpc = extension.clone();
        let extension_for_exex = extension.clone();

        let handle = builder
            .node(EthereumNode::default())
            .extend_rpc_modules(move |ctx| {
                let searcher_rpc: SearcherRpc = std::thread::spawn(move || {
                    let rt =
                        tokio::runtime::Runtime::new().expect("failed to spawn blocking runtime");
                    rt.block_on(SearcherRpc::new(chain_id, extension_for_rpc, repository.clone()))
                })
                .join()
                .map_err(|_| eyre!("failed to join ShadowRpc thread"))
                .unwrap();
                // TODO: change to auth merge
                ctx.modules
                    .merge_configured(searcher_rpc.into_rpc())
                    .map_err(|e| eyre!("failed to extend w/ SearcherRpc: {e}"))?;
                info!(target : "reth-exex", info = "RPC module extended successfully");
                Ok(())
            })
            .install_exex("SearcherExEx", {
                move |ctx| {
                    let exex = SearcherExEx::exex(ctx, extension_for_exex, sock.clone());
                    info!(target = "reth-exex", info = "SearcherExEx installed successfully");
                    exex
                }
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
