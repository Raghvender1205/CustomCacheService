use std::sync::{Arc, Mutex};
use crate::datastore::DataStore;

pub struct Cache {
    store: Mutex<DataStore>,
}

impl Cache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            store: Mutex::new(DataStore::new()),
        })
    }

    pub async fn handle_command(&self, command: crate::commands::Command) -> Option<String> {
        let mut store = self.store.lock().unwrap();
        store.execute_command(command)
    }
}
