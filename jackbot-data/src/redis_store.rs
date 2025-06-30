use redis::{aio::ConnectionManager, AsyncCommands};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, debug, instrument};
use crate::{error::DataError, books::OrderBook, subscription::book::OrderBookEvent};
use jackbot_instrument::exchange::ExchangeId;

/// Trait for Redis storage operations
pub trait RedisStore: Send + Sync {
    /// Store an order book snapshot
    fn store_snapshot(&self, exchange: ExchangeId, symbol: &str, snapshot: &OrderBook);
    
    /// Store an order book delta/update
    fn store_delta(&self, exchange: ExchangeId, symbol: &str, delta: &OrderBookEvent);
    
    /// Publish an order book snapshot to Redis pub/sub
    fn publish_snapshot(&self, exchange: ExchangeId, symbol: &str, snapshot: &OrderBook);
    
    /// Publish an order book delta to Redis pub/sub
    fn publish_delta(&self, exchange: ExchangeId, symbol: &str, delta: &OrderBookEvent);
}

/// Default Redis store implementation for caching order book and trade data
#[derive(Clone)]
pub struct DefaultRedisStore {
    conn: Arc<ConnectionManager>,
    prefix: String,
}

impl DefaultRedisStore {
    /// Create a new Redis store instance
    pub async fn new(redis_url: &str, prefix: &str) -> Result<Self, DataError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| DataError::Connection(format!("Failed to create Redis client: {}", e)))?;
        
        let conn = ConnectionManager::new(client).await
            .map_err(|e| DataError::Connection(format!("Failed to connect to Redis: {}", e)))?;
        
        Ok(Self {
            conn: Arc::new(conn),
            prefix: prefix.to_string(),
        })
    }

    /// Store serialized data with an optional TTL (in seconds)
    #[instrument(skip(self, value))]
    pub async fn set<T: Serialize>(&self, key: &str, value: &T, ttl: Option<u64>) -> Result<(), DataError> {
        let key = format!("{}:{}", self.prefix, key);
        let serialized = serde_json::to_string(value)
            .map_err(|e| DataError::Serde(format!("Failed to serialize value: {}", e)))?;
        
        let mut conn = self.conn.as_ref().clone();
        
        if let Some(ttl_seconds) = ttl {
            conn.set_ex(&key, serialized, ttl_seconds).await
                .map_err(|e| DataError::Connection(format!("Failed to set value with TTL: {}", e)))?;
        } else {
            conn.set(&key, serialized).await
                .map_err(|e| DataError::Connection(format!("Failed to set value: {}", e)))?;
        }
        
        debug!("Stored value in Redis with key: {}", key);
        Ok(())
    }

    /// Retrieve and deserialize data
    #[instrument(skip(self))]
    pub async fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>, DataError> {
        let key = format!("{}:{}", self.prefix, key);
        let mut conn = self.conn.as_ref().clone();
        
        let value: Option<String> = conn.get(&key).await
            .map_err(|e| DataError::Connection(format!("Failed to get value: {}", e)))?;
        
        match value {
            Some(serialized) => {
                let deserialized = serde_json::from_str(&serialized)
                    .map_err(|e| DataError::Serde(format!("Failed to deserialize value: {}", e)))?;
                Ok(Some(deserialized))
            }
            None => Ok(None),
        }
    }

    /// Delete a key
    #[instrument(skip(self))]
    pub async fn delete(&self, key: &str) -> Result<(), DataError> {
        let key = format!("{}:{}", self.prefix, key);
        let mut conn = self.conn.as_ref().clone();
        
        conn.del(&key).await
            .map_err(|e| DataError::Connection(format!("Failed to delete key: {}", e)))?;
        
        debug!("Deleted key from Redis: {}", key);
        Ok(())
    }

    /// Check if a key exists
    #[instrument(skip(self))]
    pub async fn exists(&self, key: &str) -> Result<bool, DataError> {
        let key = format!("{}:{}", self.prefix, key);
        let mut conn = self.conn.as_ref().clone();
        
        let exists: bool = conn.exists(&key).await
            .map_err(|e| DataError::Connection(format!("Failed to check key existence: {}", e)))?;
        
        Ok(exists)
    }

    /// Store order book snapshot with automatic expiry
    pub async fn store_orderbook_snapshot(&self, exchange: &str, symbol: &str, snapshot: &str) -> Result<(), DataError> {
        let key = format!("orderbook:{}:{}", exchange, symbol);
        let mut conn = self.conn.as_ref().clone();
        
        // Store with 5 minute TTL for orderbook snapshots
        conn.set_ex(&format!("{}:{}", self.prefix, key), snapshot, 300).await
            .map_err(|e| DataError::Connection(format!("Failed to store orderbook snapshot: {}", e)))?;
        
        Ok(())
    }

    /// Store trade data with automatic expiry
    pub async fn store_trade(&self, exchange: &str, symbol: &str, trade_id: &str, trade_data: &str) -> Result<(), DataError> {
        let key = format!("trade:{}:{}:{}", exchange, symbol, trade_id);
        let mut conn = self.conn.as_ref().clone();
        
        // Store with 1 hour TTL for trades
        conn.set_ex(&format!("{}:{}", self.prefix, key), trade_data, 3600).await
            .map_err(|e| DataError::Connection(format!("Failed to store trade: {}", e)))?;
        
        Ok(())
    }

    /// Get all keys matching a pattern
    pub async fn scan_keys(&self, pattern: &str) -> Result<Vec<String>, DataError> {
        let pattern = format!("{}:{}", self.prefix, pattern);
        let mut conn = self.conn.as_ref().clone();
        
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| DataError::Connection(format!("Failed to scan keys: {}", e)))?;
        
        Ok(keys)
    }
}

