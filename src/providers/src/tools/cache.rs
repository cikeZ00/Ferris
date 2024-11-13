use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant};

#[derive(Clone)]
pub struct CacheEntry {
    pub value: Value,
    pub expiry: Instant,
}

#[derive(Clone)]
pub struct Cache {
    data: Arc<Mutex<HashMap<String, CacheEntry>>>,
    ttl: Duration,
}

impl Cache {
    pub fn new(ttl: Duration) -> Self {
        Cache {
            data: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        let data = self.data.lock().unwrap();
        if let Some(entry) = data.get(key) {
            if entry.expiry > Instant::now() {
                return Some(entry.value.clone());
            }
        }
        None
    }

    pub fn set(&self, key: String, value: Value) {
        let mut data = self.data.lock().unwrap();
        let entry = CacheEntry {
            value,
            expiry: Instant::now() + self.ttl,
        };
        data.insert(key, entry);
    }
}
