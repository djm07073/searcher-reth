use clap::Parser;
use eyre::eyre;
use reth::chainspec::EthereumChainSpecParser;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::{ error, info };
use searcher_reth_config::SearcherConfig;
use searcher_reth_extension::{
    core::{ SearcherExtension, SetupArgs },
    exex::SearcherExEx,
    strategy::PathFinder,
};
use searcher_reth_repository::SearcherRepository;
use searcher_reth_util::{ logger, signal_manager::SignalManager };
use std::sync::Arc;
use tokio::sync::RwLock;

const SERVICE_NAME: &str = "searcher-reth";

fn main() -> eyre::Result<()> {
    let _logger = logger::init(SERVICE_NAME).map_err(|e| eyre!("Logger init failed: {}", e))?;
    let config = SearcherConfig::from_file("env.toml")?;
    let vault = config.relayer.vault.parse().unwrap_or_default();
    let wallet = config.relayer.get_wallet().unwrap();
    let repository = Arc::new(SearcherRepository::new(config.database.path.to_str().unwrap()));

    reth::cli::Cli::<EthereumChainSpecParser, SetupArgs>::parse().run(|builder, args| async move {
        // Spawn signal manager to handle OS signals
        let signal_manager = SignalManager::new();
        let spawned_signal_manager = signal_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = spawned_signal_manager.clone().start_signal_handling().await {
                error!("Signal handler failed: {}", e);
            }
            std::process::exit(0);
        });

        // Initialize the extension
        let chain_id = builder.config().chain.chain.id();
        let handle = builder
            .node(EthereumNode::default())
            .install_exex("Searcher ExEx - Path Finder", move |ctx| {
                let extension = Arc::new(
                    RwLock::new(SearcherExtension::<PathFinder<_>>::new(vault, args).unwrap())
                );
                let exex = SearcherExEx::exex(ctx, extension, wallet, signal_manager.subscribe());
                info!(
                    target: "reth-exex",
                    event = "exex_installation",
                    status = "success",
                    "Searcher ExEx - Path Finder installed successfully"
                );
                exex
            })
            .launch().await?;

        handle.wait_for_node_exit().await
    })
}
