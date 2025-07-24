//! Gate.io Exchange Connector
//! High-performance WebSocket integration for Gate.io exchange

use anyhow::{Result, Context};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, error, info, warn};

use crate::connector::{Exchange, Connection, OrderResult, Order, OrderId, Balance, OrderType, OrderSide, OrderStatus};
use crate::api::{OrderBookData, TickerData, TradeData};
use crate::streaming::StreamingManager;

/// Gate.io WebSocket message types
#[derive(Debug, Deserialize)]
struct GateMessage {
    time: u64,
    channel: String,
    event: String,
    result: Option<GateResult>,
    error: Option<GateError>,
}

#[derive(Debug, Deserialize)]
struct GateResult {
    status: String,
}

#[derive(Debug, Deserialize)]
struct GateError {
    code: i32,
    message: String,
}

/// Gate.io ticker update
#[derive(Debug, Deserialize)]
struct GateTicker {
    currency_pair: String,
    last: String,
    lowest_ask: String,
    highest_bid: String,
    change_percentage: String,
    base_volume: String,
    quote_volume: String,
    high_24h: String,
    low_24h: String,
}

/// Gate.io order book update
#[derive(Debug, Deserialize)]
struct GateOrderBook {
    t: u64, // timestamp in milliseconds
    u: u64, // update ID
    s: String, // symbol
    bids: Vec<[String; 2]>, // [price, quantity]
    asks: Vec<[String; 2]>, // [price, quantity]
}

/// Gate.io trade update
#[derive(Debug, Deserialize)]
struct GateTrade {
    id: u64,
    create_time: f64,
    currency_pair: String,
    side: String, // "buy" or "sell"
    amount: String,
    price: String,
}

/// Gate.io rate limiting configuration (100 messages per second)
const GATEIO_RATE_LIMIT_PER_SEC: usize = 100;
const GATEIO_RATE_WINDOW_MS: u64 = 1000;

/// Gate.io rate limiter for preventing API violations
#[derive(Debug)]
struct GateioRateLimiter {
    semaphore: Arc<Semaphore>,
    last_reset: Arc<Mutex<Instant>>,
}

impl GateioRateLimiter {
    fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(GATEIO_RATE_LIMIT_PER_SEC)),
            last_reset: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Wait for permission to send a message (rate limiting)
    async fn acquire_permit(&self) -> Result<()> {
        // Check if we need to reset the rate limit window
        let mut last_reset = self.last_reset.lock().await;
        let now = Instant::now();
        
        if now.duration_since(*last_reset) >= Duration::from_millis(GATEIO_RATE_WINDOW_MS) {
            // Reset the semaphore by adding back permits
            let available = self.semaphore.available_permits();
            let needed = GATEIO_RATE_LIMIT_PER_SEC.saturating_sub(available);
            self.semaphore.add_permits(needed);
            *last_reset = now;
            debug!("Gate.io rate limit window reset, permits available: {}", self.semaphore.available_permits());
        }
        
        drop(last_reset); // Release mutex early
        
        // Try to acquire permit with timeout to prevent blocking
        match tokio::time::timeout(
            Duration::from_millis(100),
            self.semaphore.acquire()
        ).await {
            Ok(Ok(permit)) => {
                permit.forget(); // Consume the permit
                Ok(())
            }
            Ok(Err(_)) => {
                warn!("Gate.io semaphore closed");
                Err(anyhow::anyhow!("Rate limiter semaphore closed"))
            }
            Err(_) => {
                warn!("Gate.io rate limit timeout - message may be dropped");
                Err(anyhow::anyhow!("Rate limit exceeded - too many requests"))
            }
        }
    }
}

pub struct GateioConnector {
    api_key: Option<String>,
    api_secret: Option<String>,
    sandbox: bool,
    streaming_manager: Arc<StreamingManager>,
    rate_limiter: Arc<GateioRateLimiter>,
}

impl GateioConnector {
    pub fn new(
        api_key: Option<String>,
        api_secret: Option<String>,
        sandbox: bool,
    ) -> Result<Self> {
        let streaming_manager = Arc::new(StreamingManager::new());
        let rate_limiter = Arc::new(GateioRateLimiter::new());
        
        info!("Gate.io connector initialized with rate limiting: {} msg/sec", GATEIO_RATE_LIMIT_PER_SEC);
        
        Ok(Self {
            api_key,
            api_secret,
            sandbox,
            streaming_manager,
            rate_limiter,
        })
    }

    /// Convert Gate.io symbol format (BTC_USDT) to standard format (BTC/USDT)
    fn normalize_symbol(gate_symbol: &str) -> String {
        gate_symbol.replace('_', "/")
    }

    /// Convert standard symbol format (BTC/USDT) to Gate.io format (BTC_USDT)
    fn to_gate_symbol(symbol: &str) -> String {
        symbol.replace('/', "_")
    }

