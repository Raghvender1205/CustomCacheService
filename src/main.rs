mod config;
mod engine;
mod policy;
mod resp;

use crate::config::Config;
use crate::engine::Engine;
use tokio::net::TcpListener;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("starting cache server: bind={} shards={} eviction={}", cfg.bind, cfg.shards, cfg.eviction);

    let engine = Engine::new(cfg.clone()).await;

    let listener = TcpListener::bind(&cfg.bind).await?;
    info!("listening on {}", cfg.bind);

    loop {
        let (socket, addr) = listener.accept().await?;
        info!("accepted connection from {}", addr);

        let engine = engine.clone();
        tokio::spawn(async move {
            if let Err(e) = resp::handle_connection(socket, engine).await {
                error!("connection error from {}: {}", addr, e);
            }
        });
    }
}
