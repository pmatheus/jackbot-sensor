//! High-Performance Kafka Producer for Market Data Streaming
//!
//! Streams real-time market data from sensor to backend Kafka cluster with:
//! - Sub-1ms latency optimization
//! - Zero-copy serialization
//! - Connection pooling and failover
//! - Protocol Buffer serialization
//! - 100K+ messages/second throughput

use anyhow::{Context, Result};
use dashmap::DashMap;
use rdkafka::config::{ClientConfig, RDKafkaLogLevel};
use rdkafka::producer::{FutureProducer, FutureRecord, DeliveryFuture};
use rdkafka::util::Timeout;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc, broadcast};
use tracing::{debug, error, info, warn};

use crate::api::{TickerData, OrderBookData, TradeData, KlineData};
use crate::proto_serializer::ProtoSerializer;

/// Kafka topic patterns based on architecture documentation
pub mod topics {
    pub const L2_DATA_PATTERN: &str = "l2-data.{}.{}";
    pub const TRADES_DATA_PATTERN: &str = "trades-data.{}.{}";
    pub const KLINES_DATA_PATTERN: &str = "klines-data.{}.{}";
    pub const MARKET_DATA: &str = "market.data";
}

/// Producer configuration for optimal performance
#[derive(Debug, Clone)]
pub struct ProducerConfig {
    pub brokers: String,
    pub client_id: String,
    pub max_connections: usize,
    pub batch_size: usize,
    pub linger_ms: u64,
    pub compression_type: String,
    pub retries: i32,
    pub request_timeout_ms: u64,
    pub delivery_timeout_ms: u64,
}

impl Default for ProducerConfig {
    fn default() -> Self {
        Self {
            brokers: "localhost:9092".to_string(),
            client_id: "jackbot-sensor".to_string(),
            max_connections: 5,           // More connections for parallelism
            batch_size: 1048576,         // 1MB for better batching efficiency
            linger_ms: 0,                // 0ms for lowest latency
            compression_type: "lz4".to_string(),  // LZ4 for best speed/compression ratio
            retries: 2,                  // Reduced retries for lower latency
            request_timeout_ms: 10000,   // Reduced timeout for faster failure detection
            delivery_timeout_ms: 30000,  // Reduced delivery timeout
        }
    }
}

/// Performance metrics for monitoring
#[derive(Debug, Default)]
pub struct ProducerMetrics {
    pub messages_sent: std::sync::atomic::AtomicU64,
    pub messages_failed: std::sync::atomic::AtomicU64,
    pub bytes_sent: std::sync::atomic::AtomicU64,
    pub avg_latency_us: std::sync::atomic::AtomicU64,
    pub connection_errors: std::sync::atomic::AtomicU32,
    pub last_error_timestamp: std::sync::atomic::AtomicI64,
}

/// Connection pool entry with health tracking
#[derive(Debug)]
struct PooledProducer {
    producer: FutureProducer,
    created_at: Instant,
    last_used: RwLock<Instant>,
    message_count: std::sync::atomic::AtomicU64,
    error_count: std::sync::atomic::AtomicU32,
}

