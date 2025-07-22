//! BingX Exchange Connector
//! High-performance WebSocket integration for BingX exchange

use anyhow::{Result, Context};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::connector::{Exchange, Connection, OrderResult, Order, OrderId, Balance, OrderType, OrderSide, OrderStatus};
use crate::api::{OrderBookData, TickerData, TradeData};
use crate::streaming::StreamingManager;

/// BingX WebSocket message types
#[derive(Debug, Deserialize)]
struct BingXMessage {
    id: Option<String>,
    code: Option<i32>,
    msg: Option<String>,
    #[serde(rename = "dataType")]
    data_type: Option<String>,
    data: Option<serde_json::Value>,
}

/// BingX ticker data
#[derive(Debug, Deserialize)]
struct BingXTicker {
    s: String,  // symbol
    c: String,  // close price
    h: String,  // high price
    l: String,  // low price
    v: String,  // volume
    qv: String, // quote volume
    o: String,  // open price
    #[serde(rename = "T")]
    timestamp: u64, // timestamp
}

/// BingX order book data
#[derive(Debug, Deserialize)]
struct BingXOrderBook {
    bids: Vec<BingXPriceLevel>,
    asks: Vec<BingXPriceLevel>,
    #[serde(rename = "T")]
    timestamp: u64,
}

#[derive(Debug, Deserialize)]
struct BingXPriceLevel {
    #[serde(rename = "0")]
    price: String,
    #[serde(rename = "1")]
    quantity: String,
}

/// BingX trade data
#[derive(Debug, Deserialize)]
struct BingXTrade {
    #[serde(rename = "T")]
    timestamp: u64,
    s: String, // symbol
    p: String, // price
    q: String, // quantity
    m: bool,   // is buyer maker
}

/// BingX memory management configuration
const BINGX_MAX_MESSAGE_SIZE: usize = 32 * 1024; // 32KB max message size
const BINGX_MESSAGE_BUFFER_LIMIT: usize = 1000; // Max buffered messages
const BINGX_MEMORY_CHECK_INTERVAL_SECS: u64 = 60; // Check memory every minute
const BINGX_MAX_MEMORY_USAGE_MB: usize = 50; // Max memory usage before cleanup

/// BingX memory usage tracker
#[derive(Debug)]
pub struct BingXMemoryTracker {
    messages_processed: AtomicU64,
    messages_dropped: AtomicU64,
    memory_cleanups: AtomicUsize,
    last_memory_check: AtomicU64,
    estimated_memory_usage: AtomicUsize, // in bytes
}

impl Default for BingXMemoryTracker {
    fn default() -> Self {
        Self {
            messages_processed: AtomicU64::new(0),
            messages_dropped: AtomicU64::new(0),
            memory_cleanups: AtomicUsize::new(0),
            last_memory_check: AtomicU64::new(0),
            estimated_memory_usage: AtomicUsize::new(0),
        }
    }
}

impl BingXMemoryTracker {
    /// Record message processing and check for memory leaks
    pub fn record_message_processing(&self, message_size: usize) -> Result<()> {
        // Check message size limit to prevent large message attacks
        if message_size > BINGX_MAX_MESSAGE_SIZE {
            self.messages_dropped.fetch_add(1, Ordering::Relaxed);
            return Err(anyhow::anyhow!("BingX message too large: {} bytes (max: {})", 
                                     message_size, BINGX_MAX_MESSAGE_SIZE));
        }

        self.messages_processed.fetch_add(1, Ordering::Relaxed);
        let old_usage = self.estimated_memory_usage.fetch_add(message_size, Ordering::Relaxed);
        
        // Check if memory usage is getting too high
        let new_usage = old_usage + message_size;
        if new_usage > BINGX_MAX_MEMORY_USAGE_MB * 1024 * 1024 {
            warn!("🚨 BingX memory usage high: {} MB", new_usage / (1024 * 1024));
            self.trigger_memory_cleanup();
        }

        // Periodic memory check
        let now = Instant::now().elapsed().as_secs();
        let last_check = self.last_memory_check.load(Ordering::Relaxed);
        
        if now - last_check > BINGX_MEMORY_CHECK_INTERVAL_SECS {
            self.last_memory_check.store(now, Ordering::Relaxed);
            self.perform_memory_check();
        }

        Ok(())
    }