impl RedisStore for DefaultRedisStore {
    fn store_snapshot(&self, exchange: ExchangeId, symbol: &str, snapshot: &OrderBook) {
        let key = format!("orderbook:{}:{}", exchange, symbol);
        let conn = self.conn.clone();
        let prefix = self.prefix.clone();
        let snapshot = snapshot.clone();
        
        tokio::spawn(async move {
            let mut conn = conn.as_ref().clone();
            let serialized = match serde_json::to_string(&snapshot) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to serialize orderbook snapshot: {}", e);
                    return;
                }
            };
            
            // Store with 5 minute TTL for orderbook snapshots
            if let Err(e) = conn.set_ex::<_, _, ()>(&format!("{}:{}", prefix, key), serialized, 300).await {
                error!("Failed to store orderbook snapshot: {}", e);
            }
        });
    }
    
    fn store_delta(&self, exchange: ExchangeId, symbol: &str, delta: &OrderBookEvent) {
        let key = format!("orderbook_delta:{}:{}", exchange, symbol);
        let conn = self.conn.clone();
        let prefix = self.prefix.clone();
        let delta = delta.clone();
        
        tokio::spawn(async move {
            let mut conn = conn.as_ref().clone();
            let serialized = match serde_json::to_string(&delta) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to serialize orderbook delta: {}", e);
                    return;
                }
            };
            
            // Store with 1 minute TTL for deltas
            if let Err(e) = conn.set_ex::<_, _, ()>(&format!("{}:{}", prefix, key), serialized, 60).await {
                error!("Failed to store orderbook delta: {}", e);
            }
        });
    }
    
    fn publish_snapshot(&self, exchange: ExchangeId, symbol: &str, snapshot: &OrderBook) {
        let channel = format!("orderbook:snapshot:{}:{}", exchange, symbol);
        let conn = self.conn.clone();
        let snapshot = snapshot.clone();
        
        tokio::spawn(async move {
            let mut conn = conn.as_ref().clone();
            let serialized = match serde_json::to_string(&snapshot) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to serialize orderbook snapshot for pubsub: {}", e);
                    return;
                }
            };
            
            // Publish to Redis pub/sub channel
            if let Err(e) = conn.publish::<_, _, ()>(channel, serialized).await {
                error!("Failed to publish orderbook snapshot: {}", e);
            }
        });
    }
    
    fn publish_delta(&self, exchange: ExchangeId, symbol: &str, delta: &OrderBookEvent) {
        let channel = format!("orderbook:delta:{}:{}", exchange, symbol);
        let conn = self.conn.clone();
        let delta = delta.clone();
        
        tokio::spawn(async move {
            let mut conn = conn.as_ref().clone();
            let serialized = match serde_json::to_string(&delta) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to serialize orderbook delta for pubsub: {}", e);
                    return;
                }
            };
            
            // Publish to Redis pub/sub channel
            if let Err(e) = conn.publish::<_, _, ()>(channel, serialized).await {
                error!("Failed to publish orderbook delta: {}", e);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Redis to be running
    async fn test_redis_store_basic_operations() {
        let store = DefaultRedisStore::new("redis://localhost:6379", "test")
            .await
            .expect("Failed to create Redis store");

        // Test set and get
        let key = "test_key";
        let value = "test_value";
        
        store.set(key, &value, None).await.expect("Failed to set value");
        let retrieved: Option<String> = store.get(key).await.expect("Failed to get value");
        
        assert_eq!(retrieved, Some(value.to_string()));

        // Test exists
        let exists = store.exists(key).await.expect("Failed to check existence");
        assert!(exists);

        // Test delete
        store.delete(key).await.expect("Failed to delete key");
        let exists_after_delete = store.exists(key).await.expect("Failed to check existence");
        assert!(!exists_after_delete);
    }
}