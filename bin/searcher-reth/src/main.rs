use clap::Parser;
use eyre::eyre;
use reth::chainspec::EthereumChainSpecParser;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::{error, info};
use searcher_reth_config::SearcherConfig;
use searcher_reth_extension::{SearcherExtension, SetupArgs, exex::SearcherExEx};
use searcher_reth_repository::SearcherRepository;
use searcher_reth_rpc::{SearcherRpc, SearcherRpcApiServer};
use searcher_reth_util::{logger, signal_manager::SignalManager};
use std::sync::Arc;
use tokio::sync::RwLock;

const SERVICE_NAME: &str = "searcher-reth";

fn main() -> eyre::Result<()> {
    let _logger = logger::init(SERVICE_NAME).map_err(|e| eyre!("Logger init failed: {}", e))?;
    let config = SearcherConfig::from_file("env.toml")?;
    let vault = config.relayer.vault.parse().unwrap_or_default();
    let wallet = config.relayer.get_wallet().unwrap();
    let repository = Arc::new(SearcherRepository::new(config.database.path.to_str().unwrap()));

    let signal_manager = SignalManager::new();
    let spawned_signal_manager = signal_manager.clone();
    tokio::spawn(async move {
        if let Err(e) = spawned_signal_manager.start_signal_handling().await {
            error!("Signal handler failed: {}", e);
        }
    });

    reth::cli::Cli::<EthereumChainSpecParser, SetupArgs>::parse().run(|builder, args| async move {
        let chain_id = builder.config().chain.chain.id();
        let extension = Arc::new(RwLock::new(SearcherExtension::new(vault, args).unwrap()));
        let extension_for_rpc = extension.clone();
        let extension_for_exex = extension.clone();

        let exex_signal_rx = signal_manager.subscribe();
        let handle = builder
            .node(EthereumNode::default())
            .extend_rpc_modules(move |ctx| {
                let searcher_rpc: SearcherRpc = std::thread::spawn(move || {
                    let rt =
                        tokio::runtime::Runtime::new().expect("failed to spawn blocking runtime");
                    rt.block_on(SearcherRpc::new(chain_id, extension_for_rpc, repository.clone()))
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
            .install_exex("SearcherExEx", move |ctx| {
                let exex = SearcherExEx::exex(ctx, extension_for_exex, wallet, exex_signal_rx);
                info!(
                    target: "reth-exex",
                    event = "exex_installation",
                    status = "success",
                    "SearcherExEx installed successfully"
                );
                exex
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
