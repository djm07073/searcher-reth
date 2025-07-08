use std::sync::{ Arc, RwLock };

use clap::Parser;
use reth::chainspec::EthereumChainSpecParser;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::error;
use searcher_reth_extension::{
    exex::SearcherExEx,
    strategy::config::{ manager::ConfigManager, strategy::PATH_FINDER_EXEX_ID },
    util::signal_manager::SignalManager,
};

fn main() -> eyre::Result<()> {
    let config = Arc::new(RwLock::new(ConfigManager::from_file("env.toml")?));
    let wallet = config.read().unwrap().get_wallet().unwrap();

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
        node_builder = node_builder.install_exex(PATH_FINDER_EXEX_ID, {
            let wallet = wallet.clone();
            let signal_manager = signal_manager.clone();
            move |ctx| {
                let searcher_exex = SearcherExEx::new(wallet, signal_manager.subscribe(), config);
                searcher_exex.exex(PATH_FINDER_EXEX_ID, ctx)
            }
        });
        let handle = node_builder.launch().await?;
        handle.wait_for_node_exit().await
    })
}
