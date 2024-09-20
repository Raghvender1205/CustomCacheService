use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use regex::Regex;
use crate::commands::Command;
use log::{error, info};

pub struct DataStore {
    data: HashMap<String, String>,
    expirations: HashMap<String, Instant>,
    lru_queue: VecDeque<String>,
    max_size: usize,
}

impl DataStore {
    pub fn new(max_size: usize) -> Self {
        Self {
            data: HashMap::new(),
            expirations: HashMap::new(),
            lru_queue: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    pub fn execute_command(&mut self, command: Command) -> Option<String> {
        match command {
            Command::Set { key, value } => {
                self.set(key, value)
            },
            Command::Get { key } => {
                self.get(&key)
            },
            Command::Delete { key } => {
                self.delete(&key)
            },
            Command::Expire { key, seconds } => {
                self.expire(key, seconds)
            },
            Command::Incr { key } => {
                self.incr(&key)
            },
            Command::Decr { key } => {
                self.decr(&key)
            },
            Command::Keys { pattern } => {
                self.keys(&pattern)
            },
        }
    }

    fn set(&mut self, key: String, value: String) -> Option<String> {
        self.data.insert(key.clone(), value.clone());
        self.update_lru(&key);
        info!("SET: key={}, value={}", key, value);
        Some("OK".to_string())
    }

    fn get(&mut self, key: &str) -> Option<String> {
        self.check_expiration(key);
        let result = self.data.get(key).cloned();
        if result.is_some() {
            self.update_lru(key);
        }
        info!("GET: key={}, result={:?}", key, result);
        result
    }

    fn delete(&mut self, key: &str) -> Option<String> {
        let result = if self.data.remove(key).is_some() {
            self.expirations.remove(key);
            self.remove_from_lru(key);
            info!("DELETE: key={}, result=Deleted", key);
            "Deleted".to_string()
        } else {
            info!("DELETE: key={}, result=Not Found", key);
            "Not Found".to_string()
        };
        Some(result)
    }

    fn expire(&mut self, key: String, seconds: u64) -> Option<String> {
        let result = if self.data.contains_key(&key) {
            let expiration = Instant::now() + Duration::from_secs(seconds);
            self.expirations.insert(key.clone(), expiration);
            self.update_lru(&key);
            info!("EXPIRE: key={}, seconds={}, result=OK", key, seconds);
            "OK".to_string()
        } else {
            info!("EXPIRE: key={}, seconds={}, result=Not Found", key, seconds);
            "Not Found".to_string()
        };
        Some(result)
    }

    fn incr(&mut self, key: &str) -> Option<String> {
        self.modify_by(key, 1)
    }

    fn decr(&mut self, key: &str) -> Option<String> {
        self.modify_by(key, -1)
    }

    fn modify_by(&mut self, key: &str, delta: i64) -> Option<String> {
        let result = self.data.get(key)
            .and_then(|value| value.parse::<i64>().ok())
            .map(|num| {
                let new_value = (num + delta).to_string();
                self.data.insert(key.to_string(), new_value.clone());
                self.update_lru(key);
                new_value
            });
        
        info!("{}: key={}, delta={}, result={:?}", 
              if delta > 0 { "INCR" } else { "DECR" }, 
              key, delta, result);
        result
    }

    fn keys(&self, pattern: &str) -> Option<String> {
        let pattern = pattern.replace("*", ".*");
        match Regex::new(&pattern) {
            Ok(regex) => {
                let keys: Vec<String> = self.data.keys()
                    .filter(|key| regex.is_match(key))
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

    fn check_expiration(&mut self, key: &str) {
        if let Some(&expires) = self.expirations.get(key) {
            if Instant::now() >= expires {
                self.data.remove(key);
                self.expirations.remove(key);
                self.remove_from_lru(key);
                info!("Key expired and removed: {}", key);
            }
        }
    }

    fn update_lru(&mut self, key: &str) {
        self.remove_from_lru(key);
        self.lru_queue.push_front(key.to_string());

        if self.lru_queue.len() > self.max_size {
            if let Some(oldest) = self.lru_queue.pop_back() {
                self.data.remove(&oldest);
                self.expirations.remove(&oldest);
                info!("LRU eviction: removed key {}", oldest);
            }
        }
    }

    fn remove_from_lru(&mut self, key: &str) {
        if let Some(index) = self.lru_queue.iter().position(|x| x == key) {
            self.lru_queue.remove(index);
        }
    }

    pub fn remove_expired_keys(&mut self) {
        let now = Instant::now();
        let expired_keys: Vec<String> = self.expirations.iter()
            .filter(|(_, &expire_time)| now >= expire_time)
            .map(|(key, _)| key.clone())
            .collect();

        for key in expired_keys {
            self.data.remove(&key);
            self.expirations.remove(&key);
            self.remove_from_lru(&key);
            info!("Expired key removed during cleanup: {}", key);
        }
    }
}
