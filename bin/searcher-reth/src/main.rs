use clap::Parser;
use eyre::eyre;
use reth::chainspec::EthereumChainSpecParser;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::error;
use searcher_reth_extension::{
    exex::SearcherExEx,
    repository::{
        SearcherRepository,
        config::{Config, SearcherConfig, strategy::PATH_FINDER_EXEX_ID},
    },
    util::{logger, signal_manager::SignalManager},
};

use std::{str, sync::Arc};

const SERVICE_NAME: &str = "searcher-reth";

fn main() -> eyre::Result<()> {
    let _logger = logger::init(SERVICE_NAME).map_err(|e| eyre!("Logger init failed: {}", e))?;
    let config: SearcherConfig = Config::from_file("env.toml")?;
    let wallet = config.relayer.get_wallet().unwrap();
    let repository = Arc::new(SearcherRepository::new(config.database.path.to_str().unwrap()));

    reth::cli::Cli::<EthereumChainSpecParser>::parse().run(|builder, _| async move {
        // Spawn signal manager to handle signals
        let signal_manager = SignalManager::new();
        let spawned_signal_manager = signal_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = spawned_signal_manager.clone().start_signal_handling().await {
                error!("Signal handler failed: {}", e);
            }
            std::process::exit(0);
        });

        let mut node_builder = builder.node(EthereumNode::default());
        let strategy_config = config.strategies.get(PATH_FINDER_EXEX_ID).unwrap();
        node_builder = node_builder.install_exex(PATH_FINDER_EXEX_ID, {
            let wallet = wallet.clone();
            let repository = repository.clone();
            let signal_manager = signal_manager.clone();
            let strategy_config = strategy_config.clone();

            move |ctx| {
                let searcher_exex =
                    SearcherExEx::new(wallet, signal_manager.subscribe(), repository);
                searcher_exex.exex(ctx, strategy_config)
            }
        });
        let handle = node_builder.launch().await?;
        handle.wait_for_node_exit().await
    })
}
