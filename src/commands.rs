use std::sync::Arc;
use serde::{Serialize, Deserialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use super::cache::Cache;
use log::{info, warn, error};

#[derive(Serialize, Deserialize, Debug)]
pub enum Command {
    Set     { key: String, value: String },
    Get     { key: String },
    Delete  { key: String },
    Expire  { key: String, seconds: u64 },
    Incr    { key: String },
    Decr    { key: String },
    Keys    { pattern: String },
}

pub async fn handle_connection(mut socket: TcpStream, cache: Arc<Cache>) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0; 1024];

    loop {
        match socket.read(&mut buffer).await {
            Ok(0) => {
                info!("Connection closed");
                return Ok(());
            }, // Connection closed
            Ok(n) => {
                let command: serde_json::Result<Command> = serde_json::from_slice(&buffer[..n]);
                if let Ok(command) = command {
                    info!("Received command: {:?}", command);
                    if let Some(response) = cache.handle_command(command).await {
                        if let Err(e) = socket.write_all(response.as_bytes()).await {
                            error!("Failed to write to socket: {}", e);
                            return Err(e.into());
                        }
                    }
                } else {
                    warn!("Invalid command received");
                    let msg = "Invalid command";
                    let _ = socket.write_all(msg.as_bytes()).await;
                }
            }
            Err(e) => {
                error!("Failed to read from socket: {}", e);
                return Err(e.into());
            }
        }
    }
}
