use eyre::Result;
use reth_tracing::tracing::{error, info};
use tokio::net::UnixDatagram;

pub struct SocketHandler {
    sock: UnixDatagram,
    path: String,
}

impl SocketHandler {
    pub fn new(socket_path: String) -> Result<Self> {
        let sock = UnixDatagram::unbound()?;
        sock.connect(&socket_path)?;
        
        info!("Socket connected at {}", socket_path);
        
        Ok(Self {
            sock,
            path: socket_path,
        })
    }

    pub async fn receive_data(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; 4096];
        match self.sock.recv(&mut buf).await {
            Ok(n) if n > 0 => Ok(buf[..n].to_vec()),
            Ok(_) => Err(eyre::eyre!("Received empty message")),
            Err(e) => Err(e.into()),
        }
    }

    pub fn cleanup(&self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                error!("Failed to remove socket file: {}", e);
            }
        }
    }
}