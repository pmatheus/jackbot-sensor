// Kafka store for caching order book and trade data
// Placeholder implementation until proper Kafka integration is needed

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use jackbot_instrument::exchange::ExchangeId;

/// Trait for Kafka store operations
pub trait KafkaStore {
    fn store_snapshot<T>(&self, exchange: ExchangeId, instrument: &str, data: &T)
    where
        T: Serialize;

    fn store_delta<T>(&self, exchange: ExchangeId, instrument: &str, data: &T)
    where
        T: Serialize;

    fn publish_snapshot<T>(&self, exchange: ExchangeId, instrument: &str, data: &T)
    where
        T: Serialize;

    fn publish_delta<T>(&self, exchange: ExchangeId, instrument: &str, data: &T)
    where
        T: Serialize;

    fn get_snapshot<T>(&self, key: &str) -> Result<Option<T>, Box<dyn std::error::Error>>
    where
        T: for<'de> Deserialize<'de>;

    fn get_deltas<T>(&self, key: &str) -> Result<Vec<T>, Box<dyn std::error::Error>>
    where
        T: for<'de> Deserialize<'de>;

    fn clear(&self);
}

/// Redis-like interface for Kafka operations
pub type KafkaClientStore = MockKafkaStore;

/// Mock Kafka store for testing and development
#[derive(Clone, Debug)]
pub struct MockKafkaStore {
    snapshots: Arc<Mutex<HashMap<String, String>>>,
    deltas: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl Default for MockKafkaStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MockKafkaStore {
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(Mutex::new(HashMap::new())),
            deltas: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl KafkaStore for MockKafkaStore {
    fn store_snapshot<T>(&self, exchange: ExchangeId, instrument: &str, data: &T)
    where
        T: Serialize,
    {
        let key = format!("{}:{}", exchange, instrument);
        if let Ok(serialized) = serde_json::to_string(data) {
            self.snapshots.lock().unwrap().insert(key, serialized);
        }
    }

    fn store_delta<T>(&self, exchange: ExchangeId, instrument: &str, data: &T)
    where
        T: Serialize,
    {
        let key = format!("{}:{}", exchange, instrument);
        if let Ok(serialized) = serde_json::to_string(data) {
            let mut deltas = self.deltas.lock().unwrap();
            deltas.entry(key).or_insert_with(Vec::new).push(serialized);
        }
    }

    fn publish_snapshot<T>(&self, exchange: ExchangeId, instrument: &str, data: &T)
    where
        T: Serialize,
    {
        // For mock implementation, just store it
        self.store_snapshot(exchange, instrument, data);
    }

    fn publish_delta<T>(&self, exchange: ExchangeId, instrument: &str, data: &T)
    where
        T: Serialize,
    {
        // For mock implementation, just store it
        self.store_delta(exchange, instrument, data);
    }

    fn get_snapshot<T>(&self, key: &str) -> Result<Option<T>, Box<dyn std::error::Error>>
    where
        T: for<'de> Deserialize<'de>,
    {
        if let Some(data) = self.snapshots.lock().unwrap().get(key) {
            let deserialized = serde_json::from_str(data)?;
            Ok(Some(deserialized))
        } else {
            Ok(None)
        }
    }

    fn get_deltas<T>(&self, key: &str) -> Result<Vec<T>, Box<dyn std::error::Error>>
    where
        T: for<'de> Deserialize<'de>,
    {
        if let Some(deltas) = self.deltas.lock().unwrap().get(key) {
            let mut result = Vec::new();
            for delta in deltas {
                result.push(serde_json::from_str(delta)?);
            }
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }

    fn clear(&self) {
        self.snapshots.lock().unwrap().clear();
        self.deltas.lock().unwrap().clear();
    }
}