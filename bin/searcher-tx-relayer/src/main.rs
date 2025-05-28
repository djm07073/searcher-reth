mod config;
mod relayer;
mod socket;
mod status;

use alloy_primitives::Address;
use config::IpcWalletConnector;
use eyre::Result;
use relayer::RelayerPool;
use reth_tracing::tracing;
use searcher_reth_config::SearcherConfig;
use socket::SocketHandler;
use status::Status;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use tokio::{
    signal::unix::{SignalKind, signal},
    spawn,
};

use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(default_value = "env.toml")]
    config: String,
}

const SERVICE_NAME: &str = "searhcer-tx-relayer";

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing::info!(
        event = "service_start",
        status = "initializing",
        "Starting tx-relayer service..."
    );
    let config = SearcherConfig::from_file(&cli.config)?;
    let wallets = config.get_wallets()?;
    let ipc = config.get_ipc();

    let _logger = searcher_reth_logger::init(SERVICE_NAME);

    let pool = Arc::new(RelayerPool::new(ipc.clone(), wallets).await?);
    let socket = SocketHandler::new(config.network.socket_path)?;
    let to = config.relayer.vault_address.parse()?;

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut switch = signal(SignalKind::user_defined1())?;
    let status = Arc::new(AtomicU8::new(Status::Paused as u8));
    let atomic_status = status.clone();
    tokio::select! {
        result = handle_messages(to ,pool, &socket, status) => {
            if let Err(e) = result {
                tracing::error!("Error handling messages: {}", e);
            }
        }
        _ = switch.recv() => {
            tracing::info!(
                event = "signal_received",
                signal = "SIGUSR1",
                "Received SIGUSR1 - Pausing/Resuming"
            );
            let current_status: Status = atomic_status.load(Ordering::SeqCst).into();
            match current_status {
                Status::Running => {
                    atomic_status.store(Status::Paused as u8, Ordering::SeqCst);
                    tracing::info!(
                        event = "status_change",
                        status = "paused",
                        previous_status = "running",
                        "Service paused"
                    );
                }
                Status::Paused => {
                    atomic_status.store(Status::Running as u8, Ordering::SeqCst);
                    tracing::info!(
                        event = "status_change",
                        status = "running",
                        previous_status = "paused",
                        "Service resumed"
                    );
                }
                _ => unreachable!(),
            }
        }
        _ = sigterm.recv() => {
            tracing::info!(
                event = "signal_received",
                signal = "SIGTERM",
                status = "stopping",
                "Received SIGTERM"
            );
            atomic_status.store(Status::Stopped as u8, Ordering::SeqCst);
            socket.cleanup();
            return Ok(());
        }
        _ = sigint.recv() => {
            tracing::info!(
                event = "signal_received",
                signal = "SIGINT",
                status = "stopping",
                "Received SIGINT"
            );
            atomic_status.store(Status::Stopped as u8, Ordering::SeqCst);
            socket.cleanup();
            return Ok(());

        }
    }

    Ok(())
}

async fn handle_messages(
    to: Address,
    pool: Arc<RelayerPool>,
    socket: &SocketHandler,
    status: Arc<AtomicU8>,
) -> Result<()> {
    let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    loop {
        match socket.receive_data().await {
            Ok(data) => match status.load(Ordering::SeqCst).into() {
                Status::Paused => {
                    tracing::info!(
                        event = "transaction_skip",
                        status = "paused",
                        data = ?data,
                        "Service paused, skipping transaction"
                    );
                    continue;
                }
                Status::Stopped => {
                    tracing::info!("Service stopped, exiting message handler");
                    break;
                }
                Status::Running => {
                    let pool = pool.clone();

                    let handle = spawn(async move {
                        match pool.send_transaction(to, data).await {
                            Ok(hash) => tracing::info!(
                                event = "transaction_sent",
                                status = "success",
                                tx_hash = ?hash,
                                "Transaction sent successfully"
                            ),
                            Err(e) => tracing::error!(
                                event = "transaction_failed",
                                status = "error",
                                error = %e,
                                "Transaction failed"
                            ),
                        }
                    });
                    handles.push(handle);
                }
            },
            Err(e) => tracing::error!("Failed to receive data: {}", e),
        }
        handles.retain(|handle| !handle.is_finished());
    }

    for handle in handles {
        if let Err(e) = handle.await {
            tracing::error!("Error waiting for transaction: {}", e);
        }
    }

    tracing::info!("Message handler stopped");
    Ok(())
}
