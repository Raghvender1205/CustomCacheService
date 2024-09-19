use std::sync::Arc;
use serde::{Serialize, Deserialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use super::cache::Cache;

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

pub async fn handle_connection(mut socket: TcpStream, cache: Arc<Cache>) {
    let mut buffer = vec![0; 1024];

    loop {
        match socket.read(&mut buffer).await {
            Ok(0) => return, // Connection closed
            Ok(n) => {
                let command: serde_json::Result<Command> = serde_json::from_slice(&buffer[..n]);
                if let Ok(command) = command {
                    if let Some(response) = cache.handle_command(command).await {
                        if let Err(e) = socket.write_all(response.as_bytes()).await {
                            println!("Failed to write to socket: {}", e);
                            return;
                        }
                    }
                } else {
                    let msg = "Invalid command";
                    let _ = socket.write_all(msg.as_bytes()).await;
                }
            }
            Err(e) => {
                println!("Failed to read from socket: {}", e);
                return;
            }
        }
    }
}
