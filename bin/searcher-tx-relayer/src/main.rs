mod relayer;
mod config;
mod socket;

use alloy_primitives::Address;
use config::Config;
use eyre::Result;
use relayer::RelayerPool;
use reth_tracing::tracing;
use socket::SocketHandler;
use tokio::{ signal::unix::{ signal, SignalKind }, spawn };
use std::sync::{ atomic::{ AtomicBool, Ordering }, Arc };

#[tokio::main]
async fn main() -> Result<()> {
    tracing::info!("Starting tx-relayer service...");
    let config = Config::new()?;
    let wallets = config.get_wallets()?;
    let ipc = config.get_ipc();

    let pool = Arc::new(RelayerPool::new(ipc.clone(), wallets).await?);
    let socket = SocketHandler::new(config.network.socket_path)?;
    let to = config.searcher.vault_address.parse::<Address>()?;

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut switch = signal(SignalKind::user_defined1())?;
    let is_running = Arc::new(AtomicBool::new(false));
    let is_running_clone = is_running.clone();
    tokio::select! {
        result = handle_messages(to ,pool, &socket, is_running) => {
            if let Err(e) = result {
                tracing::error!("Error handling messages: {}", e);
            }
        }
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM");
            std::process::exit(0);
        }
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT");
            std::process::exit(0);
            
        }
        _ = switch.recv() => {
            tracing::info!("Received SIGUSR1 (pause)");
            is_running_clone.store(!is_running_clone.load(Ordering::SeqCst), Ordering::SeqCst);
        }

    }

    socket.cleanup();
    tracing::info!("Shutdown complete");
    Ok(())
}

async fn handle_messages(
    to: Address,
    pool: Arc<RelayerPool>,
    socket: &SocketHandler,
    is_running: Arc<AtomicBool>
) -> Result<()> {
    let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    loop {
        match socket.receive_data().await {
            Ok(data) => {
                let pool = pool.clone();
                let to = to.clone();

                if !is_running.load(Ordering::SeqCst) {
                    tracing::info!("Service paused, skipping transaction {:?}", data);
                    continue;
                }
                let handle = spawn(async move {
                    match pool.send_transaction(to, data).await {
                        Ok(hash) => tracing::info!("Transaction sent: {:?}", hash),
                        Err(e) => tracing::error!("Transaction failed: {}", e),
                    }
                });
                handles.push(handle);
            }
            Err(e) => tracing::error!("Failed to receive data: {}", e),
        }
        handles.retain(|handle| !handle.is_finished());
    }
}
