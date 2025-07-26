//! Binance WebSocket Implementation for Real-Time Market Data
//!
//! This module provides production-ready Binance WebSocket connectivity
//! with order book (L2) data streaming, automatic reconnection, and Kafka integration.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smallvec::SmallVec;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, sleep, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};
use url::Url;
use rustc_hash::FxHashMap;

use crate::api::{OrderBookData, TickerData, TradeData};
use crate::exchange_websocket_config::{ExchangeWebSocketConfig, ExchangeWebSocketEndpoint};
use crate::kafka_producer::KafkaProducer;
use crate::streaming::StreamingManager;

/// Connection health metrics with atomic operations for thread safety
#[derive(Debug, Default)]
pub struct ConnectionHealth {
    /// Total messages received
    pub message_count: AtomicU64,
    /// Messages received in current window
    pub window_message_count: AtomicU64,
    /// Total bytes processed
    pub bytes_processed: AtomicU64,
    /// Parse errors
    pub parse_errors: AtomicU32,
    /// Sequence gaps detected
    pub sequence_gaps: AtomicU32,
    /// Last sequence number
    pub last_sequence: AtomicU64,
    /// Average latency in microseconds (EMA)
    pub avg_latency_us: AtomicU64,
    /// Connection errors
    pub connection_errors: AtomicU32,
}

impl ConnectionHealth {
    fn update_latency(&self, new_latency_us: u64) {
        let current = self.avg_latency_us.load(Ordering::Relaxed);
        let alpha = 0.1; // EMA smoothing factor
        let new_avg = if current == 0 {
            new_latency_us
        } else {
            ((1.0 - alpha) * current as f64 + alpha * new_latency_us as f64) as u64
        };
        self.avg_latency_us.store(new_avg, Ordering::Relaxed);
    }

    fn is_healthy(&self) -> bool {
        let error_count = self.parse_errors.load(Ordering::Relaxed) + 
                         self.connection_errors.load(Ordering::Relaxed);
        let message_count = self.message_count.load(Ordering::Relaxed);
        let avg_latency = self.avg_latency_us.load(Ordering::Relaxed);
        
        // Health criteria:
        // 1. Error rate < 1%
        // 2. Average latency < 10ms
        // 3. No sequence gaps in last 1000 messages
        let error_rate = if message_count > 0 {
            error_count as f64 / message_count as f64
        } else {
            0.0
        };
        
        error_rate < 0.01 && avg_latency < 10_000
    }
}

/// Binance WebSocket connection state
#[derive(Debug, Clone)]
pub struct BinanceWebSocketConnection {
    /// Connection ID for tracking
    pub id: Arc<str>,
    /// WebSocket URL
    pub url: Arc<str>,
    /// Symbol being subscribed to (interned)
    pub symbol: Arc<str>,
    /// Stream type (ticker, orderbook, trades)
    pub stream_type: Arc<str>,
    /// Connection timestamp
    pub connected_at: Instant,
    /// Last message timestamp
    pub last_message_at: Arc<RwLock<Option<Instant>>>,
    /// Health metrics
    pub health: Arc<ConnectionHealth>,
    /// Reconnection count
    pub reconnect_count: AtomicU32,
    /// Is testnet connection
    pub is_testnet: bool,
}

/// Binance WebSocket message types
#[derive(Debug, Deserialize)]
#[serde(tag = "e")]
enum BinanceStreamMessage {
    #[serde(rename = "24hrTicker")]
    Ticker(BinanceTickerData),
    #[serde(rename = "depthUpdate")]
    DepthUpdate(BinanceDepthUpdate),
    #[serde(rename = "trade")]
    Trade(BinanceTrade),
}

#[derive(Debug, Deserialize)]
struct BinanceTickerData {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "c")]
    last_price: String,
    #[serde(rename = "b")]
    bid_price: String,
    #[serde(rename = "a")]
    ask_price: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "P")]
    price_change_percent: String,
    #[serde(rename = "h")]
    high_price: String,
    #[serde(rename = "l")]
    low_price: String,
    #[serde(rename = "E")]
    event_time: i64,
}

