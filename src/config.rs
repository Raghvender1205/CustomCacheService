use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Config {
    /// Bind address, e.g. 0.0.0.0:6379
    #[arg(long, default_value = "0.0.0.0:6379")]
    pub bind: String,

    /// Max items per shard (total capacity ~= max_items * shards)
    #[arg(long, default_value_t = 100_000)]
    pub max_items: usize,

    /// Eviction policy: lru | sieve
    #[arg(long, default_value = "lru", value_parser = ["lru", "sieve"])]
    pub eviction: String,

    /// Number of shards (ideally ~= CPU cores)
    #[arg(long, default_value_t = 8)]
    pub shards: usize,

    /// Periodic cleanup interval (seconds)
    #[arg(long, default_value_t = 30)]
    pub cleanup_secs: u64,
}

impl Config {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}
