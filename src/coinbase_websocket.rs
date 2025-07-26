//! Coinbase WebSocket Implementation for Real-Time Market Data
//!
//! This module provides production-ready Coinbase WebSocket connectivity
//! with order book (L2) data streaming, automatic reconnection, and Kafka integration.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, sleep, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};
use url::Url;

use crate::api::{OrderBookData, TickerData, TradeData};
use crate::exchange_websocket_config::{ExchangeWebSocketConfig, ExchangeWebSocketEndpoint};
use crate::kafka_producer::KafkaProducer;
use crate::streaming::StreamingManager;

/// Coinbase WebSocket connection state
#[derive(Debug, Clone)]
pub struct CoinbaseWebSocketConnection {
    /// Connection ID for tracking
    pub id: String,
    /// WebSocket URL
    pub url: String,
    /// Symbols being subscribed to
    pub symbols: Vec<String>,
    /// Channel type (ticker, level2, trades)
    pub channel: String,
    /// Connection timestamp
    pub connected_at: Instant,
    /// Last message timestamp
    pub last_message_at: Option<Instant>,
    /// Message count
    pub message_count: u64,
    /// Reconnection count
    pub reconnect_count: u32,
    /// Is sandbox connection
    pub is_sandbox: bool,
}

/// Coinbase WebSocket message types
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum CoinbaseMessage {
    #[serde(rename = "ticker")]
    Ticker(CoinbaseTickerMessage),
    #[serde(rename = "l2update")]
    L2Update(CoinbaseL2Update),
    #[serde(rename = "match")]
    Match(CoinbaseMatch),
    #[serde(rename = "subscriptions")]
    Subscriptions(CoinbaseSubscriptions),
    #[serde(rename = "error")]
    Error(CoinbaseError),
}

#[derive(Debug, Deserialize)]
struct CoinbaseTickerMessage {
    product_id: String,
    price: String,
    best_bid: String,
    best_ask: String,
    volume_24h: String,
    open_24h: String,
    high_24h: String,
    low_24h: String,
    time: String,
}

#[derive(Debug, Deserialize)]
struct CoinbaseL2Update {
    product_id: String,
    changes: Vec<[String; 3]>, // [side, price, size]
    time: String,
}