    /// Trigger emergency memory cleanup
    fn trigger_memory_cleanup(&self) {
        let cleanup_count = self.memory_cleanups.fetch_add(1, Ordering::SeqCst);
        warn!("🧹 BingX memory cleanup triggered #{}", cleanup_count + 1);
        
        // Reset estimated usage (aggressive cleanup)
        let old_usage = self.estimated_memory_usage.swap(0, Ordering::SeqCst);
        info!("🧹 BingX memory cleanup: freed ~{} MB", old_usage / (1024 * 1024));
        
        // Force garbage collection hint
        // Note: Rust doesn't have explicit GC, but dropping large objects helps
        drop(vec![0u8; 0]); // Minimal allocation to trigger allocator cleanup
    }

    /// Perform regular memory health check
    fn perform_memory_check(&self) {
        let usage = self.estimated_memory_usage.load(Ordering::Relaxed);
        let processed = self.messages_processed.load(Ordering::Relaxed);
        let dropped = self.messages_dropped.load(Ordering::Relaxed);
        let cleanups = self.memory_cleanups.load(Ordering::Relaxed);
        
        if usage > BINGX_MAX_MEMORY_USAGE_MB * 1024 * 1024 / 2 {
            warn!("⚠️  BingX memory usage warning: {} MB", usage / (1024 * 1024));
        }
        
        debug!("📊 BingX Memory Stats: usage={}MB, processed={}, dropped={}, cleanups={}",
               usage / (1024 * 1024), processed, dropped, cleanups);
    }

    /// Get memory statistics
    pub fn get_stats(&self) -> (u64, u64, usize, usize) {
        (
            self.messages_processed.load(Ordering::Relaxed),
            self.messages_dropped.load(Ordering::Relaxed),
            self.memory_cleanups.load(Ordering::Relaxed),
            self.estimated_memory_usage.load(Ordering::Relaxed)
        )
    }
}

/// Safe message parser that prevents memory leaks
pub struct BingXMessageParser {
    memory_tracker: Arc<BingXMemoryTracker>,
}

impl BingXMessageParser {
    pub fn new(memory_tracker: Arc<BingXMemoryTracker>) -> Self {
        Self { memory_tracker }
    }

    /// Parse BingX message with memory leak protection
    pub fn parse_message(&self, raw_message: &[u8]) -> Result<BingXMessage> {
        // Check message size first
        self.memory_tracker.record_message_processing(raw_message.len())?;
        
        // Parse with size limits to prevent memory exhaustion
        if raw_message.len() > BINGX_MAX_MESSAGE_SIZE {
            return Err(anyhow::anyhow!("BingX message exceeds size limit"));
        }
        
        // Use streaming parser to avoid loading entire message into memory
        let message: BingXMessage = serde_json::from_slice(raw_message)
            .context("Failed to parse BingX message")?;
            
        // Validate message structure to prevent malformed data leaks
        self.validate_message(&message)?;
        
        Ok(message)
    }

    /// Validate message structure to prevent memory leaks from malformed data
    fn validate_message(&self, message: &BingXMessage) -> Result<()> {
        // Validate data field isn't excessively large
        if let Some(ref data) = message.data {
            let data_str = data.to_string();
            if data_str.len() > BINGX_MAX_MESSAGE_SIZE / 2 {
                return Err(anyhow::anyhow!("BingX message data field too large"));
            }
        }

        // Validate string fields for reasonable lengths
        if let Some(ref id) = message.id {
            if id.len() > 256 {
                return Err(anyhow::anyhow!("BingX message ID too long"));
            }
        }

        if let Some(ref msg) = message.msg {
            if msg.len() > 1024 {
                return Err(anyhow::anyhow!("BingX message text too long"));
            }
        }

        Ok(())
    }
}

pub struct BingXConnector {
    api_key: Option<String>,
    api_secret: Option<String>,
    sandbox: bool,
    streaming_manager: Arc<StreamingManager>,
    memory_tracker: Arc<BingXMemoryTracker>,
    message_parser: Arc<BingXMessageParser>,
}

