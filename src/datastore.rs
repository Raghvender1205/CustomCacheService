use std::collections::HashMap;
use crate::commands::Command;

pub struct DataStore {
    data: HashMap<String, String>,
}

impl DataStore {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn execute_command(&mut self, command: Command) -> Option<String> {
        match command {
            Command::Set { key, value } => {
                self.data.insert(key, value);
                Some("OK".to_string())
            },
            Command::Get { key } => {
                self.data.get(&key).cloned()
            },
            Command::Delete { key } => {
                if self.data.remove(&key).is_some() {
                    Some("Deleted".to_string())
                } else {
                    Some("Not Found".to_string())
                }
            }
        }
    }
}
