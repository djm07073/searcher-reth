use std::sync::Arc;

use clap::Parser;
use eyre::eyre;
use reth::chainspec::EthereumChainSpecParser;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::info;
use searcher_reth_config::SearcherConfig;
use searcher_reth_extension::{ SearcherExtension, SetupArgs, exex::SearcherExEx };
use searcher_reth_repository::SearcherRepository;
use searcher_reth_rpc::{ SearcherRpc, SearcherRpcApiServer };
use tokio::sync::RwLock;

const SERVICE_NAME: &str = "searcher-reth";

fn main() -> eyre::Result<()> {
    let _logger = searcher_reth_logger::init(SERVICE_NAME);
    let config = SearcherConfig::from_file("env.toml")?;
    let vault_address = config.relayer.vault_address
        .parse()
        .map_err(|_| eyre!("Invalid vault address"))?;
    let wallet = config.relayer.get_wallet().unwrap();
    let repository = Arc::new(SearcherRepository::new(config.database.path.to_str().unwrap()));
    // database
    reth::cli::Cli::<EthereumChainSpecParser, SetupArgs>::parse().run(|builder, args| async move {
        let chain_id = builder.config().chain.chain.id();
        let extension = Arc::new(RwLock::new(SearcherExtension::new(vault_address, args).unwrap()));
        let extension_for_rpc = extension.clone();
        let extension_for_exex = extension.clone();
        let handle = builder
            .node(EthereumNode::default())
            .extend_rpc_modules(move |ctx| {
                let searcher_rpc: SearcherRpc = std::thread
                    ::spawn(move || {
                        let rt = tokio::runtime::Runtime
                            ::new()
                            .expect("failed to spawn blocking runtime");
                        rt.block_on(
                            SearcherRpc::new(chain_id, extension_for_rpc, repository.clone())
                        )
                    })
                    .join()
                    .map_err(|_| eyre!("failed to join Searcher Rpc thread"))
                    .unwrap();
                ctx.modules
                    .merge_configured(searcher_rpc.into_rpc())
                    .map_err(|e| eyre!("failed to extend w/ SearcherRpc: {e}"))?;
                info!(
                    target: "reth-exex",
                    event = "rpc_extension",
                    status = "success",
                    "RPC module extended successfully"
                );
                Ok(())
            })
            .install_exex("SearcherExEx", {
                move |ctx| {
                    let exex = SearcherExEx::exex(ctx, extension_for_exex, wallet);
                    info!(
                        target: "reth-exex",
                        event = "exex_installation",
                        status = "success",
                        "SearcherExEx installed successfully"
                    );
                    exex
                }
            })
            .launch().await?;

        handle.wait_for_node_exit().await
    })
}
