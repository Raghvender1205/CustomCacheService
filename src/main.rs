mod cache;
mod datastore;
mod commands;

use log::{info, warn, error};
use env_logger::Env;
use tokio::net::TcpListener;
use cache::Cache;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    info!("Starting Cache Server");
    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    info!("Listening on port 6379");
    let cache = Cache::new();

    loop {
        match listener.accept().await {
            Ok((socket, addr)) => {
                info!("New connection from: {}", addr);
                let cache_clone = cache.clone();
                tokio::spawn(async move {
                    if let Err(e) = commands::handle_connection(socket, cache_clone).await {
                        error!("Error handling connection: {}", e);
                    }
                });
            }
            Err(e) => {
                warn!("Failed to accept connection: {}", e);
            }
        }
    }
}
