use reth_tracing::tracing;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use tokio::signal::unix::{SignalKind, signal};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Running = 1,
    Paused = 0,
    Stopped = 2,
}

impl From<u8> for Status {
    fn from(value: u8) -> Self {
        match value {
            0 => Status::Paused,
            1 => Status::Running,
            2 => Status::Stopped,
            _ => Status::Stopped,
        }
    }
}

pub(crate) async fn handle_signals(status: Arc<AtomicU8>) -> Result<(), eyre::Error> {
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut switch = signal(SignalKind::user_defined1())?;

    loop {
        tokio::select! {
            _ = switch.recv() => {
                toggle_service_status(status.clone()).await;
            }
            _ = sigterm.recv() => {
                handle_shutdown_signal("SIGTERM", status.clone()).await;
                break;
            }
            _ = sigint.recv() => {
                handle_shutdown_signal("SIGINT", status.clone()).await;
                break;
            }
        }
    }
    Ok(())
}

async fn toggle_service_status(status: Arc<AtomicU8>) {
    let current_status: Status = status.load(Ordering::SeqCst).into();
    match current_status {
        Status::Running => {
            status.store(Status::Paused as u8, Ordering::SeqCst);
            tracing::info!(
                event = "status_change",
                status = "paused",
                previous_status = "running",
                "Service paused"
            );
        }
        Status::Paused => {
            status.store(Status::Running as u8, Ordering::SeqCst);
            tracing::info!(
                event = "status_change",
                status = "running",
                previous_status = "paused",
                "Service resumed"
            );
        }
        Status::Stopped => unreachable!(),
    }
}

async fn handle_shutdown_signal(signal_type: &str, status: Arc<AtomicU8>) {
    tracing::info!(
        event = "signal_received",
        signal = signal_type,
        status = "stopping",
        "Received {signal_type}"
    );
    status.store(Status::Stopped as u8, Ordering::SeqCst);
}
