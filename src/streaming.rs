use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc, broadcast, Semaphore};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use tracing::{info, warn, error, debug};
use uuid::Uuid;
use url::Url;

use crate::api::{TickerData, OrderBookData, TradeData, KlineData, PositionData, BalanceData, OrderResponse};
use crate::production_config::ProductionConfig;
use crate::kafka_producer::{KafkaProducer, ProducerConfig};

/// Enum representing different types of streaming events
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Ticker(TickerData),
    OrderBook(OrderBookData),
    Trade(TradeData),
    Kline(KlineData),
    Position(PositionData),
    Balance(BalanceData),
    Order(OrderResponse),
}

#[derive(Debug, Clone)]
pub struct StreamConnection {
    pub id: String,
    pub exchange: String,
    pub connection_type: String,
    pub last_ping: Instant,
    pub is_active: bool,
}

#[derive(Debug)]
pub struct LatencyTracker {
    pub measurements: Vec<u64>,
    pub last_update: Instant,
}

impl LatencyTracker {
    pub fn new() -> Self {
        Self {
            measurements: Vec::new(),
            last_update: Instant::now(),
        }
    }
}



// WebSocket message types according to API contract
#[derive(Debug, Serialize, Deserialize)]
pub struct WebSocketStreamMessage {
    pub channel: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
    pub sequence: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub channel: String,
    pub connection_id: String,
    pub user_id: Option<String>,
    pub created_at: i64,
}

/// Channel capacity constants to prevent memory exhaustion
const BOUNDED_CHANNEL_CAPACITY: usize = 10_000;
const BROADCAST_CHANNEL_CAPACITY: usize = 50_000;
const BACKPRESSURE_TIMEOUT_MS: u64 = 100;

/// Backpressure metrics
#[derive(Debug, Default)]
pub struct BackpressureMetrics {
    pub dropped_messages: std::sync::atomic::AtomicU64,
    pub backpressure_events: std::sync::atomic::AtomicU64,
    pub overflow_events: std::sync::atomic::AtomicU64,
    pub successful_sends: std::sync::atomic::AtomicU64,
}