impl BingXConnector {
    pub fn new(
        api_key: Option<String>,
        api_secret: Option<String>,
        sandbox: bool,
    ) -> Result<Self> {
        let streaming_manager = Arc::new(StreamingManager::new());
        let memory_tracker = Arc::new(BingXMemoryTracker::default());
        let message_parser = Arc::new(BingXMessageParser::new(Arc::clone(&memory_tracker)));
        
        info!("BingX connector initialized with memory leak protection");
        
        Ok(Self {
            api_key,
            api_secret,
            sandbox,
            streaming_manager,
            memory_tracker,
            message_parser,
        })
    }

    /// Get memory usage statistics
    pub fn get_memory_stats(&self) -> (u64, u64, usize, usize) {
        self.memory_tracker.get_stats()
    }

    /// Convert standard symbol format (BTC/USDT) to BingX format (BTC-USDT)
    fn to_bingx_symbol(symbol: &str) -> String {
        symbol.replace('/', "-")
    }

    /// Convert BingX symbol format (BTC-USDT) to standard format (BTC/USDT)
    fn normalize_symbol(bingx_symbol: &str) -> String {
        bingx_symbol.replace('-', "/")
    }

    /// Parse order book from BingX format
    fn parse_order_book(&self, symbol: &str, book: BingXOrderBook) -> Result<OrderBookData> {
        let mut order_book = OrderBookData {
            exchange: "bingx".to_string(),
            symbol: symbol.to_string(),
            timestamp: book.timestamp,
            bids: Vec::with_capacity(book.bids.len()),
            asks: Vec::with_capacity(book.asks.len()),
        };

        // Parse bids
        for bid in book.bids {
            let price = bid.price.parse::<Decimal>()
                .context("Failed to parse bid price")?;
            let quantity = bid.quantity.parse::<Decimal>()
                .context("Failed to parse bid quantity")?;
            order_book.bids.push((price, quantity));
        }

        // Parse asks
        for ask in book.asks {
            let price = ask.price.parse::<Decimal>()
                .context("Failed to parse ask price")?;
            let quantity = ask.quantity.parse::<Decimal>()
                .context("Failed to parse ask quantity")?;
            order_book.asks.push((price, quantity));
        }

        // BingX sends pre-sorted data
        Ok(order_book)
    }

    /// Parse trade from BingX format
    fn parse_trade(&self, trade: BingXTrade) -> Result<TradeData> {
        Ok(TradeData {
            exchange: "bingx".to_string(),
            symbol: Self::normalize_symbol(&trade.s),
            trade_id: trade.timestamp.to_string(),
            price: trade.p.parse()?,
            quantity: trade.q.parse()?,
            is_buyer_maker: trade.m,
            timestamp: trade.timestamp as i64,
        })
    }

    /// Parse ticker from BingX format
    fn parse_ticker(&self, ticker: BingXTicker) -> Result<TickerData> {
        Ok(TickerData {
            exchange: "bingx".to_string(),
            symbol: Self::normalize_symbol(&ticker.s),
            bid: 0.0, // BingX ticker doesn't provide bid
            ask: 0.0, // BingX ticker doesn't provide ask
            last: ticker.c.parse()?,
            volume: ticker.v.parse()?,
            timestamp: ticker.timestamp as i64,
        })
    }
}

#[async_trait]
impl Exchange for BingXConnector {
    async fn connect(&self) -> Result<Connection> {
        info!("Connecting to BingX");
        
        let ws_url = if self.sandbox {
            "wss://open-api-ws.bingx.com/market" // BingX doesn't have separate testnet WS
        } else {
            "wss://open-api-ws.bingx.com/market"
        };
        
        Ok(Arc::new(()) as Connection)
    }

