//! KuCoin order book WebSocket stream implementation.
//!
//! High-performance order book management with <10ms update latency.

use super::types::*;
use crate::error::UnindexedClientError;
use futures_util::{SinkExt, StreamExt};
use jackbot_data::exchange::kucoin::rate_limit::KucoinRateLimit;
use jackbot_integration::{
    circuit_breaker::CircuitBreaker,
    error::SocketError,
    protocol::websocket::{connect, WebSocket},
    rate_limit::Priority,
};
use rust_decimal::Decimal;
use std::{collections::BTreeMap, str::FromStr, sync::Arc};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Order book price level.
#[derive(Debug, Clone)]
pub struct PriceLevel {
    pub price: Decimal,
    pub quantity: Decimal,
    pub sequence: i64,
}

/// KuCoin L2 order book snapshot and updates.
#[derive(Debug, Clone)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: BTreeMap<Decimal, PriceLevel>,
    pub asks: BTreeMap<Decimal, PriceLevel>,
    pub sequence: i64,
    pub last_update: chrono::DateTime<chrono::Utc>,
}

impl OrderBook {
    /// Create a new empty order book.
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            sequence: 0,
            last_update: chrono::Utc::now(),
        }
    }

    /// Apply L2 update to the order book.
    pub fn apply_update(&mut self, update: &KuCoinL2Update) -> Result<(), UnindexedClientError> {
        // Validate sequence
        if update.sequence_start <= self.sequence {
            debug!("Ignoring old update: {} <= {}", update.sequence_start, self.sequence);
            return Ok(());
        }

        // Apply bid updates
        for bid in &update.changes.bids {
            let price = Decimal::from_str(&bid[0])
                .map_err(|e| UnindexedClientError::Other(format!("Invalid bid price: {}", e)))?;
            let quantity = Decimal::from_str(&bid[1])
                .map_err(|e| UnindexedClientError::Other(format!("Invalid bid quantity: {}", e)))?;
            let sequence = bid[2].parse::<i64>()
                .map_err(|e| UnindexedClientError::Other(format!("Invalid bid sequence: {}", e)))?;

            if quantity.is_zero() {
                self.bids.remove(&price);
            } else {
                self.bids.insert(price, PriceLevel {
                    price,
                    quantity,
                    sequence,
                });
            }
        }

        // Apply ask updates
        for ask in &update.changes.asks {
            let price = Decimal::from_str(&ask[0])
                .map_err(|e| UnindexedClientError::Other(format!("Invalid ask price: {}", e)))?;
            let quantity = Decimal::from_str(&ask[1])
                .map_err(|e| UnindexedClientError::Other(format!("Invalid ask quantity: {}", e)))?;
            let sequence = ask[2].parse::<i64>()
                .map_err(|e| UnindexedClientError::Other(format!("Invalid ask sequence: {}", e)))?;

            if quantity.is_zero() {
                self.asks.remove(&price);
            } else {
                self.asks.insert(price, PriceLevel {
                    price,
                    quantity,
                    sequence,
                });
            }
        }

        self.sequence = update.sequence_end;
        self.last_update = chrono::Utc::now();

        Ok(())
    }

    /// Get the best bid price and quantity.
    pub fn best_bid(&self) -> Option<&PriceLevel> {
        self.bids.values().next_back()
    }

    /// Get the best ask price and quantity.
    pub fn best_ask(&self) -> Option<&PriceLevel> {
        self.asks.values().next()
    }

    /// Get the spread between best bid and ask.
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some(ask.price - bid.price),
            _ => None,
        }
    }

    /// Get top N levels from the order book.
    pub fn levels(&self, depth: usize) -> (Vec<&PriceLevel>, Vec<&PriceLevel>) {
        let bids: Vec<&PriceLevel> = self.bids.values().rev().take(depth).collect();
        let asks: Vec<&PriceLevel> = self.asks.values().take(depth).collect();
        (bids, asks)
    }
}