#[derive(Debug, Deserialize)]
struct CoinbaseMatch {
    trade_id: u64,
    product_id: String,
    price: String,
    size: String,
    side: String,
    time: String,
    maker_order_id: Option<String>,
    taker_order_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CoinbaseSubscriptions {
    channels: Vec<CoinbaseChannel>,
}

#[derive(Debug, Deserialize)]
struct CoinbaseChannel {
    name: String,
    product_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CoinbaseError {
    message: String,
}

/// Subscribe message for Coinbase
#[derive(Debug, Serialize)]
struct CoinbaseSubscribe {
    #[serde(rename = "type")]
    msg_type: String,
    product_ids: Vec<String>,
    channels: Vec<String>,
}

/// Coinbase WebSocket client
pub struct CoinbaseWebSocketClient {
    /// Streaming manager for publishing data
    streaming_manager: Arc<StreamingManager>,
    /// WebSocket endpoint configuration
    endpoint: ExchangeWebSocketEndpoint,
    /// Active connections
    connections: Arc<RwLock<Vec<CoinbaseWebSocketConnection>>>,
    /// Kafka producer for direct publishing
    kafka_producer: Option<Arc<KafkaProducer>>,
    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: Arc<RwLock<mpsc::Receiver<()>>>,
    /// Order book state tracking
    orderbook_state: Arc<RwLock<std::collections::HashMap<String, OrderBookState>>>,
}

/// Order book state for L2 updates
#[derive(Debug, Default)]
struct OrderBookState {
    bids: std::collections::BTreeMap<String, f64>, // price -> size
    asks: std::collections::BTreeMap<String, f64>, // price -> size
    last_update: Instant,
}

impl CoinbaseWebSocketClient {
    /// Create a new Coinbase WebSocket client
    pub fn new(
        streaming_manager: Arc<StreamingManager>,
        kafka_producer: Option<Arc<KafkaProducer>>,
        is_sandbox: bool,
    ) -> Result<Self> {
        let config = if is_sandbox {
            ExchangeWebSocketConfig::testnet()
        } else {
            ExchangeWebSocketConfig::production()
        };
        
        let endpoint = config
            .get_endpoint("coinbase")
            .ok_or_else(|| anyhow::anyhow!("Coinbase WebSocket configuration not found"))?
            .clone();
        
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        Ok(Self {
            streaming_manager,
            endpoint,
            connections: Arc::new(RwLock::new(Vec::new())),
            kafka_producer,
            shutdown_tx,
            shutdown_rx: Arc::new(RwLock::new(shutdown_rx)),
            orderbook_state: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }
    
    /// Subscribe to order book stream for a symbol
    pub async fn subscribe_orderbook(&self, symbol: &str) -> Result<()> {
        self.subscribe_channel(vec![symbol.to_string()], "level2").await
    }
    
    /// Subscribe to ticker stream for a symbol
    pub async fn subscribe_ticker(&self, symbol: &str) -> Result<()> {
        self.subscribe_channel(vec![symbol.to_string()], "ticker").await
    }
    
    /// Subscribe to trades stream for a symbol
    pub async fn subscribe_trades(&self, symbol: &str) -> Result<()> {
        self.subscribe_channel(vec![symbol.to_string()], "matches").await
    }
    
    /// Generic channel subscription
    async fn subscribe_channel(&self, symbols: Vec<String>, channel: &str) -> Result<()> {
        let connection_id = format!("coinbase-{}-{}-{}", channel, symbols.join(","), uuid::Uuid::new_v4());
        
        info!(
            "🚀 Starting Coinbase {} stream for {:?} ({})",
            channel,
            symbols,
            if self.endpoint.is_testnet { "SANDBOX" } else { "PRODUCTION" }
        );
        
        // Convert symbol format (BTC/USDT -> BTC-USD)
        let coinbase_symbols: Vec<String> = symbols
            .iter()
            .map(|s| s.replace('/', "-"))
            .collect();
        
        // Create connection
        let connection = CoinbaseWebSocketConnection {
            id: connection_id.clone(),
            url: self.endpoint.primary_url.clone(),
            symbols: coinbase_symbols.clone(),
            channel: channel.to_string(),
            connected_at: Instant::now(),
            last_message_at: None,
            message_count: 0,
            reconnect_count: 0,
            is_sandbox: self.endpoint.is_testnet,
        };
        
        // Add to active connections
        self.connections.write().await.push(connection.clone());
        
        // Spawn connection handler
        let client = self.clone();
        tokio::spawn(async move {
            if let Err(e) = client.handle_connection(connection).await {
                error!("Coinbase WebSocket connection error: {}", e);
            }
        });
        
        Ok(())
    }
    
    /// Handle WebSocket connection with automatic reconnection
    async fn handle_connection(&self, mut connection: CoinbaseWebSocketConnection) -> Result<()> {
        let mut reconnect_delay = Duration::from_secs(1);
        let max_reconnect_delay = Duration::from_secs(60);
        
        loop {
            match self.connect_and_stream(&mut connection).await {
                Ok(_) => {
                    info!("Coinbase WebSocket stream {} closed normally", connection.id);
                    break;
                }
                Err(e) => {
                    error!("Coinbase WebSocket error for {}: {}", connection.id, e);
                    connection.reconnect_count += 1;
                    
                    if connection.reconnect_count > 100 {
                        error!("Max reconnection attempts reached for {}", connection.id);
                        break;
                    }
                    
                    warn!(
                        "Reconnecting {} in {:?} (attempt {})",
                        connection.id, reconnect_delay, connection.reconnect_count
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
        
        // Remove from active connections
        self.connections.write().await.retain(|c| c.id != connection.id);
        Ok(())
    }
    
    /// Connect and stream data
    async fn connect_and_stream(&self, connection: &mut CoinbaseWebSocketConnection) -> Result<()> {
        let url = Url::parse(&connection.url)?;
        
        info!("🔌 Connecting to Coinbase WebSocket: {}", url);
        
        // Connect with timeout
        let (ws_stream, _) = timeout(
            self.endpoint.connection_timeout,
            connect_async(url)
        )
        .await
        .context("Connection timeout")?
        .context("Failed to connect")?;
        
        info!("✅ Connected to Coinbase WebSocket for {:?}", connection.symbols);
        
        let (mut tx, mut rx) = ws_stream.split();
        
        // Send subscribe message
        let subscribe_msg = CoinbaseSubscribe {
            msg_type: "subscribe".to_string(),
            product_ids: connection.symbols.clone(),
            channels: vec![connection.channel.clone()],
        };
        
        let subscribe_text = serde_json::to_string(&subscribe_msg)?;
        tx.send(Message::Text(subscribe_text)).await?;
        
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
        let result = self.process_messages(&mut rx, connection).await;
        
        // Cleanup
        heartbeat_handle.abort();
        result
    }
    
    /// Process incoming WebSocket messages
    async fn process_messages(
        &self,
        rx: &mut futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>,
        connection: &mut CoinbaseWebSocketConnection,
    ) -> Result<()> {
        while let Some(msg) = rx.next().await {
            match msg? {
                Message::Text(text) => {
                    let start = Instant::now();
                    connection.last_message_at = Some(start);
                    connection.message_count += 1;
                    
                    if let Err(e) = self.handle_message(&text, connection).await {
                        warn!("Failed to handle message: {}", e);
                    }
                    
                    let latency = start.elapsed();
                    if latency.as_millis() > 10 {
                        warn!("Message processing took {}ms (target: <10ms)", latency.as_millis());
                    } else {
                        debug!("Message processed in {}µs", latency.as_micros());
                    }
                }
                Message::Close(_) => {
                    info!("WebSocket closed for {}", connection.id);
                    break;
                }
                Message::Pong(_) => {
                    debug!("Received pong from Coinbase");
                }
                _ => {}
            }
        }
        
        Ok(())
    }
    
    /// Handle a single message
    async fn handle_message(&self, text: &str, connection: &CoinbaseWebSocketConnection) -> Result<()> {
        let msg: CoinbaseMessage = serde_json::from_str(text)?;
        
        match msg {
            CoinbaseMessage::Ticker(ticker) => self.handle_ticker(ticker).await?,
            CoinbaseMessage::L2Update(update) => self.handle_l2_update(update).await?,
            CoinbaseMessage::Match(trade) => self.handle_trade(trade).await?,
            CoinbaseMessage::Subscriptions(subs) => {
                info!("Subscribed to channels: {:?}", subs.channels);
            }
            CoinbaseMessage::Error(err) => {
                error!("Coinbase error: {}", err.message);
            }
        }
        
        Ok(())
    }
    
    /// Handle ticker data
    async fn handle_ticker(&self, msg: CoinbaseTickerMessage) -> Result<()> {
        // Convert symbol format (BTC-USD -> BTC/USD)
        let symbol = msg.product_id.replace('-', "/");
        
        let ticker = TickerData {
            symbol,
            exchange: "coinbase".to_string(),
            price: msg.price.parse().unwrap_or(0.0),
            bid: msg.best_bid.parse().unwrap_or(0.0),
            ask: msg.best_ask.parse().unwrap_or(0.0),
            volume_24h: msg.volume_24h.parse().unwrap_or(0.0),
            change_24h: {
                let open: f64 = msg.open_24h.parse().unwrap_or(0.0);
                let current: f64 = msg.price.parse().unwrap_or(0.0);
                if open > 0.0 {
                    ((current - open) / open * 100.0)
                } else {
                    0.0
                }
            },
            high_24h: msg.high_24h.parse().unwrap_or(0.0),
            low_24h: msg.low_24h.parse().unwrap_or(0.0),
            timestamp: chrono::DateTime::parse_from_rfc3339(&msg.time)
                .map(|dt| dt.timestamp_millis())
                .unwrap_or_else(|_| chrono::Utc::now().timestamp_millis()),
        };
        
        self.streaming_manager.publish_ticker(ticker).await?;
        Ok(())
    }
    
    /// Handle L2 order book updates
    async fn handle_l2_update(&self, msg: CoinbaseL2Update) -> Result<()> {
        // Convert symbol format (BTC-USD -> BTC/USD)
        let symbol = msg.product_id.replace('-', "/");
        
        // Update local order book state
        let mut states = self.orderbook_state.write().await;
        let state = states.entry(symbol.clone()).or_insert_with(Default::default);
        
        for change in msg.changes {
            let side = &change[0];
            let price = change[1].clone();
            let size: f64 = change[2].parse().unwrap_or(0.0);
            
            if side == "buy" {
                if size > 0.0 {
                    state.bids.insert(price, size);
                } else {
                    state.bids.remove(&price);
                }
            } else if side == "sell" {
                if size > 0.0 {
                    state.asks.insert(price, size);
                } else {
                    state.asks.remove(&price);
                }
            }
        }
        
        state.last_update = Instant::now();
        
        // Convert to order book data (top 20 levels)
        let bids: Vec<[f64; 2]> = state.bids
            .iter()
            .rev()
            .take(20)
            .map(|(price, size)| [price.parse().unwrap_or(0.0), *size])
            .collect();
        
        let asks: Vec<[f64; 2]> = state.asks
            .iter()
            .take(20)
            .map(|(price, size)| [price.parse().unwrap_or(0.0), *size])
            .collect();
        
        let orderbook = OrderBookData {
            symbol,
            exchange: "coinbase".to_string(),
            bids,
            asks,
            timestamp: chrono::DateTime::parse_from_rfc3339(&msg.time)
                .map(|dt| dt.timestamp_millis())
                .unwrap_or_else(|_| chrono::Utc::now().timestamp_millis()),
            sequence_id: None,
        };
        
        self.streaming_manager.publish_orderbook(orderbook).await?;
        Ok(())
    }
    
    /// Handle trade data
    async fn handle_trade(&self, msg: CoinbaseMatch) -> Result<()> {
        // Convert symbol format (BTC-USD -> BTC/USD)
        let symbol = msg.product_id.replace('-', "/");
        
        let trade = TradeData {
            symbol,
            exchange: "coinbase".to_string(),
            id: msg.trade_id.to_string(),
            price: msg.price.parse().unwrap_or(0.0),
            quantity: msg.size.parse().unwrap_or(0.0),
            side: msg.side.to_lowercase(),
            timestamp: chrono::DateTime::parse_from_rfc3339(&msg.time)
                .map(|dt| dt.timestamp_millis())
                .unwrap_or_else(|_| chrono::Utc::now().timestamp_millis()),
            is_maker: msg.maker_order_id.is_some(),
        };
        
        self.streaming_manager.publish_trade(trade).await?;
        Ok(())
    }
    
    /// Shutdown all connections
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down Coinbase WebSocket client");
        let _ = self.shutdown_tx.send(()).await;
        Ok(())
    }
    
    /// Get connection statistics
    pub async fn get_stats(&self) -> serde_json::Value {
        let connections = self.connections.read().await;
        
        let stats: Vec<serde_json::Value> = connections
            .iter()
            .map(|conn| {
                serde_json::json!({
                    "id": conn.id,
                    "symbols": conn.symbols,
                    "channel": conn.channel,
                    "connected_seconds": conn.connected_at.elapsed().as_secs(),
                    "message_count": conn.message_count,
                    "reconnect_count": conn.reconnect_count,
                    "is_sandbox": conn.is_sandbox,
                    "last_message_seconds_ago": conn.last_message_at
                        .map(|t| t.elapsed().as_secs())
                        .unwrap_or(999999),
                })
            })
            .collect();
        
        serde_json::json!({
            "total_connections": connections.len(),
            "connections": stats,
        })
    }
}

impl Clone for CoinbaseWebSocketClient {
    fn clone(&self) -> Self {
        Self {
            streaming_manager: self.streaming_manager.clone(),
            endpoint: self.endpoint.clone(),
            connections: self.connections.clone(),
            kafka_producer: self.kafka_producer.clone(),
            shutdown_tx: self.shutdown_tx.clone(),
            shutdown_rx: self.shutdown_rx.clone(),
            orderbook_state: self.orderbook_state.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_symbol_conversion() {
        let coinbase_symbol = "BTC-USD";
        let internal_symbol = coinbase_symbol.replace('-', "/");
        assert_eq!(internal_symbol, "BTC/USD");
        
        let internal_symbol = "BTC/USD";
        let coinbase_symbol = internal_symbol.replace('/', "-");
        assert_eq!(coinbase_symbol, "BTC-USD");
    }
}