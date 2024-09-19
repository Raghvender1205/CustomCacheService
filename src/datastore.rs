use std::collections::HashMap;
use std::time::{Duration, Instant};
use regex::Regex;
use crate::commands::Command;

pub struct DataStore {
    data: HashMap<String, String>,
    expirations: HashMap<String, Instant>
}

impl DataStore {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            expirations: HashMap::new(),
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
            },
            Command::Expire { key, seconds} => {
                if self.data.contains_key(&key) {
                    let expiration = Instant::now() + Duration::from_secs(seconds);
                    self.expirations.insert(key, expiration);
                    Some("OK".to_string())
                } else {
                    Some("Not Found".to_string())
                }
            },
            Command::Incr { key } => {
                self.modify_by(&key, 1)
            },
            Command::Decr { key } => {
                self.modify_by(&key, -1)
            },
            Command::Keys { pattern } => {
                let pattern = pattern.replace("*", ".*");
                let regex = Regex::new(&pattern).unwrap();
                let keys: Vec<String> = self.data.keys()
                    .filter(|key| regex.is_match(key) )
                    .cloned()
                    .collect();
                Some(format!("{:?}", keys))
            }
        }
    }

    fn check_expiration(&mut self, key: &String) {
        if let Some(&expires) = self.expirations.get(key) {
            if Instant::now() >= expires {
                self.data.remove(key);
                self.expirations.remove(key);
            }
        }
    }

    fn modify_by(&mut self, key: &String, amount: i64) -> Option<String> {
        self.check_expiration(key);
        let value = self.data.entry(key.clone()).or_insert("0".to_string());
        let current_val: i64 = value.parse().unwrap_or(0);
        *value = (current_val + amount).to_string();
        Some(value.clone())
    }
}
