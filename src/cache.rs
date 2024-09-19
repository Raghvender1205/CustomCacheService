use std::sync::{Arc, Mutex};
use std::time::Instant;
use log::info;
use crate::datastore::DataStore;

pub struct Cache {
    store: Mutex<DataStore>,
    metrics: Mutex<Metrics>,
}

struct Metrics {
    total_commands: u64,
    start_time: Instant,
}

impl Cache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            store: Mutex::new(DataStore::new()),
            metrics: Mutex::new(Metrics {
                total_commands: 0,
                start_time: Instant::now(),
            }),
        })
    }

    pub async fn handle_command(&self, command: crate::commands::Command) -> Option<String> {
        let mut store = self.store.lock().unwrap();
        let result = store.execute_command(command);

        // Update metrics
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_commands += 1;

        // Log metrics every 1000 commands
        if metrics.total_commands % 1000 == 0 {
            let uptime = metrics.start_time.elapsed().as_secs();
            info!(
                "Metrics: Total commands: {}, Uptime: {} seconds, Commands per second: {}",
                metrics.total_commands,
                uptime,
                metrics.total_commands as f64 / uptime as f64
            );
        }

        result
    }
}
