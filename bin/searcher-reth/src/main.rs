use std::sync::{ Arc, RwLock };

use clap::Parser;
use reth::chainspec::EthereumChainSpecParser;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::{ self, error };
use searcher_reth_extension::{
    exex::SearcherExEx,
    relayer_pool::WalletPool,
    util::signal_manager::SignalManager,
};
use searcher_reth_manager::{ common::PATH_FINDER_EXEX_ID, manager::ConfigManager, SignalType };

fn main() -> eyre::Result<()> {
    let config = Arc::new(RwLock::new(ConfigManager::from_file("env.toml")?));
    let wallet = config.read().unwrap().get_wallet().unwrap();
    let wallet = Arc::new(WalletPool::new(wallet));
    tracing::info!("Starting searcher-reth with wallet: {:?}", wallet.signers());

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

        // Handle configuration reload signals
        let mut config_signal_rx = signal_manager.subscribe();
        let config_to_reload = config.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(signal) = config_signal_rx.recv().await {
                    if SignalType::Reload == signal {
                        tracing::info!(
                            target: "reth-exex",
                            event = "reload_config",
                            "Reloading configuration"
                        );
                        if let Err(e) = config_to_reload.write().unwrap().reload() {
                            error!("Failed to reload config: {}", e);
                        }
                    }
                }
            }
        });

        // Install Exex for various st 
        let mut node_builder = builder.node(EthereumNode::default());
        node_builder = node_builder.install_exex(PATH_FINDER_EXEX_ID, {
            move |ctx| {
                let searcher_exex = SearcherExEx::new(wallet, signal_manager.subscribe(), config);
                searcher_exex.exex(PATH_FINDER_EXEX_ID, ctx)
            }
        });
        let handle = node_builder.launch().await?;
        handle.wait_for_node_exit().await
    })
}
