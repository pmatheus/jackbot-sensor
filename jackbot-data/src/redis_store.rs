use crate::{
    books::OrderBook,
    subscription::{book::OrderBookEvent, trade::PublicTrade},
};
use jackbot_instrument::{exchange::ExchangeId, Side};

use fnv::FnvHashMap;
use std::sync::{Arc, Mutex};
use chrono::Utc;
use serde_json;
use rust_decimal::prelude::ToPrimitive;

/// Default Redis key prefix used across exchanges.
pub const DEFAULT_PREFIX: &str = "jb";

/// Maximum number of deltas or trades retained per instrument.
pub const MAX_LIST_LEN: usize = 1000;

/// Build the snapshot key for a given exchange and instrument.
pub fn snapshot_key(prefix: &str, exchange: ExchangeId, instrument: &str) -> String {
    format!("{}:{}:{}:snapshot", prefix, exchange, instrument)
}

/// Build the delta list key for a given exchange and instrument.
pub fn delta_key(prefix: &str, exchange: ExchangeId, instrument: &str) -> String {
    format!("{}:{}:{}:deltas", prefix, exchange, instrument)
}

/// Build the trades list key for a given exchange and instrument.
pub fn trade_key(prefix: &str, exchange: ExchangeId, instrument: &str) -> String {
    format!("{}:{}:{}:trades", prefix, exchange, instrument)
}

/// Storage interface for persisting snapshots, deltas and trades.
pub trait RedisStore: Send + Sync {
    fn store_snapshot(&self, exchange: ExchangeId, instrument: &str, snapshot: &OrderBook);
    fn store_delta(&self, exchange: ExchangeId, instrument: &str, delta: &OrderBookEvent);
    fn store_trade(&self, exchange: ExchangeId, instrument: &str, trade: &PublicTrade);

    /// Retrieve the latest snapshot for the given exchange and instrument.
    fn get_snapshot(&self, exchange: ExchangeId, instrument: &str) -> Option<OrderBook>;

    /// Retrieve up to `limit` most recent order book deltas.
    fn get_deltas(
        &self,
        exchange: ExchangeId,
        instrument: &str,
        limit: usize,
    ) -> Vec<OrderBookEvent>;

    /// Retrieve up to `limit` most recent trades.
    fn get_trades(&self, exchange: ExchangeId, instrument: &str, limit: usize) -> Vec<PublicTrade>;

    /// Publish snapshot to Redis pub/sub channel for real-time streaming.
    fn publish_snapshot(&self, exchange: ExchangeId, instrument: &str, snapshot: &OrderBook);

    /// Publish delta to Redis pub/sub channel for real-time streaming.
    fn publish_delta(&self, exchange: ExchangeId, instrument: &str, delta: &OrderBookEvent);

    /// Publish trade to Redis pub/sub channel for real-time streaming.
    fn publish_trade(&self, exchange: ExchangeId, instrument: &str, trade: &PublicTrade);
}

/// In-memory RedisStore used for testing.
#[derive(Clone, Default, Debug)]
pub struct InMemoryStore {
    snapshots: Arc<Mutex<FnvHashMap<String, String>>>,
    deltas: Arc<Mutex<FnvHashMap<String, Vec<String>>>>,
    trades: Arc<Mutex<FnvHashMap<String, Vec<String>>>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot_key(prefix: &str, exchange: ExchangeId, instrument: &str) -> String {
        format!("{}:{}:{}:snapshot", prefix, exchange, instrument)
    }
    pub fn delta_key(prefix: &str, exchange: ExchangeId, instrument: &str) -> String {
        format!("{}:{}:{}:deltas", prefix, exchange, instrument)
    }
    pub fn trade_key(prefix: &str, exchange: ExchangeId, instrument: &str) -> String {
        format!("{}:{}:{}:trades", prefix, exchange, instrument)
    }

    /// Helper used in tests.
    pub fn get_snapshot_json(&self, exchange: ExchangeId, instrument: &str) -> Option<String> {
        let key = snapshot_key("jb", exchange, instrument);
        self.snapshots.lock().unwrap().get(&key).cloned()
    }

