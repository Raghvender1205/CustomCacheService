use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use serde_json::Result as JsonResult;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Serialize, Deserialize, Debug)]
enum Command {
    Set { key: String, value: String },
    Get { key: String },
    Delete { key: String }
}

struct Cache {
    store: Mutex<HashMap<String, String>>,
}

impl Cache {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            store: Mutex::new(HashMap::new()),
        })
    }

    async fn handle_command(&self, command: Command) -> Option<String> {
        let mut store = self.store.lock().unwrap();
        match command {
            Command::Set { key, value } => {
                store.insert(key, value);
                Some("OK".to_string())
            }
            Command::Get { key } => {
                store.get(&key).cloned()
            }
            Command::Delete { key } => {
                if store.remove(&key).is_some() {
                    Some("Deleted".to_string())
                } else {
                    Some("Not Found".to_string())
                }
            }
        }
    }
}

async fn handle_connection(mut socket: TcpStream, cache: Arc<Cache>) {
    let mut buffer = vec![0; 1024];

    loop {
        match socket.read(&mut buffer).await {
            Ok(0) => return, // Connection closed
            Ok(n) => {
                let command: JsonResult<Command> = serde_json::from_slice(&buffer[..n]);
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    let cache = Cache::new();

    loop {
        let (socket, _) = listener.accept().await?;
        let cache = cache.clone();
        tokio::spawn(async move {
            handle_connection(socket, cache).await;
        });
    }
}