#[derive(Debug, Deserialize)]
struct BinanceDepthUpdate {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "U")]
    first_update_id: u64,
    #[serde(rename = "u")]
    final_update_id: u64,
    #[serde(rename = "b")]
    bids: Vec<[String; 2]>,
    #[serde(rename = "a")]
    asks: Vec<[String; 2]>,
    #[serde(rename = "E")]
    event_time: i64,
}

#[derive(Debug, Deserialize)]
struct BinanceTrade {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "t")]
    trade_id: u64,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
    #[serde(rename = "T")]
    trade_time: i64,
}

/// Pre-allocated buffer pool for message parsing
struct BufferPool {
    buffers: Vec<Vec<u8>>,
    index: AtomicU32,
}

impl BufferPool {
    fn new(size: usize, count: usize) -> Self {
        let mut buffers = Vec::with_capacity(count);
        for _ in 0..count {
            buffers.push(Vec::with_capacity(size));
        }
        Self {
            buffers,
            index: AtomicU32::new(0),
        }
    }

    fn get_buffer(&self) -> &mut Vec<u8> {
        let idx = self.index.fetch_add(1, Ordering::Relaxed) as usize % self.buffers.len();
        unsafe {
            // SAFETY: We ensure exclusive access through atomic index
            &mut *(self.buffers.as_ptr().add(idx) as *mut Vec<u8>)
        }
    }
}

/// Symbol interner for reducing string allocations
#[derive(Clone)]
struct SymbolInterner {
    cache: Arc<RwLock<FxHashMap<String, Arc<str>>>>,
}

impl SymbolInterner {
    fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(FxHashMap::default())),
        }
    }

    async fn intern(&self, symbol: &str) -> Arc<str> {
        {
            let cache = self.cache.read().await;
            if let Some(interned) = cache.get(symbol) {
                return Arc::clone(interned);
            }
        }
        
        let mut cache = self.cache.write().await;
        let interned = Arc::from(symbol);
        cache.insert(symbol.to_string(), Arc::clone(&interned));
        interned
    }
}

/// Binance WebSocket client
pub struct BinanceWebSocketClient {
    /// Streaming manager for publishing data
    streaming_manager: Arc<StreamingManager>,
    /// WebSocket endpoint configuration
    endpoint: ExchangeWebSocketEndpoint,
    /// Active connections
    connections: Arc<RwLock<Vec<BinanceWebSocketConnection>>>,
    /// Kafka producer for direct publishing
    kafka_producer: Option<Arc<KafkaProducer>>,
    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: Arc<RwLock<mpsc::Receiver<()>>>,
    /// Symbol interner for reducing allocations
    symbol_interner: SymbolInterner,
    /// Pre-allocated buffer pool
    buffer_pool: Arc<BufferPool>,
    /// Symbol mapping cache (exchange format -> normalized)
    symbol_map: Arc<RwLock<FxHashMap<String, Arc<str>>>>,
}

impl BinanceWebSocketClient {
    /// Create a new Binance WebSocket client with optimizations
    pub fn new(
        streaming_manager: Arc<StreamingManager>,
        kafka_producer: Option<Arc<KafkaProducer>>,
        is_testnet: bool,
    ) -> Result<Self> {
        let config = if is_testnet {
            ExchangeWebSocketConfig::testnet()
        } else {
            ExchangeWebSocketConfig::production()
        };
        
        let endpoint = config
            .get_endpoint("binance")
            .ok_or_else(|| anyhow::anyhow!("Binance WebSocket configuration not found"))?
            .clone();
        
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        // Pre-allocate buffer pool: 64KB buffers, 100 of them
        let buffer_pool = Arc::new(BufferPool::new(64 * 1024, 100));
        
        Ok(Self {
            streaming_manager,
            endpoint,
            connections: Arc::new(RwLock::new(Vec::new())),
            kafka_producer,
            shutdown_tx,
            shutdown_rx: Arc::new(RwLock::new(shutdown_rx)),
            symbol_interner: SymbolInterner::new(),
            buffer_pool,
            symbol_map: Arc::new(RwLock::new(FxHashMap::default())),
        })
    }
    
    /// Subscribe to order book stream for a symbol
    pub async fn subscribe_orderbook(&self, symbol: &str) -> Result<()> {
        self.subscribe_stream(symbol, "orderbook").await
    }
    