/// Create a KuCoin order book stream.
pub async fn create_orderbook_stream(
    config: &KuCoinConfig,
    symbol: String,
) -> Result<mpsc::UnboundedReceiver<OrderBook>, UnindexedClientError> {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut config = config.clone();
    let symbol_clone = symbol.clone();

    // Get WebSocket connection info for public streams
    let rest_client = super::rest::KuCoinRestClient::new(config.clone());
    let ws_info = rest_client.get_ws_connection_info().await
        .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

    // Use the first instance server
    let server = ws_info.instance_servers.first()
        .ok_or_else(|| UnindexedClientError::Other("No WebSocket servers available".to_string()))?;

    let ws_url = format!("{}?token={}", server.endpoint, ws_info.token);
    config.ws_url = Some(url::Url::parse(&ws_url)
        .map_err(|e| UnindexedClientError::Other(e.to_string()))?);

    let ping_interval_ms = server.ping_interval;

    tokio::spawn(async move {
        let mut breaker = CircuitBreaker::new(5, Duration::from_secs(5));
        let rate_limiter = KucoinRateLimit::new();
        let mut order_book = OrderBook::new(symbol_clone.clone());

        loop {
            if breaker.is_open() {
                if let Some(wait) = breaker.remaining() {
                    warn!(?wait, "Circuit breaker open, waiting before reconnect");
                    tokio::time::sleep(wait).await;
                    continue;
                }
            }

            rate_limiter.acquire_ws(Priority::Normal).await;

            match connect(config.ws_url.as_ref().unwrap().clone()).await {
                Ok(ws) => {
                    breaker.reset();
                    let result = run_orderbook_connection(
                        ws, 
                        &tx, 
                        &symbol_clone,
                        &mut order_book,
                        ping_interval_ms,
                    ).await;
                    if result.is_err() {
                        breaker.record_failure();
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    breaker.record_failure();
                    error!(?err, "Failed to connect to KuCoin WebSocket");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    });

    Ok(rx)
}

/// Run the order book WebSocket connection.
async fn run_orderbook_connection(
    mut ws: WebSocket,
    tx: &mpsc::UnboundedSender<OrderBook>,
    symbol: &str,
    order_book: &mut OrderBook,
    ping_interval_ms: i64,
) -> Result<(), ()> {
    // Subscribe to order book updates
    if let Err(e) = subscribe_orderbook(&mut ws, symbol).await {
        error!(?e, "Failed to subscribe to order book");
        return Err(());
    }

    // Start ping task
    let mut ping_interval = interval(Duration::from_millis(ping_interval_ms as u64));

    // Main message loop
    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                let ping_msg = serde_json::json!({
                    "id": Uuid::new_v4().to_string(),
                    "type": "ping"
                });
                
                if let Err(e) = ws.send(WsMessage::Text(ping_msg.to_string().into())).await {
                    error!(?e, "Failed to send ping");
                    return Err(());
                }
            }
            msg = ws.next() => {
                match msg {
                    Some(Ok(msg)) => {
                        if let Err(e) = handle_orderbook_message(msg, tx, order_book).await {
                            error!(?e, "Error handling WebSocket message");
                        }
                    }
                    Some(Err(e)) => {
                        error!(?e, "WebSocket error");
                        return Err(());
                    }
                    None => {
                        warn!("WebSocket connection closed");
                        return Err(());
                    }
                }
            }
        }
    }
}

/// Subscribe to KuCoin order book channel.
async fn subscribe_orderbook(
    ws: &mut WebSocket,
    symbol: &str,
) -> Result<(), SocketError> {
    // Subscribe to Level 2 order book updates
    let orderbook_sub = KuCoinWsSubscribe {
        id: Uuid::new_v4().to_string(),
        r#type: "subscribe".to_string(),
        topic: format!("/market/level2:{}", symbol),
        private_channel: false,
        response: true,
    };

    ws.send(WsMessage::Text(
        serde_json::to_string(&orderbook_sub).map_err(|e| SocketError::Other(e.to_string()))?.into(),
    ))
    .await
    .map_err(|e| SocketError::Other(e.to_string()))?;

    info!("📖 Subscribed to KuCoin L2 order book for {}", symbol);
    Ok(())
}

/// Handle incoming order book WebSocket messages.
async fn handle_orderbook_message(
    msg: WsMessage,
    tx: &mpsc::UnboundedSender<OrderBook>,
    order_book: &mut OrderBook,
) -> Result<(), Box<dyn std::error::Error>> {
    match msg {
        WsMessage::Text(text) => {
            let ws_msg: KuCoinWsMessage = serde_json::from_str(&text)?;

            match ws_msg.r#type.as_str() {
                "message" => {
                    if let Some(topic) = ws_msg.topic {
                        if let Some(data) = ws_msg.data {
                            if topic.starts_with("/market/level2:") {
                                handle_orderbook_update(data, tx, order_book)?;
                            }
                        }
                    }
                }
                "pong" => {
                    debug!("Received pong");
                }
                "welcome" => {
                    info!("📡 KuCoin WebSocket connection established");
                }
                "ack" => {
                    if let Some(id) = ws_msg.id {
                        debug!("Subscription acknowledged: {}", id);
                    }
                }
                "error" => {
                    error!("WebSocket error: {:?}", ws_msg.data);
                }
                _ => {
                    debug!("Unhandled message type: {}", ws_msg.r#type);
                }
            }
        }
        WsMessage::Ping(_) => {
            debug!("Received ping");
        }
        WsMessage::Pong(_) => {
            // Pong received
        }
        WsMessage::Close(_) => {
            warn!("Received close frame from server");
            return Err("Connection closed".into());
        }
        _ => {}
    }

    Ok(())
}

/// Handle order book updates.
fn handle_orderbook_update(
    data: serde_json::Value,
    tx: &mpsc::UnboundedSender<OrderBook>,
    order_book: &mut OrderBook,
) -> Result<(), Box<dyn std::error::Error>> {
    let update: KuCoinL2Update = serde_json::from_value(data)?;
    
    // Apply the update to the order book
    if let Err(e) = order_book.apply_update(&update) {
        error!("Failed to apply order book update: {}", e);
        return Err(e.into());
    }

    // Send the updated order book
    let _ = tx.send(order_book.clone());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_book_operations() {
        let mut ob = OrderBook::new("BTC-USDT".to_string());
        
        // Test empty order book
        assert!(ob.best_bid().is_none());
        assert!(ob.best_ask().is_none());
        assert!(ob.spread().is_none());

        // Add some levels manually for testing
        ob.bids.insert(
            Decimal::from_str("50000.0").unwrap(),
            PriceLevel {
                price: Decimal::from_str("50000.0").unwrap(),
                quantity: Decimal::from_str("1.5").unwrap(),
                sequence: 1,
            }
        );

        ob.asks.insert(
            Decimal::from_str("50100.0").unwrap(),
            PriceLevel {
                price: Decimal::from_str("50100.0").unwrap(),
                quantity: Decimal::from_str("2.0").unwrap(),
                sequence: 2,
            }
        );

        // Test best prices
        assert_eq!(ob.best_bid().unwrap().price, Decimal::from_str("50000.0").unwrap());
        assert_eq!(ob.best_ask().unwrap().price, Decimal::from_str("50100.0").unwrap());
        assert_eq!(ob.spread().unwrap(), Decimal::from_str("100.0").unwrap());

        // Test levels
        let (bids, asks) = ob.levels(5);
        assert_eq!(bids.len(), 1);
        assert_eq!(asks.len(), 1);
    }
}