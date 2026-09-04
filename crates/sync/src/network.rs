//! Local network sync between RavenBot instances

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

/// Sync protocol version
#[allow(dead_code)]
const PROTOCOL_VERSION: u32 = 1;

/// Sync message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncMessage {
    /// Discovery broadcast
    Discovery {
        instance_id: Uuid,
        name: String,
    },
    /// Pairing request
    PairRequest {
        instance_id: Uuid,
        pairing_code: String,
    },
    /// Pairing response
    PairResponse {
        instance_id: Uuid,
        accepted: bool,
    },
    /// Sync request
    SyncRequest {
        instance_id: Uuid,
        last_sync: Option<String>,
    },
    /// Sync data
    SyncData {
        instance_id: Uuid,
        bundles: Vec<serde_json::Value>,
    },
    /// Sync complete
    SyncComplete {
        instance_id: Uuid,
    },
}

/// Local sync server/client
pub struct LocalSync {
    /// This instance's ID
    instance_id: Uuid,
    /// Instance name
    name: String,
    /// Paired instances
    #[allow(dead_code)]
    paired_instances: Vec<PairedInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairedInstance {
    id: Uuid,
    name: String,
    address: SocketAddr,
    public_key: Vec<u8>,
}

impl LocalSync {
    pub fn new(name: String) -> Self {
        Self {
            instance_id: Uuid::new_v4(),
            name,
            paired_instances: Vec::new(),
        }
    }

    /// Start the sync server
    pub async fn start_server(&self, port: u16) -> Result<(), String> {
        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("Failed to bind: {}", e))?;

        tracing::info!(port = port, "Sync server started");

        loop {
            match listener.accept().await {
                Ok((socket, addr)) => {
                    tracing::debug!(addr = %addr, "New connection");
                    tokio::spawn(Self::handle_connection(socket));
                }
                Err(e) => {
                    tracing::error!(error = %e, "Accept error");
                }
            }
        }
    }

    /// Handle a sync connection
    async fn handle_connection(mut socket: TcpStream) {
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB max message
        
        match socket.read(&mut buffer).await {
            Ok(n) => {
                if n > 0 {
                    let message: Result<SyncMessage, _> = serde_json::from_slice(&buffer[..n]);
                    
                    match message {
                        Ok(msg) => {
                            tracing::debug!(message = ?msg, "Received sync message");
                            // Handle message and send response
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Invalid sync message");
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Read error");
            }
        }
    }

    /// Connect to another instance
    pub async fn connect(&self, address: &str) -> Result<(), String> {
        let mut socket = TcpStream::connect(address)
            .await
            .map_err(|e| format!("Failed to connect: {}", e))?;

        // Send discovery message
        let message = SyncMessage::Discovery {
            instance_id: self.instance_id,
            name: self.name.clone(),
        };

        let json = serde_json::to_vec(&message)
            .map_err(|e| format!("Serialization error: {}", e))?;

        socket.write_all(&json)
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        tracing::info!(address = address, "Connected to remote instance");
        Ok(())
    }

    /// Generate pairing code
    pub fn generate_pairing_code() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let code: u32 = rng.gen_range(100000..999999);
        code.to_string()
    }

    /// Pair with another instance
    pub async fn pair(&mut self, address: &str, pairing_code: &str) -> Result<(), String> {
        let mut socket = TcpStream::connect(address)
            .await
            .map_err(|e| format!("Failed to connect: {}", e))?;

        // Send pair request
        let message = SyncMessage::PairRequest {
            instance_id: self.instance_id,
            pairing_code: pairing_code.to_string(),
        };

        let json = serde_json::to_vec(&message)
            .map_err(|e| format!("Serialization error: {}", e))?;

        socket.write_all(&json)
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        // Read response
        let mut buffer = vec![0u8; 1024];
        let n = socket.read(&mut buffer)
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        let response: SyncMessage = serde_json::from_slice(&buffer[..n])
            .map_err(|e| format!("Invalid response: {}", e))?;

        match response {
            SyncMessage::PairResponse { accepted: true, .. } => {
                tracing::info!(address = address, "Pairing accepted");
                Ok(())
            }
            SyncMessage::PairResponse { accepted: false, .. } => {
                Err("Pairing rejected".to_string())
            }
            _ => {
                Err("Invalid response".to_string())
            }
        }
    }

    /// Sync with a paired instance
    pub async fn sync_with(&self, address: &str) -> Result<usize, String> {
        let mut socket = TcpStream::connect(address)
            .await
            .map_err(|e| format!("Failed to connect: {}", e))?;

        // Send sync request
        let message = SyncMessage::SyncRequest {
            instance_id: self.instance_id,
            last_sync: None, // In production, track last sync time
        };

        let json = serde_json::to_vec(&message)
            .map_err(|e| format!("Serialization error: {}", e))?;

        socket.write_all(&json)
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        // Read sync data
        let mut buffer = vec![0u8; 10 * 1024 * 1024]; // 10MB
        let n = socket.read(&mut buffer)
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        let response: SyncMessage = serde_json::from_slice(&buffer[..n])
            .map_err(|e| format!("Invalid response: {}", e))?;

        match response {
            SyncMessage::SyncData { bundles, .. } => {
                let count = bundles.len();
                tracing::info!(count = count, "Received sync data");
                // In production, import the bundles
                Ok(count)
            }
            _ => {
                Err("Invalid sync response".to_string())
            }
        }
    }

    /// Get instance ID
    pub fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    /// Get instance name
    pub fn name(&self) -> &str {
        &self.name
    }
}