#[derive(Clone)]
pub struct StreamingManager {
    subscriptions: Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
    connections: Arc<RwLock<HashMap<String, mpsc::Sender<String>>>>, // BOUNDED CHANNEL
    ticker_sender: broadcast::Sender<TickerData>,
    orderbook_sender: broadcast::Sender<OrderBookData>,
    trade_sender: broadcast::Sender<TradeData>,
    kline_sender: broadcast::Sender<KlineData>,
    order_sender: broadcast::Sender<OrderResponse>,
    position_sender: broadcast::Sender<PositionData>,
    balance_sender: broadcast::Sender<BalanceData>,
    // Production streaming components
    production_config: Arc<ProductionConfig>,
    active_streams: Arc<RwLock<HashMap<String, StreamConnection>>>,
    latency_tracker: Arc<RwLock<LatencyTracker>>,
    // Backpressure management
    backpressure_semaphore: Arc<Semaphore>,
    backpressure_metrics: Arc<BackpressureMetrics>,
    // Kafka producer for backend streaming
    kafka_producer: Option<Arc<KafkaProducer>>,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self::new_with_kafka(None)
    }

    pub fn new_with_kafka(kafka_producer: Option<Arc<KafkaProducer>>) -> Self {
        let (ticker_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        let (orderbook_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        let (trade_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        let (kline_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        let (order_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        let (position_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        let (balance_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            ticker_sender,
            orderbook_sender,
            trade_sender,
            kline_sender,
            order_sender,
            position_sender,
            balance_sender,
            production_config: Arc::new(ProductionConfig::default()),
            active_streams: Arc::new(RwLock::new(HashMap::new())),
            latency_tracker: Arc::new(RwLock::new(LatencyTracker::new())),
            backpressure_semaphore: Arc::new(Semaphore::new(BOUNDED_CHANNEL_CAPACITY)),
            backpressure_metrics: Arc::new(BackpressureMetrics::default()),
            kafka_producer,
        }
    }
    
    pub async fn add_connection(
        &self,
        connection_id: String,
        sender: mpsc::Sender<String>,
    ) {
        self.connections.write().await.insert(connection_id.clone(), sender);
        info!("Added WebSocket connection with bounded channel: {}", connection_id);
    }

    /// Send message with backpressure handling and graceful degradation
    async fn send_with_backpressure(&self, sender: &mpsc::Sender<String>, message: String, connection_id: &str) {
        // Try to acquire permit for backpressure control
        let _permit = match tokio::time::timeout(
            Duration::from_millis(BACKPRESSURE_TIMEOUT_MS),
            self.backpressure_semaphore.acquire()
        ).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                // Semaphore closed - graceful degradation
                self.backpressure_metrics.backpressure_events.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!("Backpressure semaphore closed for connection {}", connection_id);
                return;
            }
            Err(_) => {
                // Timeout - drop message to prevent memory exhaustion
                self.backpressure_metrics.dropped_messages.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!("Backpressure timeout for connection {} - dropping message", connection_id);
                return;
            }
        };

        // Attempt to send with timeout
        match tokio::time::timeout(
            Duration::from_millis(BACKPRESSURE_TIMEOUT_MS),
            sender.send(message)
        ).await {
            Ok(Ok(_)) => {
                self.backpressure_metrics.successful_sends.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(Err(_)) => {
                // Channel closed - remove connection
                warn!("Channel closed for connection {}", connection_id);
                self.remove_connection(connection_id).await;
            }
            Err(_) => {
                // Send timeout - backpressure detected
                self.backpressure_metrics.backpressure_events.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!("Send timeout for connection {} - applying backpressure", connection_id);
            }
        }
    }

    /// Get backpressure metrics for monitoring
    pub fn get_backpressure_metrics(&self) -> BackpressureMetrics {
        BackpressureMetrics {
            dropped_messages: std::sync::atomic::AtomicU64::new(
                self.backpressure_metrics.dropped_messages.load(std::sync::atomic::Ordering::Relaxed)
            ),
            backpressure_events: std::sync::atomic::AtomicU64::new(
                self.backpressure_metrics.backpressure_events.load(std::sync::atomic::Ordering::Relaxed)
            ),
            overflow_events: std::sync::atomic::AtomicU64::new(
                self.backpressure_metrics.overflow_events.load(std::sync::atomic::Ordering::Relaxed)
            ),
            successful_sends: std::sync::atomic::AtomicU64::new(
                self.backpressure_metrics.successful_sends.load(std::sync::atomic::Ordering::Relaxed)
            ),
        }
    }
    
    pub async fn remove_connection(&self, connection_id: &str) {
        self.connections.write().await.remove(connection_id);
        
        // Remove all subscriptions for this connection
        let mut subscriptions = self.subscriptions.write().await;
        for (_, subs) in subscriptions.iter_mut() {
            subs.retain(|sub| sub.connection_id != connection_id);
        }
        
        info!("Removed WebSocket connection: {}", connection_id);
    }
    
    pub async fn subscribe(
        &self,
        connection_id: String,
        user_id: Option<String>,
        channels: Vec<String>,
    ) -> Result<()> {
        let mut subscriptions = self.subscriptions.write().await;
        
        for channel in channels {
            if self.is_valid_channel(&channel, &user_id) {
                let subscription = Subscription {
                    channel: channel.clone(),
                    connection_id: connection_id.clone(),
                    user_id: user_id.clone(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                };
                
                subscriptions
                    .entry(channel.clone())
                    .or_insert_with(Vec::new)
                    .push(subscription);
                
                debug!("Subscribed connection {} to channel: {}", connection_id, channel);
                
                // Start streaming for market data channels
                if channel.starts_with("ticker:") || 
                   channel.starts_with("orderbook:") || 
                   channel.starts_with("trades:") || 
                   channel.starts_with("klines:") {
                    self.start_market_data_stream(&channel).await?;
                }
            } else {
                warn!("Invalid subscription attempt - channel: {}, user: {:?}", channel, user_id);
            }
        }
        
        Ok(())
    }
    
    pub async fn unsubscribe(
        &self,
        connection_id: &str,
        channels: Vec<String>,
    ) -> Result<()> {
        let mut subscriptions = self.subscriptions.write().await;
        
        for channel in channels {
            if let Some(subs) = subscriptions.get_mut(&channel) {
                subs.retain(|sub| sub.connection_id != connection_id);
                if subs.is_empty() {
                    subscriptions.remove(&channel);
                }
                debug!("Unsubscribed connection {} from channel: {}", connection_id, channel);
            }
        }
        
        Ok(())
    }
    
    // Market data publishing methods
    pub async fn publish_ticker(&self, ticker: TickerData) -> Result<()> {
        let channel = format!("ticker:{}:{}", ticker.symbol, ticker.exchange);
        
        let message = WebSocketStreamMessage {
            channel: channel.clone(),
            message_type: "ticker".to_string(),
            data: serde_json::to_value(&ticker)?,
            timestamp: ticker.timestamp,
            sequence: None,
        };
        
        // Send to WebSocket subscribers
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.ticker_sender.send(ticker.clone());
        
        // Send to Kafka if producer is available
        if let Some(ref kafka_producer) = self.kafka_producer {
            if let Err(e) = kafka_producer.publish_ticker(&ticker).await {
                error!("Failed to publish ticker to Kafka: {}", e);
            } else {
                debug!("Published ticker to Kafka: {}:{}", ticker.exchange, ticker.symbol);
            }
        }
        
        Ok(())
    }
    
    pub async fn publish_orderbook(&self, orderbook: OrderBookData) -> Result<()> {
        let depth = orderbook.bids.len().max(orderbook.asks.len());
        let channel = format!("orderbook:{}:{}:{}", orderbook.symbol, orderbook.exchange, depth);
        
        let message = WebSocketStreamMessage {
            channel: channel.clone(),
            message_type: "orderbook".to_string(),
            data: serde_json::to_value(&orderbook)?,
            timestamp: orderbook.timestamp,
            sequence: orderbook.sequence_id,
        };
        
        // Send to WebSocket subscribers
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.orderbook_sender.send(orderbook.clone());
        
        // Send to Kafka if producer is available
        if let Some(ref kafka_producer) = self.kafka_producer {
            if let Err(e) = kafka_producer.publish_orderbook(&orderbook).await {
                error!("Failed to publish orderbook to Kafka: {}", e);
            } else {
                debug!("Published orderbook to Kafka: {}:{}", orderbook.exchange, orderbook.symbol);
            }
        }
        
        Ok(())
    }
    
    pub async fn publish_trade(&self, trade: TradeData) -> Result<()> {
        let channel = format!("trades:{}:{}", trade.symbol, trade.exchange);
        
        let message = WebSocketStreamMessage {
            channel: channel.clone(),
            message_type: "trade".to_string(),
            data: serde_json::to_value(&trade)?,
            timestamp: trade.timestamp,
            sequence: None,
        };
        
        // Send to WebSocket subscribers
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.trade_sender.send(trade.clone());
        
        // Send to Kafka if producer is available
        if let Some(ref kafka_producer) = self.kafka_producer {
            if let Err(e) = kafka_producer.publish_trade(&trade).await {
                error!("Failed to publish trade to Kafka: {}", e);
            } else {
                debug!("Published trade to Kafka: {}:{}", trade.exchange, trade.symbol);
            }
        }
        
        Ok(())
    }
    
    pub async fn publish_kline(&self, kline: KlineData) -> Result<()> {
        let channel = format!("klines:{}:{}:{}", kline.symbol, kline.exchange, kline.interval);
        
        let message = WebSocketStreamMessage {
            channel: channel.clone(),
            message_type: "kline".to_string(),
            data: serde_json::to_value(&kline)?,
            timestamp: kline.close_time,
            sequence: None,
        };
        
        // Send to WebSocket subscribers
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.kline_sender.send(kline.clone());
        
        // Send to Kafka if producer is available
        if let Some(ref kafka_producer) = self.kafka_producer {
            if let Err(e) = kafka_producer.publish_kline(&kline).await {
                error!("Failed to publish kline to Kafka: {}", e);
            } else {
                debug!("Published kline to Kafka: {}:{}:{}", kline.exchange, kline.symbol, kline.interval);
            }
        }
        
        Ok(())
    }
    
    // Account data publishing methods
    pub async fn publish_order_update(&self, order: OrderResponse) -> Result<()> {
        let channel = format!("orders:{}", order.user_id);
        
        let message = WebSocketStreamMessage {
            channel: channel.clone(),
            message_type: "order".to_string(),
            data: serde_json::to_value(&order)?,
            timestamp: order.updated_at,
            sequence: None,
        };
        
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.order_sender.send(order);
        
        Ok(())
    }
    
    pub async fn publish_position_update(&self, position: PositionData) -> Result<()> {
        let channel = format!("positions:{}", position.user_id);
        
        let message = WebSocketStreamMessage {
            channel: channel.clone(),
            message_type: "position".to_string(),
            data: serde_json::to_value(&position)?,
            timestamp: position.timestamp,
            sequence: None,
        };
        
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.position_sender.send(position);
        
        Ok(())
    }
    
    pub async fn publish_balance_update(&self, balance: BalanceData) -> Result<()> {
        let channel = format!("balances:{}", balance.user_id);
        
        let message = WebSocketStreamMessage {
            channel: channel.clone(),
            message_type: "balance".to_string(),
            data: serde_json::to_value(&balance)?,
            timestamp: balance.timestamp,
            sequence: None,
        };
        
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.balance_sender.send(balance);
        
        Ok(())
    }
    
    // Helper methods with backpressure handling
    async fn send_to_subscribers(&self, channel: &str, message: &WebSocketStreamMessage) {
        let subscriptions = self.subscriptions.read().await;
        let connections = self.connections.read().await;
        
        if let Some(subs) = subscriptions.get(channel) {
            let message_str = match serde_json::to_string(message) {
                Ok(msg) => msg,
                Err(e) => {
                    error!("Failed to serialize message for channel {}: {}", channel, e);
                    return;
                }
            };
            
            // Send to subscribers with backpressure control (sequential to avoid closure issues)
            for sub in subs {
                if let Some(sender) = connections.get(&sub.connection_id) {
                    self.send_with_backpressure(sender, message_str.clone(), &sub.connection_id).await;
                }
            }
            
            debug!("Processed message for {} subscribers on channel: {}", subs.len(), channel);
        }
    }
    
    fn is_valid_channel(&self, channel: &str, user_id: &Option<String>) -> bool {
        let parts: Vec<&str> = channel.split(':').collect();
        
        if parts.is_empty() {
            return false;
        }
        
        match parts[0] {
            "ticker" | "orderbook" | "trades" | "klines" => {
                // Market data channels: type:symbol:exchange[:options]
                parts.len() >= 3 && self.is_valid_symbol(parts[1])
            },
            "orders" | "positions" | "balances" | "alerts" => {
                // Account channels: type:user_id
                parts.len() >= 2 && user_id.is_some() && 
                user_id.as_ref().unwrap() == parts[1]
            },
            _ => false,
        }
    }
    
    fn is_valid_symbol(&self, symbol: &str) -> bool {
        // Basic symbol validation - should be BASE/QUOTE format
        symbol.contains('/') && symbol.len() >= 5
    }
    
    async fn start_market_data_stream(&self, channel: &str) -> Result<()> {
        info!("[PRODUCTION] Starting REAL market data stream for channel: {}", channel);
        
        let parts: Vec<&str> = channel.split(':').collect();
        if parts.len() < 3 {
            return Err(anyhow::anyhow!("Invalid channel format: {}", channel));
        }
        
        let stream_type = parts[0];
        let symbol = parts[1];
        let exchange = parts[2];
        
        // Create Arc<Self> for streaming methods
        let self_arc = Arc::new(self.clone());
        
        match exchange {
            // Note: Real exchange streaming is handled by dedicated WebSocket clients
            // See binance_websocket.rs for Binance implementation
            _ => {
                warn!("Real-time streaming for {} handled by dedicated WebSocket client", exchange);
                Ok(())
            }
        }
    }
    
    /// Start KuCoin real-time data stream
    async fn start_kucoin_stream(&self, stream_type: &str, symbol: &str) -> Result<()> {
        info!("[PRODUCTION] Starting KuCoin {} stream for {}", stream_type, symbol);
        
        // For now, this is a placeholder that logs the stream request
        // In a full implementation, this would:
        // 1. Connect to KuCoin WebSocket API
        // 2. Subscribe to the specific stream (ticker, trades, orderbook)
        // 3. Parse incoming data and publish via self.publish_*
        
        match stream_type {
            "ticker" => {
                info!("🎯 KuCoin ticker stream requested for {}", symbol);
                // Implement KuCoin ticker WebSocket subscription - see EXCHANGE_CLIENT_SPEC.md#kucoin-websocket
            }
            "trades" => {
                info!("📊 KuCoin trades stream requested for {}", symbol);
                // Implement KuCoin trades WebSocket subscription - see EXCHANGE_CLIENT_SPEC.md#kucoin-websocket
            }
            "orderbook" => {
                info!("📖 KuCoin orderbook stream requested for {}", symbol);
                // Implement KuCoin orderbook WebSocket subscription - see EXCHANGE_CLIENT_SPEC.md#kucoin-websocket
            }
            _ => {
                warn!("Unsupported KuCoin stream type: {}", stream_type);
            }
        }
        
        Ok(())
    }

    pub async fn get_subscription_stats(&self) -> serde_json::Value {
        let subscriptions = self.subscriptions.read().await;
        let connections = self.connections.read().await;
        
        let mut channel_counts: HashMap<String, usize> = HashMap::new();
        let mut total_subscriptions = 0;
        
        for (channel, subs) in subscriptions.iter() {
            let channel_type = channel.split(':').next().unwrap_or("unknown");
            *channel_counts.entry(channel_type.to_string()).or_insert(0) += subs.len();
            total_subscriptions += subs.len();
        }
        
        serde_json::json!({
            "totalConnections": connections.len(),
            "totalSubscriptions": total_subscriptions,
            "channelCounts": channel_counts,
            "activeChannels": subscriptions.len()
        })
    }

    /// Subscribe to all market data events
    pub async fn subscribe_all(&self) -> Result<broadcast::Receiver<StreamEvent>> {
        // Create a unified channel for all events
        let (tx, rx) = broadcast::channel(1000);
        
        // TODO: Implement forwarding from individual channels to unified channel
        // For now, just return the receiver
        Ok(rx)
    }
}

// Background task to simulate market data for testing
pub async fn simulate_market_data(streaming: Arc<StreamingManager>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(1000));
    
    loop {
        interval.tick().await;
        
        // Simulate ticker update
        let ticker = TickerData {
            symbol: "BTC/USDT".to_string(),
            exchange: "binance".to_string(),
            price: 100000.0 + (rand::random::<f64>() - 0.5) * 1000.0,
            bid: 99999.0,
            ask: 100001.0,
            volume_24h: 12345.67890000,
            change_24h: 5.1234,
            high_24h: 101000.00000000,
            low_24h: 99000.00000000,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };
        
        if let Err(e) = streaming.publish_ticker(ticker).await {
            error!("Failed to publish ticker: {}", e);
        }
        
        // Simulate trade update
        let trade = TradeData {
            symbol: "BTC/USDT".to_string(),
            exchange: "binance".to_string(),
            id: format!("trade_{}", Uuid::new_v4()),
            price: 100000.0 + (rand::random::<f64>() - 0.5) * 100.0,
            quantity: rand::random::<f64>() * 0.1,
            side: if rand::random::<bool>() { "buy" } else { "sell" }.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            is_maker: rand::random::<bool>(),
        };
        
        if let Err(e) = streaming.publish_trade(trade).await {
            error!("Failed to publish trade: {}", e);
        }
    }
}