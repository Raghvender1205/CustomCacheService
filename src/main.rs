mod cache;
mod datastore;
mod commands;

use tokio::net::TcpListener;
use cache::Cache;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    let cache = Cache::new();

    loop {
        let (socket, _) = listener.accept().await?;
        let cache_clone = cache.clone();
        tokio::spawn(async move {
            commands::handle_connection(socket, cache_clone).await;
        });
    }
}