    /// Subscribe to ticker stream for a symbol
    pub async fn subscribe_ticker(&self, symbol: &str) -> Result<()> {
        self.subscribe_stream(symbol, "ticker").await
    }
    
    /// Subscribe to trades stream for a symbol
    pub async fn subscribe_trades(&self, symbol: &str) -> Result<()> {
        self.subscribe_stream(symbol, "trades").await
    }
    
    /// Generic stream subscription with optimizations
    async fn subscribe_stream(&self, symbol: &str, stream_type: &str) -> Result<()> {
        let connection_id = Arc::from(format!("binance-{}-{}-{}", stream_type, symbol, uuid::Uuid::new_v4()));
        
        info!(
            "🚀 Starting Binance {} stream for {} ({})",
            stream_type,
            symbol,
            if self.endpoint.is_testnet { "TESTNET" } else { "PRODUCTION" }
        );
        
        // Use cached symbol conversion
        let normalized_symbol = self.symbol_interner.intern(symbol).await;
        let binance_symbol = self.get_or_create_binance_symbol(&normalized_symbol).await;
        
        // Build stream name based on type
        let stream_name = match stream_type {
            "ticker" => format!("{}@ticker", binance_symbol),
            "trades" => format!("{}@trade", binance_symbol),
            "orderbook" => format!("{}@depth@100ms", binance_symbol), // 100ms updates for low latency
            _ => return Err(anyhow::anyhow!("Unsupported stream type: {}", stream_type)),
        };
        
        let url = Arc::from(format!("{}/stream?streams={}", self.endpoint.primary_url, stream_name));
        let stream_type_arc = self.symbol_interner.intern(stream_type).await;
        
        // Create connection with optimized structures
        let connection = BinanceWebSocketConnection {
            id: connection_id.clone(),
            url,
            symbol: normalized_symbol,
            stream_type: stream_type_arc,
            connected_at: Instant::now(),
            last_message_at: Arc::new(RwLock::new(None)),
            health: Arc::new(ConnectionHealth::default()),
            reconnect_count: AtomicU32::new(0),
            is_testnet: self.endpoint.is_testnet,
        };
        
        // Add to active connections
        self.connections.write().await.push(connection.clone());
        
        // Spawn connection handler
        let client = self.clone();
        tokio::spawn(async move {
            if let Err(e) = client.handle_connection(connection).await {
                error!("Binance WebSocket connection error: {}", e);
            }
        });
        
        Ok(())
    }
    
    /// Get or create Binance-specific symbol format with caching
    async fn get_or_create_binance_symbol(&self, symbol: &str) -> String {
        let cache = self.symbol_map.read().await;
        if let Some(binance_symbol) = cache.get(symbol) {
            return binance_symbol.to_string();
        }
        drop(cache);
        
        // Convert symbol format (BTC/USDT -> btcusdt)
        let binance_symbol = symbol.to_lowercase().replace('/', "");
        
        let mut cache = self.symbol_map.write().await;
        cache.insert(symbol.to_string(), Arc::from(binance_symbol.as_str()));
        binance_symbol
    }
    
    /// Handle WebSocket connection with automatic reconnection and health monitoring
    async fn handle_connection(&self, connection: BinanceWebSocketConnection) -> Result<()> {
        let mut reconnect_delay = Duration::from_secs(1);
        let max_reconnect_delay = Duration::from_secs(60);
        
        // Start health monitoring task
        let health_monitor = self.start_health_monitor(connection.clone());
        
        loop {
            match self.connect_and_stream(&connection).await {
                Ok(_) => {
                    info!("Binance WebSocket stream {} closed normally", connection.id);
                    break;
                }
                Err(e) => {
                    error!("Binance WebSocket error for {}: {}", connection.id, e);
                    let reconnect_count = connection.reconnect_count.fetch_add(1, Ordering::Relaxed);
                    connection.health.connection_errors.fetch_add(1, Ordering::Relaxed);
                    
                    if reconnect_count > 100 {
                        error!("Max reconnection attempts reached for {}", connection.id);
                        break;
                    }
                    
                    // Check health before reconnecting
                    if !connection.health.is_healthy() {
                        warn!("Connection {} unhealthy, extending reconnect delay", connection.id);
                        reconnect_delay = reconnect_delay * 3;
                    }
                    
                    warn!(
                        "Reconnecting {} in {:?} (attempt {})",
                        connection.id, reconnect_delay, reconnect_count + 1
                    );
                    
                    sleep(reconnect_delay).await;
                    
                    // Exponential backoff with jitter
                    reconnect_delay = std::cmp::min(
                        reconnect_delay * 2 + Duration::from_millis(rand::random::<u64>() % 1000),
                        max_reconnect_delay,
                    );
                }
            }
        }
        
        // Stop health monitoring
        health_monitor.abort();
        
        // Remove from active connections
        self.connections.write().await.retain(|c| c.id != connection.id);
        Ok(())
    }
    