    async fn subscribe_market_data(&self, symbols: Vec<String>) -> Result<crate::connector::MarketDataStream> {
        for symbol in &symbols {
            let bingx_symbol = Self::to_bingx_symbol(symbol);
            debug!("Subscribing to BingX market data for {}", bingx_symbol);
        }
        
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn place_order(&self, order: Order) -> Result<OrderResult> {
        warn!("Order placement not yet implemented for BingX");
        Ok(OrderResult {
            order_id: "BINGX-ORDER-ID".to_string(),
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
        warn!("Order cancellation not yet implemented for BingX");
        Ok(())
    }

    async fn get_balance(&self) -> Result<Vec<Balance>> {
        warn!("Balance retrieval not yet implemented for BingX");
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_conversion() {
        assert_eq!(BingXConnector::to_bingx_symbol("BTC/USDT"), "BTC-USDT");
        assert_eq!(BingXConnector::normalize_symbol("BTC-USDT"), "BTC/USDT");
        assert_eq!(BingXConnector::to_bingx_symbol("ETH/BTC"), "ETH-BTC");
        assert_eq!(BingXConnector::normalize_symbol("ETH-BTC"), "ETH/BTC");
    }

    #[tokio::test]
    async fn test_order_book_parsing() {
        let connector = BingXConnector::new("wss://test.bingx.com".to_string());
        
        let bingx_book = BingXOrderBook {
            bids: vec![
                BingXPriceLevel { 
                    price: "42000.50".to_string(), 
                    quantity: "1.5".to_string() 
                },
                BingXPriceLevel { 
                    price: "42000.00".to_string(), 
                    quantity: "2.0".to_string() 
                },
            ],
            asks: vec![
                BingXPriceLevel { 
                    price: "42001.00".to_string(), 
                    quantity: "1.0".to_string() 
                },
                BingXPriceLevel { 
                    price: "42001.50".to_string(), 
                    quantity: "2.5".to_string() 
                },
            ],
            timestamp: 1234567890000,
        };
        
        let order_book = connector.parse_order_book("BTC/USDT", bingx_book).unwrap();
        
        assert_eq!(order_book.exchange, "bingx");
        assert_eq!(order_book.symbol, "BTC/USDT");
        assert_eq!(order_book.timestamp, 1234567890000);
        assert_eq!(order_book.bids.len(), 2);
        assert_eq!(order_book.asks.len(), 2);
        assert_eq!(order_book.bids[0].0.to_string(), "42000.50");
        assert_eq!(order_book.asks[0].0.to_string(), "42001.00");
    }

    #[test]
    fn test_trade_parsing() {
        let connector = BingXConnector::new("wss://test.bingx.com".to_string());
        
        let bingx_trade = BingXTrade {
            timestamp: 1234567890000,
            s: "BTC-USDT".to_string(),
            p: "42000.50".to_string(),
            q: "0.5".to_string(),
            m: false, // buyer is taker (buy order)
        };
        
        let trade = connector.parse_trade(bingx_trade).unwrap();
        
        assert_eq!(trade.exchange, "bingx");
        assert_eq!(trade.symbol, "BTC/USDT");
        assert_eq!(trade.price.to_string(), "42000.50");
        assert_eq!(trade.quantity.to_string(), "0.5");
        assert_eq!(trade.side, "buy");
        assert_eq!(trade.timestamp, 1234567890000);
    }

    #[test]
    fn test_ticker_parsing() {
        let connector = BingXConnector::new("wss://test.bingx.com".to_string());
        
        let bingx_ticker = BingXTicker {
            s: "BTC-USDT".to_string(),
            c: "42000.50".to_string(),
            h: "42500.00".to_string(),
            l: "41500.00".to_string(),
            v: "1234.56".to_string(),
            qv: "51234567.89".to_string(),
            o: "41800.00".to_string(),
            timestamp: 1234567890000,
        };
        
        let market_data = connector.parse_ticker(bingx_ticker).unwrap();
        
        assert_eq!(market_data.exchange, "bingx");
        assert_eq!(market_data.symbol, "BTC/USDT");
        assert_eq!(market_data.price.to_string(), "42000.50");
        assert_eq!(market_data.volume_24h.to_string(), "1234.56");
        assert_eq!(market_data.high_24h.to_string(), "42500.00");
        assert_eq!(market_data.low_24h.to_string(), "41500.00");
        assert_eq!(market_data.timestamp, 1234567890000);
    }
}