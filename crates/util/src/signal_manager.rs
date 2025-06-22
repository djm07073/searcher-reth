use reth_tracing::tracing;
use tokio::{
    signal::unix::{SignalKind, signal},
    sync::broadcast,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SignalType {
    Shutdown,
    Pause,
    Resume,
    Reload,
}

#[derive(Debug, Clone)]
pub struct SignalManager {
    signal_tx: broadcast::Sender<SignalType>,
}

impl Default for SignalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalManager {
    pub fn new() -> Self {
        let (signal_tx, _) = broadcast::channel(16);

        Self { signal_tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SignalType> {
        self.signal_tx.subscribe()
    }

    pub async fn start_signal_handling(&self) -> Result<(), eyre::Error> {
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigusr1 = signal(SignalKind::user_defined1())?;
        let mut sigusr2 = signal(SignalKind::user_defined2())?;

        tracing::info!(
            event = "signal_handler_start",
            "Signal handler started, listening for SIGTERM, SIGINT, SIGUSR1, SIGHUP"
        );

        let mut is_paused = false;

        loop {
            tokio::select! {
                _ = sigusr1.recv() => {
                    is_paused = !is_paused;
                    let signal_type = if is_paused { SignalType::Pause } else { SignalType::Resume };
                    let status_str = if is_paused { "paused" } else { "running" };

                    let _ = self.signal_tx.send(signal_type);
                    tracing::info!(
                        event = "turn off/on",
                        status = status_str,
                    );
                }
                _ = sigusr2.recv() => {
                    let _ = self.signal_tx.send(SignalType::Reload);
                    tracing::info!(
                        event = "reload",
                        status = "reloaded",
                    );
                }
                _ = sigterm.recv() => {
                    self.handle_shutdown_signal("SIGTERM").await;
                    break;
                }
                _ = sigint.recv() => {
                    self.handle_shutdown_signal("SIGINT").await;
                    break;
                }
            }
        }

        tracing::info!(event = "signal_handler_stop", "Signal handler stopped");
        Ok(())
    }

    async fn handle_shutdown_signal(&self, signal_type: &str) {
        let _ = self.signal_tx.send(SignalType::Shutdown);
        tracing::info!(
            event = "signal_received",
            signal = signal_type,
            "Initiating graceful shutdown"
        );
    }
}
