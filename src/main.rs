mod cache;
mod datastore;
mod commands;

use std::time::Duration;
use log::{info, warn, error};
use env_logger::Env;
use tokio::net::TcpListener;
use tokio::time::interval;
use cache::Cache;

const MAX_CACHE_SIZE: usize = 1000;
const CLEANUP_INTERVAL_SECS: u64 = 60;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    info!("Starting Cache Server");
    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    info!("Listening on port 6379");
    let cache = Cache::new(MAX_CACHE_SIZE);

    // Spawn a task for periodic cleanup of expired keys
    let cleanup_cache = cache.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
        loop {
            interval.tick().await;
            cleanup_cache.cleanup_expired_keys().await;
        }
    });

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
