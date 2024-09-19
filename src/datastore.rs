use std::collections::HashMap;
use std::time::{Duration, Instant};
use regex::Regex;
use crate::commands::Command;
use log::{error, info};

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
                self.data.insert(key.clone(), value.clone());
                info!("SET: key={}, value={}", key, value);
                Some("OK".to_string())
            },
            Command::Get { key } => {
                let result = self.data.get(&key).cloned();
                info!("GET: key={}, result={:?}", key, result);
                result
            },
            Command::Delete { key } => {
                let result = if self.data.remove(&key).is_some() {
                    info!("DELETE: key={}, result=Deleted", key);
                    "Deleted".to_string()
                } else {
                    info!("DELETE: key={}, result=Not Found", key);
                    "Not Found".to_string()
                };
                Some(result)
            },
            Command::Expire { key, seconds} => {
                let result = if self.data.contains_key(&key) {
                    let expiration = Instant::now() + Duration::from_secs(seconds);
                    self.expirations.insert(key.clone(), expiration);
                    info!("EXPIRE: key={}, seconds={}, result=OK", key, seconds);
                    "OK".to_string()
                } else {
                    info!("EXPIRE: key={}, seconds={}, result=Not Found", key, seconds);
                    "Not Found".to_string()
                };
                Some(result)
            },
            Command::Incr { key } => {
                let result = self.modify_by(&key, 1);
                info!("INCR: key={}, result={:?}", key, result);
                result
            },
            Command::Decr { key } => {
                let result = self.modify_by(&key, -1);
                info!("DECR: key={}, result={:?}", key, result);
                result
            },
            Command::Keys { pattern } => {
                let pattern = pattern.replace("*", ".*");
                match Regex::new(&pattern) {
                    Ok(regex) => {
                        let keys: Vec<String> = self.data.keys()
                            .filter(|key| regex.is_match(key) )
                            .cloned()
                            .collect();
                        info!("KEYS: pattern={}, result={:?}", pattern, keys);
                        Some(format!("{:?}", keys))
                    },
                    Err(e) => {
                        error!("Invalid regex pattern: {}", e);
                        Some("Invalid pattern".to_string())
                    }
                }
            }
        }
    }

    fn check_expiration(&mut self, key: &String) {
        if let Some(&expires) = self.expirations.get(key) {
            if Instant::now() >= expires {
                self.data.remove(key);
                self.expirations.remove(key);
                info!("Key expired and removed: {}", key);
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
