mod macros;

use std::sync::{Arc, RwLock};

use clap::Parser;
use reth::chainspec::EthereumChainSpecParser;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::error;
use searcher_reth_extension::{
    relayer_pool::WalletPool,
    strategy::{liquidator::Liquidator, path_finding::PathFinder, profit_reporter::init_reporter},
    util::signal_manager::SignalManager,
};
use searcher_reth_manager::{
    common::{LIQUIDATOR_EXEX_ID, PATH_FINDER_EXEX_ID},
    manager::ConfigManager,
};

fn main() -> eyre::Result<()> {
    let config = Arc::new(RwLock::new(ConfigManager::from_file("env.toml")?));
    reth::cli::Cli::<EthereumChainSpecParser>::parse().run(|builder, _| async move {
        let telegram_cfg = config.read().unwrap().get_telegram();
        let wallet = config.read().unwrap().get_wallet().unwrap();
        let wallet = Arc::new(WalletPool::new(wallet));
        if let Some(tg) = telegram_cfg {
            init_reporter(tg.bot_token, tg.chat_id, tg.report_interval_secs);
        }
        // Spawn signal manager to handle signals
        let signal_manager = SignalManager::new();
        let spawned_signal_manager = signal_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = spawned_signal_manager.clone().start_signal_handling().await {
                error!("Signal handler failed: {}", e);
            }
            std::process::exit(0);
        });

        // Install strategies
        let mut node_builder = builder.node(EthereumNode::default());
        node_builder = install_strategy!(
            node_builder,
            config,
            wallet,
            signal_manager,
            PATH_FINDER_EXEX_ID,
            PathFinder
        );
        node_builder = install_strategy!(
            node_builder,
            config,
            wallet,
            signal_manager,
            LIQUIDATOR_EXEX_ID,
            Liquidator
        );
        // TODO: Add other strategies here as needed
        let handle = node_builder.launch().await?;
        handle.wait_for_node_exit().await
    })
}