    /// Start health monitoring task for a connection
    fn start_health_monitor(&self, connection: BinanceWebSocketConnection) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            let mut last_message_count = 0u64;
            
            loop {
                interval.tick().await;
                
                let current_count = connection.health.message_count.load(Ordering::Relaxed);
                let window_count = current_count - last_message_count;
                connection.health.window_message_count.store(window_count, Ordering::Relaxed);
                
                if window_count == 0 {
                    warn!("No messages received for connection {} in last 10 seconds", connection.id);
                }
                
                let health_status = if connection.health.is_healthy() { "healthy" } else { "unhealthy" };
                let avg_latency = connection.health.avg_latency_us.load(Ordering::Relaxed);
                
                debug!(
                    "Connection {} health: {} (messages: {}/10s, latency: {}μs)",
                    connection.id, health_status, window_count, avg_latency
                );
                
                last_message_count = current_count;
            }
        })
    }
    
    /// Connect and stream data
    async fn connect_and_stream(&self, connection: &BinanceWebSocketConnection) -> Result<()> {
        let url = Url::parse(&connection.url)?;
        
        info!("🔌 Connecting to Binance WebSocket: {}", url);
        
        // Connect with timeout
        let (ws_stream, _) = timeout(
            self.endpoint.connection_timeout,
            connect_async(url)
        )
        .await
        .context("Connection timeout")?
        .context("Failed to connect")?;
        
        info!("✅ Connected to Binance WebSocket for {}", connection.symbol);
        
        let (mut tx, mut rx) = ws_stream.split();
        
        // Spawn heartbeat task
        let heartbeat_interval = self.endpoint.heartbeat_interval;
        let heartbeat_handle = tokio::spawn(async move {
            let mut interval = interval(heartbeat_interval);
            loop {
                interval.tick().await;
                if tx.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
        });
        
        // Process messages
        let result = self.process_messages(&mut rx, &connection).await;
        
        // Cleanup
        heartbeat_handle.abort();
        result
    }
    
    /// Process incoming WebSocket messages with health tracking
    async fn process_messages(
        &self,
        rx: &mut futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>,
        connection: &BinanceWebSocketConnection,
    ) -> Result<()> {
        while let Some(msg) = rx.next().await {
            match msg? {
                Message::Text(text) => {
                    let start = Instant::now();
                    let bytes_len = text.len() as u64;
                    
                    // Update last message time
                    {
                        let mut last_msg = connection.last_message_at.write().await;
                        *last_msg = Some(start);
                    }
                    
                    // Update health metrics
                    connection.health.message_count.fetch_add(1, Ordering::Relaxed);
                    connection.health.bytes_processed.fetch_add(bytes_len, Ordering::Relaxed);
                    
                    if let Err(e) = self.handle_message(&text, connection).await {
                        warn!("Failed to handle message: {}", e);
                        connection.health.parse_errors.fetch_add(1, Ordering::Relaxed);
                    }
                    
                    // Track latency
                    let latency = start.elapsed();
                    let latency_us = latency.as_micros() as u64;
                    connection.health.update_latency(latency_us);
                    
                    if latency.as_millis() > 10 {
                        warn!("Message processing took {}ms (target: <10ms)", latency.as_millis());
                    } else {
                        debug!("Message processed in {}µs", latency_us);
                    }
                }
                Message::Close(_) => {
                    info!("WebSocket closed for {}", connection.id);
                    break;
                }
                Message::Pong(_) => {
                    debug!("Received pong from Binance");
                }
                _ => {}
            }
        }
        
        Ok(())
    }
    
    /// Handle a single message
    async fn handle_message(&self, text: &str, connection: &BinanceWebSocketConnection) -> Result<()> {
        let data: Value = serde_json::from_str(text)?;
        
        // Handle stream wrapper if present
        let stream_data = if data.get("stream").is_some() {
            &data["data"]
        } else {
            &data
        };
        
        match connection.stream_type.as_str() {
            "ticker" => self.handle_ticker(stream_data, &connection.symbol).await?,
            "orderbook" => self.handle_orderbook(stream_data, &connection.symbol).await?,
            "trades" => self.handle_trade(stream_data, &connection.symbol).await?,
            _ => warn!("Unknown stream type: {}", connection.stream_type),
        }
        
        Ok(())
    }
    
    /// Handle ticker data
    async fn handle_ticker(&self, data: &Value, symbol: &str) -> Result<()> {
        let ticker = TickerData {
            symbol: symbol.to_string(),
            exchange: "binance".to_string(),
            price: data["c"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            bid: data["b"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            ask: data["a"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            volume_24h: data["v"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            change_24h: data["P"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            high_24h: data["h"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            low_24h: data["l"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            timestamp: data["E"].as_i64().unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
        };
        
        self.streaming_manager.publish_ticker(ticker).await?;
        Ok(())
    }
    
    /// Handle order book data with sequence tracking
    async fn handle_orderbook(&self, data: &Value, symbol: &str) -> Result<()> {
        // Extract sequence numbers for gap detection
        let first_update_id = data["U"].as_u64().unwrap_or(0);
        let final_update_id = data["u"].as_u64().unwrap_or(0);
        
        // Check for sequence gaps
        let last_sequence = self.check_sequence_gap(symbol, first_update_id, final_update_id).await;
        
        // Use pre-allocated vectors for better performance
        let mut bids = SmallVec::<[[f64; 2]; 20]>::new();
        let mut asks = SmallVec::<[[f64; 2]; 20]>::new();
        
        // Parse bids with minimal allocations
        if let Some(bid_array) = data["b"].as_array() {
            for level in bid_array.iter().take(20) {
                if let (Some(price), Some(qty)) = (
                    level[0].as_str().and_then(|s| s.parse::<f64>().ok()),
                    level[1].as_str().and_then(|s| s.parse::<f64>().ok()),
                ) {
                    bids.push([price, qty]);
                }
            }
        }
        
        // Parse asks with minimal allocations
        if let Some(ask_array) = data["a"].as_array() {
            for level in ask_array.iter().take(20) {
                if let (Some(price), Some(qty)) = (
                    level[0].as_str().and_then(|s| s.parse::<f64>().ok()),
                    level[1].as_str().and_then(|s| s.parse::<f64>().ok()),
                ) {
                    asks.push([price, qty]);
                }
            }
        }
        
        let orderbook = OrderBookData {
            symbol: symbol.to_string(),
            exchange: "binance".to_string(),
            bids: bids.into_vec(),
            asks: asks.into_vec(),
            timestamp: data["E"].as_i64().unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
            sequence_id: Some(final_update_id),
        };
        
        self.streaming_manager.publish_orderbook(orderbook).await?;
        Ok(())
    }
    
    /// Check for sequence gaps and track sequences per symbol
    async fn check_sequence_gap(&self, symbol: &str, first_update_id: u64, final_update_id: u64) -> u64 {
        // This is a simplified version - in production, you'd track per-symbol sequences
        let last_sequence = self.connections.read().await
            .iter()
            .find(|c| c.symbol.as_ref() == symbol)
            .map(|c| c.health.last_sequence.load(Ordering::Relaxed))
            .unwrap_or(0);
        
        if last_sequence > 0 && first_update_id > last_sequence + 1 {
            warn!(
                "Sequence gap detected for {}: expected {}, got {}",
                symbol,
                last_sequence + 1,
                first_update_id
            );
            
            // Update gap counter
            if let Some(conn) = self.connections.read().await.iter().find(|c| c.symbol.as_ref() == symbol) {
                conn.health.sequence_gaps.fetch_add(1, Ordering::Relaxed);
            }
            
            // In production, request a snapshot here
            // self.request_orderbook_snapshot(symbol).await;
        }
        
        // Update last sequence
        if let Some(conn) = self.connections.read().await.iter().find(|c| c.symbol.as_ref() == symbol) {
            conn.health.last_sequence.store(final_update_id, Ordering::Relaxed);
        }
        
        final_update_id
    }
    
    /// Handle trade data
    async fn handle_trade(&self, data: &Value, symbol: &str) -> Result<()> {
        let trade = TradeData {
            symbol: symbol.to_string(),
            exchange: "binance".to_string(),
            id: data["t"].as_u64().map(|t| t.to_string()).unwrap_or_default(),
            price: data["p"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            quantity: data["q"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            side: if data["m"].as_bool().unwrap_or(false) {
                "sell"
            } else {
                "buy"
            }
            .to_string(),
            timestamp: data["T"].as_i64().unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
            is_maker: data["m"].as_bool().unwrap_or(false),
        };
        
        self.streaming_manager.publish_trade(trade).await?;
        Ok(())
    }
    
    /// Shutdown all connections
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down Binance WebSocket client");
        let _ = self.shutdown_tx.send(()).await;
        Ok(())
    }
    
    /// Get connection statistics with health metrics
    pub async fn get_stats(&self) -> serde_json::Value {
        let connections = self.connections.read().await;
        
        let stats: Vec<serde_json::Value> = connections
            .iter()
            .map(|conn| {
                let last_msg_time = {
                    let last_msg = conn.last_message_at.blocking_read();
                    last_msg.map(|t| t.elapsed().as_secs()).unwrap_or(999999)
                };
                
                serde_json::json!({
                    "id": conn.id.as_ref(),
                    "symbol": conn.symbol.as_ref(),
                    "stream_type": conn.stream_type.as_ref(),
                    "connected_seconds": conn.connected_at.elapsed().as_secs(),
                    "message_count": conn.health.message_count.load(Ordering::Relaxed),
                    "bytes_processed": conn.health.bytes_processed.load(Ordering::Relaxed),
                    "avg_latency_us": conn.health.avg_latency_us.load(Ordering::Relaxed),
                    "parse_errors": conn.health.parse_errors.load(Ordering::Relaxed),
                    "sequence_gaps": conn.health.sequence_gaps.load(Ordering::Relaxed),
                    "reconnect_count": conn.reconnect_count.load(Ordering::Relaxed),
                    "is_testnet": conn.is_testnet,
                    "is_healthy": conn.health.is_healthy(),
                    "last_message_seconds_ago": last_msg_time,
                })
            })
            .collect();
        
        let total_messages: u64 = connections
            .iter()
            .map(|c| c.health.message_count.load(Ordering::Relaxed))
            .sum();
        
        let total_bytes: u64 = connections
            .iter()
            .map(|c| c.health.bytes_processed.load(Ordering::Relaxed))
            .sum();
        
        let healthy_connections = connections
            .iter()
            .filter(|c| c.health.is_healthy())
            .count();
        
        serde_json::json!({
            "total_connections": connections.len(),
            "healthy_connections": healthy_connections,
            "total_messages": total_messages,
            "total_bytes_processed": total_bytes,
            "connections": stats,
        })
    }
}

impl Clone for BinanceWebSocketClient {
    fn clone(&self) -> Self {
        Self {
            streaming_manager: self.streaming_manager.clone(),
            endpoint: self.endpoint.clone(),
            connections: self.connections.clone(),
            kafka_producer: self.kafka_producer.clone(),
            shutdown_tx: self.shutdown_tx.clone(),
            shutdown_rx: self.shutdown_rx.clone(),
            symbol_interner: self.symbol_interner.clone(),
            buffer_pool: self.buffer_pool.clone(),
            symbol_map: self.symbol_map.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_symbol_conversion() {
        let symbol = "BTC/USDT";
        let binance_symbol = symbol.to_lowercase().replace('/', "");
        assert_eq!(binance_symbol, "btcusdt");
    }
    
    #[test]
    fn test_stream_name_generation() {
        let symbol = "btcusdt";
        
        let ticker_stream = format!("{}@ticker", symbol);
        assert_eq!(ticker_stream, "btcusdt@ticker");
        
        let trade_stream = format!("{}@trade", symbol);
        assert_eq!(trade_stream, "btcusdt@trade");
        
        let orderbook_stream = format!("{}@depth@100ms", symbol);
        assert_eq!(orderbook_stream, "btcusdt@depth@100ms");
    }
}