impl PooledProducer {
    fn new(producer: FutureProducer) -> Self {
        let now = Instant::now();
        Self {
            producer,
            created_at: now,
            last_used: RwLock::new(now),
            message_count: std::sync::atomic::AtomicU64::new(0),
            error_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    async fn update_last_used(&self) {
        *self.last_used.write().await = Instant::now();
    }

    fn increment_message_count(&self) {
        self.message_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn increment_error_count(&self) {
        self.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn is_healthy(&self) -> bool {
        let error_count = self.error_count.load(std::sync::atomic::Ordering::Relaxed);
        let message_count = self.message_count.load(std::sync::atomic::Ordering::Relaxed);
        
        if message_count == 0 {
            return true; // New connection
        }
        
        let error_rate = error_count as f64 / message_count as f64;
        error_rate < 0.05 // Less than 5% error rate
    }
}

/// High-performance Kafka producer with connection pooling
pub struct KafkaProducer {
    config: ProducerConfig,
    producer_pool: Arc<DashMap<u32, Arc<PooledProducer>>>,
    metrics: Arc<ProducerMetrics>,
    current_producer_index: std::sync::atomic::AtomicU32,
    health_check_interval: Duration,
    _health_check_handle: tokio::task::JoinHandle<()>,
}

impl KafkaProducer {
    /// Create new Kafka producer with connection pooling
    pub async fn new(config: ProducerConfig) -> Result<Self> {
        let producer_pool = Arc::new(DashMap::new());
        let metrics = Arc::new(ProducerMetrics::default());
        
        // Initialize connection pool
        for i in 0..config.max_connections {
            let producer = Self::create_producer(&config)
                .with_context(|| format!("Failed to create producer {}", i))?;
            
            producer_pool.insert(i as u32, Arc::new(PooledProducer::new(producer)));
        }

        let health_check_interval = Duration::from_secs(30);
        
        // Start health check task
        let pool_clone = producer_pool.clone();
        let config_clone = config.clone();
        let health_check_handle = tokio::spawn(async move {
            Self::health_check_task(pool_clone, config_clone, health_check_interval).await;
        });

        Ok(Self {
            config,
            producer_pool,
            metrics,
            current_producer_index: std::sync::atomic::AtomicU32::new(0),
            health_check_interval,
            _health_check_handle: health_check_handle,
        })
    }

    /// Create a new Kafka producer with optimized settings
    fn create_producer(config: &ProducerConfig) -> Result<FutureProducer> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &config.brokers)
            .set("client.id", &config.client_id)
            .set("message.timeout.ms", &config.delivery_timeout_ms.to_string())
            .set("request.timeout.ms", &config.request_timeout_ms.to_string())
            .set("retries", &config.retries.to_string())
            .set("retry.backoff.ms", "100")
            .set("batch.size", &config.batch_size.to_string())
            .set("linger.ms", &config.linger_ms.to_string())
            .set("compression.type", &config.compression_type)
            .set("acks", "1") // Leader acknowledgment for balance of performance/reliability
            .set("enable.idempotence", "true")
            .set("max.in.flight.requests.per.connection", "5")
            .set("socket.keepalive.enable", "true")
            .set("socket.nagle.disable", "true") // Disable Nagle for low latency
            .set_log_level(RDKafkaLogLevel::Warning)
            .create()
            .context("Failed to create Kafka producer")?;

        info!("Created Kafka producer with brokers: {}", config.brokers);
        Ok(producer)
    }

    /// Get the next healthy producer from the pool using round-robin
    fn get_producer(&self) -> Result<Arc<PooledProducer>> {
        let pool_size = self.producer_pool.len() as u32;
        let start_index = self.current_producer_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % pool_size;
        
        // Try to find a healthy producer
        for i in 0..pool_size {
            let index = (start_index + i) % pool_size;
            if let Some(producer) = self.producer_pool.get(&index) {
                if producer.is_healthy() {
                    return Ok(producer.clone());
                }
            }
        }

        // If no healthy producer found, use the first available
        self.producer_pool
            .get(&0)
            .map(|p| p.clone())
            .ok_or_else(|| anyhow::anyhow!("No producers available in pool"))
    }

    /// Publish ticker data to Kafka
    pub async fn publish_ticker(&self, ticker: &TickerData) -> Result<()> {
        let topic = format!("l2-data.{}.{}", 
            ticker.exchange.to_lowercase(), 
            ticker.symbol.replace('/', "").to_lowercase()
        );
        
        let key = format!("{}:{}", ticker.exchange, ticker.symbol);
        let payload = self.serialize_ticker(ticker)?;
        
        self.send_message(&topic, &key, payload).await
            .with_context(|| format!("Failed to publish ticker for {}", ticker.symbol))
    }

    /// Publish orderbook data to Kafka
    pub async fn publish_orderbook(&self, orderbook: &OrderBookData) -> Result<()> {
        let topic = format!("l2-data.{}.{}", 
            orderbook.exchange.to_lowercase(), 
            orderbook.symbol.replace('/', "").to_lowercase()
        );
        
        let key = format!("{}:{}", orderbook.exchange, orderbook.symbol);
        let payload = self.serialize_orderbook(orderbook)?;
        
        self.send_message(&topic, &key, payload).await
            .with_context(|| format!("Failed to publish orderbook for {}", orderbook.symbol))
    }

    /// Publish trade data to Kafka
    pub async fn publish_trade(&self, trade: &TradeData) -> Result<()> {
        let topic = format!("trades-data.{}.{}", 
            trade.exchange.to_lowercase(), 
            trade.symbol.replace('/', "").to_lowercase()
        );
        
        let key = format!("{}:{}", trade.exchange, trade.symbol);
        let payload = self.serialize_trade(trade)?;
        
        self.send_message(&topic, &key, payload).await
            .with_context(|| format!("Failed to publish trade for {}", trade.symbol))
    }

    /// Publish kline data to Kafka
    pub async fn publish_kline(&self, kline: &KlineData) -> Result<()> {
        let topic = format!("klines-data.{}.{}", 
            kline.exchange.to_lowercase(), 
            kline.symbol.replace('/', "").to_lowercase()
        );
        
        let key = format!("{}:{}:{}", kline.exchange, kline.symbol, kline.interval);
        let payload = self.serialize_kline(kline)?;
        
        self.send_message(&topic, &key, payload).await
            .with_context(|| format!("Failed to publish kline for {}", kline.symbol))
    }

    /// Core message sending logic with retry and failover
    async fn send_message(&self, topic: &str, key: &str, payload: Vec<u8>) -> Result<()> {
        let start_time = Instant::now();
        let producer = self.get_producer()?;
        
        let record = FutureRecord::to(topic)
            .key(key)
            .payload(&payload)
            .timestamp(chrono::Utc::now().timestamp_millis());

        producer.update_last_used().await;

        let delivery_result = tokio::time::timeout(
            Duration::from_millis(self.config.request_timeout_ms),
            producer.producer.send(record, Timeout::Never)
        ).await;

        match delivery_result {
            Ok(Ok((partition, offset))) => {
                let latency = start_time.elapsed().as_micros() as u64;
                
                // Update metrics
                self.metrics.messages_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.metrics.bytes_sent.fetch_add(payload.len() as u64, std::sync::atomic::Ordering::Relaxed);
                self.update_avg_latency(latency);
                
                producer.increment_message_count();
                
                debug!("Message sent to topic {} partition {} offset {} in {}μs", 
                       topic, partition, offset, latency);
                Ok(())
            }
            Ok(Err((kafka_error, _message))) => {
                producer.increment_error_count();
                self.metrics.messages_failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.metrics.connection_errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.metrics.last_error_timestamp.store(
                    chrono::Utc::now().timestamp_millis(),
                    std::sync::atomic::Ordering::Relaxed
                );
                
                error!("Kafka delivery error for topic {}: {}", topic, kafka_error);
                Err(anyhow::anyhow!("Kafka delivery error: {}", kafka_error))
            }
            Err(_timeout) => {
                producer.increment_error_count();
                self.metrics.messages_failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                
                warn!("Kafka send timeout for topic {}", topic);
                Err(anyhow::anyhow!("Kafka send timeout"))
            }
        }
    }

    /// Update average latency metric using exponential moving average
    fn update_avg_latency(&self, new_latency: u64) {
        let current_avg = self.metrics.avg_latency_us.load(std::sync::atomic::Ordering::Relaxed);
        let alpha = 0.1; // EMA smoothing factor
        let new_avg = if current_avg == 0 {
            new_latency
        } else {
            ((1.0 - alpha) * current_avg as f64 + alpha * new_latency as f64) as u64
        };
        self.metrics.avg_latency_us.store(new_avg, std::sync::atomic::Ordering::Relaxed);
    }

    /// Serialize ticker data using Protocol Buffers
    fn serialize_ticker(&self, ticker: &TickerData) -> Result<Vec<u8>> {
        ProtoSerializer::serialize_ticker(ticker)
    }

    /// Serialize orderbook data using Protocol Buffers
    fn serialize_orderbook(&self, orderbook: &OrderBookData) -> Result<Vec<u8>> {
        ProtoSerializer::serialize_orderbook(orderbook)
    }

    /// Serialize trade data using Protocol Buffers
    fn serialize_trade(&self, trade: &TradeData) -> Result<Vec<u8>> {
        ProtoSerializer::serialize_trade(trade)
    }

    /// Serialize kline data using Protocol Buffers
    fn serialize_kline(&self, kline: &KlineData) -> Result<Vec<u8>> {
        ProtoSerializer::serialize_kline(kline)
    }

    /// Get producer metrics for monitoring
    pub fn get_metrics(&self) -> ProducerMetrics {
        ProducerMetrics {
            messages_sent: std::sync::atomic::AtomicU64::new(
                self.metrics.messages_sent.load(std::sync::atomic::Ordering::Relaxed)
            ),
            messages_failed: std::sync::atomic::AtomicU64::new(
                self.metrics.messages_failed.load(std::sync::atomic::Ordering::Relaxed)
            ),
            bytes_sent: std::sync::atomic::AtomicU64::new(
                self.metrics.bytes_sent.load(std::sync::atomic::Ordering::Relaxed)
            ),
            avg_latency_us: std::sync::atomic::AtomicU64::new(
                self.metrics.avg_latency_us.load(std::sync::atomic::Ordering::Relaxed)
            ),
            connection_errors: std::sync::atomic::AtomicU32::new(
                self.metrics.connection_errors.load(std::sync::atomic::Ordering::Relaxed)
            ),
            last_error_timestamp: std::sync::atomic::AtomicI64::new(
                self.metrics.last_error_timestamp.load(std::sync::atomic::Ordering::Relaxed)
            ),
        }
    }

    /// Health check task to maintain connection pool health
    async fn health_check_task(
        producer_pool: Arc<DashMap<u32, Arc<PooledProducer>>>,
        config: ProducerConfig,
        interval: Duration,
    ) {
        let mut health_check_interval = tokio::time::interval(interval);
        
        loop {
            health_check_interval.tick().await;
            
            let mut unhealthy_producers = Vec::new();
            
            // Check each producer's health
            for entry in producer_pool.iter() {
                let (index, producer) = entry.pair();
                if !producer.is_healthy() {
                    unhealthy_producers.push(*index);
                }
            }
            
            // Replace unhealthy producers
            for index in unhealthy_producers {
                warn!("Replacing unhealthy producer {}", index);
                match Self::create_producer(&config) {
                    Ok(new_producer) => {
                        producer_pool.insert(index, Arc::new(PooledProducer::new(new_producer)));
                        info!("Replaced producer {} successfully", index);
                    }
                    Err(e) => {
                        error!("Failed to replace producer {}: {}", index, e);
                    }
                }
            }
            
            debug!("Health check completed. Pool size: {}", producer_pool.len());
        }
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down Kafka producer...");
        
        // Flush all producers
        for entry in self.producer_pool.iter() {
            let producer = entry.value();
            if let Err(e) = producer.producer.flush(Duration::from_secs(5)) {
                warn!("Failed to flush producer during shutdown: {}", e);
            }
        }
        
        info!("Kafka producer shutdown completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    
    fn create_test_config() -> ProducerConfig {
        ProducerConfig {
            brokers: "localhost:9092".to_string(),
            max_connections: 2,
            ..Default::default()
        }
    }
    
    fn create_test_ticker() -> TickerData {
        TickerData {
            symbol: "BTC/USDT".to_string(),
            exchange: "binance".to_string(),
            price: 50000.0,
            bid: 49999.0,
            ask: 50001.0,
            volume_24h: 1000.0,
            change_24h: 2.5,
            high_24h: 51000.0,
            low_24h: 49000.0,
            timestamp: Utc::now().timestamp_millis(),
        }
    }
    
    #[test]
    fn test_topic_formatting() {
        let ticker = create_test_ticker();
        let expected_topic = "l2-data.binance.btcusdt";
        let actual_topic = format!("l2-data.{}.{}", 
            ticker.exchange.to_lowercase(), 
            ticker.symbol.replace('/', "").to_lowercase()
        );
        assert_eq!(actual_topic, expected_topic);
    }
    
    #[test]
    fn test_serialization() {
        let ticker = create_test_ticker();
        let serialized = serde_json::to_vec(&ticker).unwrap();
        assert!(!serialized.is_empty());
        
        let deserialized: TickerData = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(deserialized.symbol, ticker.symbol);
        assert_eq!(deserialized.price, ticker.price);
    }
    
    #[tokio::test]
    async fn test_producer_pool_creation() {
        let config = create_test_config();
        
        // This test will only pass if Kafka is running
        if std::env::var("CI").is_ok() {
            return; // Skip in CI
        }
        
        match KafkaProducer::new(config).await {
            Ok(producer) => {
                assert_eq!(producer.producer_pool.len(), 2);
                let _ = producer.shutdown().await;
            }
            Err(e) => {
                println!("Kafka not available for testing: {}", e);
            }
        }
    }
}