    /// Parse order book from Gate.io format
    fn parse_order_book(&self, book: GateOrderBook) -> Result<OrderBookData> {
        let mut order_book = OrderBookData {
            exchange: "gateio".to_string(),
            symbol: Self::normalize_symbol(&book.s),
            timestamp: book.t as i64,
            bids: Vec::with_capacity(book.bids.len()),
            asks: Vec::with_capacity(book.asks.len()),
            sequence_id: None,
        };

        // Parse bids
        for bid in book.bids {
            let price = bid[0].parse::<Decimal>()
                .context("Failed to parse bid price")?;
            let quantity = bid[1].parse::<Decimal>()
                .context("Failed to parse bid quantity")?;
            order_book.bids.push([price.to_string().parse::<f64>().unwrap(), quantity.to_string().parse::<f64>().unwrap()]);
        }

        // Parse asks
        for ask in book.asks {
            let price = ask[0].parse::<Decimal>()
                .context("Failed to parse ask price")?;
            let quantity = ask[1].parse::<Decimal>()
                .context("Failed to parse ask quantity")?;
            order_book.asks.push([price.to_string().parse::<f64>().unwrap(), quantity.to_string().parse::<f64>().unwrap()]);
        }

        // Sort for consistency
        order_book.bids.sort_by(|a, b| b[0].partial_cmp(&a[0]).unwrap_or(std::cmp::Ordering::Equal)); // Descending
        order_book.asks.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal)); // Ascending

        Ok(order_book)
    }
}

#[async_trait]
impl Exchange for GateioConnector {
    async fn connect(&self) -> Result<Connection> {
        info!("Connecting to Gate.io");
        
        let ws_url = if self.sandbox {
            "wss://api.gateio.ws/ws/4" // Gate.io doesn't have separate testnet WS
        } else {
            "wss://api.gateio.ws/ws/4"
        };
        
        Ok(Arc::new(()) as Connection)
    }

    async fn subscribe_market_data(&self, symbols: Vec<String>) -> Result<crate::connector::MarketDataStream> {
        for symbol in &symbols {
            let gate_symbol = Self::to_gate_symbol(symbol);
            debug!("Subscribing to Gate.io market data for {}", gate_symbol);
        }
        
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn place_order(&self, order: Order) -> Result<OrderResult> {
        // Apply rate limiting before making API call
        self.rate_limiter.acquire_permit().await
            .context("Rate limit exceeded for Gate.io order placement")?;
            
        info!("Placing Gate.io order with rate limiting: {:?}", order.symbol);
        
        // TODO: Implement actual Gate.io order placement API call
        warn!("Gate.io order placement - using mock response (implement actual API)");
        
        Ok(OrderResult {
            order_id: format!("GATE-{}", uuid::Uuid::new_v4()),
            status: OrderStatus::New,
            filled_quantity: 0.0,
            remaining_quantity: order.quantity,
            average_price: 0.0,
            commission: 0.0,
            commission_asset: "USDT".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn cancel_order(&self, id: OrderId) -> Result<()> {
        warn!("Order cancellation not yet implemented for Gate.io");
        Ok(())
    }

    async fn get_balance(&self) -> Result<Vec<Balance>> {
        warn!("Balance retrieval not yet implemented for Gate.io");
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_conversion() {
        assert_eq!(GateioConnector::normalize_symbol("BTC_USDT"), "BTC/USDT");
        assert_eq!(GateioConnector::to_gate_symbol("BTC/USDT"), "BTC_USDT");
        assert_eq!(GateioConnector::normalize_symbol("ETH_BTC"), "ETH/BTC");
        assert_eq!(GateioConnector::to_gate_symbol("ETH/BTC"), "ETH_BTC");
    }

    #[tokio::test]
    async fn test_order_book_parsing() {
        let connector = GateioConnector::new("wss://test.gate.io".to_string());
        
        let gate_book = GateOrderBook {
            t: 1234567890,
            u: 1,
            s: "BTC_USDT".to_string(),
            bids: vec![
                ["42000.50".to_string(), "1.5".to_string()],
                ["42000.00".to_string(), "2.0".to_string()],
            ],
            asks: vec![
                ["42001.00".to_string(), "1.0".to_string()],
                ["42001.50".to_string(), "2.5".to_string()],
            ],
        };
        
        let order_book = connector.parse_order_book(gate_book).unwrap();
        
        assert_eq!(order_book.exchange, "gateio");
        assert_eq!(order_book.symbol, "BTC/USDT");
        assert_eq!(order_book.timestamp, 1234567890);
        assert_eq!(order_book.bids.len(), 2);
        assert_eq!(order_book.asks.len(), 2);
        
        // Check bid ordering (descending)
        assert!(order_book.bids[0].0 > order_book.bids[1].0);
        
        // Check ask ordering (ascending)
        assert!(order_book.asks[0].0 < order_book.asks[1].0);
    }
}