    /// Helper used in tests.
    pub fn delta_len(&self, exchange: ExchangeId, instrument: &str) -> usize {
        let key = delta_key("jb", exchange, instrument);
        self.deltas
            .lock()
            .unwrap()
            .get(&key)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

impl RedisStore for InMemoryStore {
    fn store_snapshot(&self, exchange: ExchangeId, instrument: &str, snapshot: &OrderBook) {
        let json = serde_json::to_string(snapshot).expect("serialise snapshot");
        let key = snapshot_key("jb", exchange, instrument);
        self.snapshots.lock().unwrap().insert(key, json);
    }

    fn store_delta(&self, exchange: ExchangeId, instrument: &str, delta: &OrderBookEvent) {
        let json = serde_json::to_string(delta).expect("serialise delta");
        let key = Self::delta_key("jb", exchange, instrument);
        let mut guard = self.deltas.lock().unwrap();
        let list = guard.entry(key).or_default();
        list.push(json);
        if list.len() > MAX_LIST_LEN {
            let excess = list.len() - MAX_LIST_LEN;
            list.drain(0..excess);
        }
    }

    fn store_trade(&self, exchange: ExchangeId, instrument: &str, trade: &PublicTrade) {
        let json = serde_json::to_string(trade).expect("serialise trade");
        let key = trade_key("jb", exchange, instrument);
        let mut guard = self.trades.lock().unwrap();
        let list = guard.entry(key).or_default();
        list.push(json);
        if list.len() > MAX_LIST_LEN {
            let excess = list.len() - MAX_LIST_LEN;
            list.drain(0..excess);
        }
    }

    fn get_snapshot(&self, exchange: ExchangeId, instrument: &str) -> Option<OrderBook> {
        let key = snapshot_key("jb", exchange, instrument);
        self.snapshots
            .lock()
            .unwrap()
            .get(&key)
            .and_then(|s| serde_json::from_str(s).ok())
    }

    fn get_deltas(
        &self,
        exchange: ExchangeId,
        instrument: &str,
        limit: usize,
    ) -> Vec<OrderBookEvent> {
        let key = delta_key("jb", exchange, instrument);
        self.deltas
            .lock()
            .unwrap()
            .get(&key)
            .map(|v| {
                v.iter()
                    .rev()
                    .take(limit)
                    .filter_map(|s| serde_json::from_str(s).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_trades(&self, exchange: ExchangeId, instrument: &str, limit: usize) -> Vec<PublicTrade> {
        let key = trade_key("jb", exchange, instrument);
        self.trades
            .lock()
            .unwrap()
            .get(&key)
            .map(|v| {
                v.iter()
                    .rev()
                    .take(limit)
                    .filter_map(|s| serde_json::from_str(s).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn publish_snapshot(&self, _exchange: ExchangeId, _instrument: &str, _snapshot: &OrderBook) {
        // No-op for in-memory testing store
    }

    fn publish_delta(&self, _exchange: ExchangeId, _instrument: &str, _delta: &OrderBookEvent) {
        // No-op for in-memory testing store
    }

    fn publish_trade(&self, _exchange: ExchangeId, _instrument: &str, _trade: &PublicTrade) {
        // No-op for in-memory testing store
    }
}

/// Redis backed store used in production.
#[derive(Clone)]
pub struct RedisClientStore {
    client: redis::Client,
    prefix: String,
}

impl std::fmt::Debug for RedisClientStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisClientStore")
            .field("client", &"<redis_client>")
            .field("prefix", &self.prefix)
            .finish()
    }
}

impl RedisClientStore {
    pub fn new(url: &str, prefix: impl Into<String>) -> redis::RedisResult<Self> {
        Ok(Self {
            client: redis::Client::open(url)?,
            prefix: prefix.into(),
        })
    }

    fn key(&self, suffix: &str, exchange: ExchangeId, instrument: &str) -> String {
        format!("{}:{}:{}:{}", self.prefix, exchange, instrument, suffix)
    }
}

impl RedisStore for RedisClientStore {
    fn store_snapshot(&self, exchange: ExchangeId, instrument: &str, snapshot: &OrderBook) {
        let key = self.key("snapshot", exchange, instrument);
        if let Ok(json) = serde_json::to_string(snapshot) {
            if let Ok(mut conn) = self.client.get_connection() {
                let _: redis::RedisResult<()> =
                    redis::pipe().atomic().set(key, json).query(&mut conn);
            }
        }
    }

    fn store_delta(&self, exchange: ExchangeId, instrument: &str, delta: &OrderBookEvent) {
        let key = self.key("deltas", exchange, instrument);
        if let Ok(json) = serde_json::to_string(delta) {
            if let Ok(mut conn) = self.client.get_connection() {
                let _: redis::RedisResult<()> = redis::pipe()
                    .atomic()
                    .cmd("RPUSH")
                    .arg(&key)
                    .arg(&json)
                    .cmd("LTRIM")
                    .arg(&key)
                    .arg(-(MAX_LIST_LEN as isize))
                    .arg(-1)
                    .query(&mut conn);
            }
        }
    }

    fn store_trade(&self, exchange: ExchangeId, instrument: &str, trade: &PublicTrade) {
        let key = self.key("trades", exchange, instrument);
        if let Ok(json) = serde_json::to_string(trade) {
            if let Ok(mut conn) = self.client.get_connection() {
                let _: redis::RedisResult<()> = redis::pipe()
                    .atomic()
                    .cmd("RPUSH")
                    .arg(&key)
                    .arg(&json)
                    .cmd("LTRIM")
                    .arg(&key)
                    .arg(-(MAX_LIST_LEN as isize))
                    .arg(-1)
                    .query(&mut conn);
            }
        }
    }

    fn get_snapshot(&self, exchange: ExchangeId, instrument: &str) -> Option<OrderBook> {
        let key = self.key("snapshot", exchange, instrument);
        if let Ok(mut conn) = self.client.get_connection() {
            redis::cmd("GET")
                .arg(key)
                .query::<Option<String>>(&mut conn)
                .ok()
                .and_then(|s| s.and_then(|val| serde_json::from_str(&val).ok()))
        } else {
            None
        }
    }

    fn get_deltas(
        &self,
        exchange: ExchangeId,
        instrument: &str,
        limit: usize,
    ) -> Vec<OrderBookEvent> {
        if limit == 0 {
            return Vec::new();
        }
        let key = self.key("deltas", exchange, instrument);
        if let Ok(mut conn) = self.client.get_connection() {
            let start = -(limit as isize);
            redis::cmd("LRANGE")
                .arg(&key)
                .arg(start)
                .arg(-1)
                .query::<Vec<String>>(&mut conn)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|s| serde_json::from_str(&s).ok())
                .collect()
        } else {
            Vec::new()
        }
    }

    fn get_trades(&self, exchange: ExchangeId, instrument: &str, limit: usize) -> Vec<PublicTrade> {
        if limit == 0 {
            return Vec::new();
        }
        let key = self.key("trades", exchange, instrument);
        if let Ok(mut conn) = self.client.get_connection() {
            let start = -(limit as isize);
            redis::cmd("LRANGE")
                .arg(&key)
                .arg(start)
                .arg(-1)
                .query::<Vec<String>>(&mut conn)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|s| serde_json::from_str(&s).ok())
                .collect()
        } else {
            Vec::new()
        }
    }

    fn publish_snapshot(&self, exchange: ExchangeId, instrument: &str, snapshot: &OrderBook) {
        let channel = format!("jb:{}:{}:snapshot", exchange, instrument);
        
        // Create bids and asks arrays
        let bids: Vec<[f64; 2]> = snapshot.bids().levels().iter()
            .map(|level| [level.price.to_f64().unwrap_or(0.0), level.amount.to_f64().unwrap_or(0.0)])
            .collect();
        let asks: Vec<[f64; 2]> = snapshot.asks().levels().iter()
            .map(|level| [level.price.to_f64().unwrap_or(0.0), level.amount.to_f64().unwrap_or(0.0)])
            .collect();
        
        // Create message manually to avoid json! macro dependency issues
        let message = format!(
            r#"{{"type":"snapshot","exchange":"{}","symbol":"{}","timestamp":{},"data":{{"bids":{},"asks":{}}}}}"#,
            exchange,
            instrument,
            Utc::now().timestamp_millis(),
            serde_json::to_string(&bids).unwrap_or_default(),
            serde_json::to_string(&asks).unwrap_or_default()
        );
        
        if let Ok(mut conn) = self.client.get_connection() {
            let _: redis::RedisResult<()> = redis::cmd("PUBLISH")
                .arg(&channel)
                .arg(&message)
                .query(&mut conn);
        }
    }

    fn publish_delta(&self, exchange: ExchangeId, instrument: &str, delta: &OrderBookEvent) {
        let channel = format!("jb:{}:{}:deltas", exchange, instrument);
        
        // Serialize the delta data first
        if let Ok(delta_json) = serde_json::to_string(delta) {
            let message = format!(
                r#"{{"type":"delta","exchange":"{}","symbol":"{}","timestamp":{},"data":{}}}"#,
                exchange,
                instrument,
                Utc::now().timestamp_millis(),
                delta_json
            );
            
            if let Ok(mut conn) = self.client.get_connection() {
                let _: redis::RedisResult<()> = redis::cmd("PUBLISH")
                    .arg(&channel)
                    .arg(&message)
                    .query(&mut conn);
            }
        }
    }

    fn publish_trade(&self, exchange: ExchangeId, instrument: &str, trade: &PublicTrade) {
        let channel = format!("jb:{}:{}:trades", exchange, instrument);
        
        let side = match trade.side {
            Side::Buy => "buy",
            Side::Sell => "sell",
        };
        
        let trade_id = &trade.id;
        
        let message = format!(
            r#"{{"type":"trade","exchange":"{}","symbol":"{}","timestamp":{},"data":{{"id":"{}","price":{},"quantity":{},"side":"{}","is_maker":false}}}}"#,
            exchange,
            instrument,
            Utc::now().timestamp_millis(),
            trade_id,
            trade.price,
            trade.amount,
            side
        );
        
        if let Ok(mut conn) = self.client.get_connection() {
            let _: redis::RedisResult<()> = redis::cmd("PUBLISH")
                .arg(&channel)
                .arg(&message)
                .query(&mut conn);
        }
    }
}
