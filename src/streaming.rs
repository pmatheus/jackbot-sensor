use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, broadcast};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::api::{TickerData, OrderBookData, TradeData, KlineData, PositionData, BalanceData, OrderResponse};

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

pub struct StreamingManager {
    subscriptions: Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
    connections: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<String>>>>,
    ticker_sender: broadcast::Sender<TickerData>,
    orderbook_sender: broadcast::Sender<OrderBookData>,
    trade_sender: broadcast::Sender<TradeData>,
    kline_sender: broadcast::Sender<KlineData>,
    order_sender: broadcast::Sender<OrderResponse>,
    position_sender: broadcast::Sender<PositionData>,
    balance_sender: broadcast::Sender<BalanceData>,
}

impl StreamingManager {
    pub fn new() -> Self {
        let (ticker_sender, _) = broadcast::channel(1000);
        let (orderbook_sender, _) = broadcast::channel(1000);
        let (trade_sender, _) = broadcast::channel(1000);
        let (kline_sender, _) = broadcast::channel(1000);
        let (order_sender, _) = broadcast::channel(1000);
        let (position_sender, _) = broadcast::channel(1000);
        let (balance_sender, _) = broadcast::channel(1000);
        
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
        }
    }
    
    pub async fn add_connection(
        &self,
        connection_id: String,
        sender: mpsc::UnboundedSender<String>,
    ) {
        self.connections.write().await.insert(connection_id.clone(), sender);
        info!("Added WebSocket connection: {}", connection_id);
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
        
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.ticker_sender.send(ticker);
        
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
        
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.orderbook_sender.send(orderbook);
        
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
        
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.trade_sender.send(trade);
        
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
        
        self.send_to_subscribers(&channel, &message).await;
        let _ = self.kline_sender.send(kline);
        
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
    
    // Helper methods
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
            
            for sub in subs {
                if let Some(sender) = connections.get(&sub.connection_id) {
                    if let Err(e) = sender.send(message_str.clone()) {
                        warn!("Failed to send message to connection {}: {}", sub.connection_id, e);
                    }
                }
            }
            
            debug!("Sent message to {} subscribers for channel: {}", subs.len(), channel);
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
        // TODO: Start actual market data stream from exchange
        // This would connect to the exchange WebSocket and start streaming data
        debug!("Starting market data stream for channel: {}", channel);
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