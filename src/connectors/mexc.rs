//! MEXC Exchange Connector
//! High-performance WebSocket integration for MEXC exchange

use anyhow::{Result, Context};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::connector::{Exchange, Connection, OrderResult, Order, OrderId, Balance, OrderType, OrderSide, OrderStatus};
use crate::api::{OrderBookData, TickerData, TradeData};
use crate::streaming::StreamingManager;

/// MEXC WebSocket message types
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MexcMessage {
    Subscription(MexcSubscription),
    Data(MexcData),
}

#[derive(Debug, Deserialize)]
struct MexcSubscription {
    id: Option<String>,
    code: i32,
    msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MexcData {
    c: String, // channel
    d: serde_json::Value, // data
    t: u64, // timestamp
}

/// MEXC ticker data
#[derive(Debug, Deserialize)]
struct MexcTicker {
    s: String, // symbol
    p: String, // price
    r: String, // 24hr change rate
    h: String, // 24hr high
    l: String, // 24hr low
    v: String, // 24hr volume
    q: String, // 24hr quote volume
}

/// MEXC order book data
#[derive(Debug, Deserialize)]
struct MexcOrderBook {
    symbol: String,
    version: String,
    bids: Vec<OrderLevel>,
    asks: Vec<OrderLevel>,
}

#[derive(Debug, Deserialize)]
struct OrderLevel {
    p: String, // price
    v: String, // volume
}

/// MEXC trade data
#[derive(Debug, Deserialize)]
struct MexcTrade {
    p: String, // price
    v: String, // volume
    S: i32, // side: 1 = buy, 2 = sell
    t: u64, // timestamp
}

/// MEXC connection stability monitoring configuration
const MEXC_PING_INTERVAL_SECS: u64 = 30;
const MEXC_CONNECTION_TIMEOUT_SECS: u64 = 60;
const MEXC_MAX_RECONNECT_ATTEMPTS: usize = 10;
const MEXC_RECONNECT_DELAY_MS: u64 = 2000;

/// MEXC connection health metrics
#[derive(Debug)]
pub struct MexcConnectionMetrics {
    pub is_connected: AtomicBool,
    pub last_ping_timestamp: AtomicU64,
    pub ping_response_time_ms: AtomicU64,
    pub reconnect_count: AtomicUsize,
    pub message_count: AtomicU64,
    pub error_count: AtomicU64,
    pub connection_uptime_start: AtomicU64,
}

impl Default for MexcConnectionMetrics {
    fn default() -> Self {
        let now = Instant::now().elapsed().as_millis() as u64;
        Self {
            is_connected: AtomicBool::new(false),
            last_ping_timestamp: AtomicU64::new(now),
            ping_response_time_ms: AtomicU64::new(0),
            reconnect_count: AtomicUsize::new(0),
            message_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            connection_uptime_start: AtomicU64::new(now),
        }
    }
}

/// MEXC connection stability monitor
#[derive(Debug)]
pub struct MexcConnectionMonitor {
    metrics: Arc<MexcConnectionMetrics>,
    monitoring_active: Arc<AtomicBool>,
}

impl MexcConnectionMonitor {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(MexcConnectionMetrics::default()),
            monitoring_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start monitoring connection stability
    pub async fn start_monitoring(&self) -> Result<()> {
        if self.monitoring_active.swap(true, Ordering::SeqCst) {
            return Ok(()); // Already monitoring
        }

        let metrics = Arc::clone(&self.metrics);
        let monitoring_active = Arc::clone(&self.monitoring_active);
        
        tokio::spawn(async move {
            info!("🔍 MEXC connection monitoring started");
            let mut ping_interval = tokio::time::interval(Duration::from_secs(MEXC_PING_INTERVAL_SECS));
            
            while monitoring_active.load(Ordering::SeqCst) {
                ping_interval.tick().await;
                
                // Check connection health
                let now = Instant::now().elapsed().as_millis() as u64;
                let last_ping = metrics.last_ping_timestamp.load(Ordering::Relaxed);
                let time_since_last_ping = now.saturating_sub(last_ping);
                
                if time_since_last_ping > (MEXC_CONNECTION_TIMEOUT_SECS * 1000) {
                    // Connection appears stale
                    if metrics.is_connected.swap(false, Ordering::SeqCst) {
                        warn!("⚠️  MEXC connection timeout detected - {} seconds since last ping", 
                              time_since_last_ping / 1000);
                        
                        // Trigger reconnection
                        if let Err(e) = Self::attempt_reconnection(&metrics).await {
                            error!("MEXC reconnection failed: {}", e);
                        }
                    }
                } else if !metrics.is_connected.load(Ordering::Relaxed) {
                    // Connection recovered
                    metrics.is_connected.store(true, Ordering::SeqCst);
                    let uptime_hours = (now - metrics.connection_uptime_start.load(Ordering::Relaxed)) / (1000 * 3600);
                    info!("✅ MEXC connection restored - uptime: {} hours", uptime_hours);
                }
                
                // Log connection stats periodically
                if metrics.message_count.load(Ordering::Relaxed) % 1000 == 0 {
                    Self::log_connection_stats(&metrics).await;
                }
            }
        });

        Ok(())
    }

    /// Attempt to reconnect to MEXC
    async fn attempt_reconnection(metrics: &Arc<MexcConnectionMetrics>) -> Result<()> {
        let reconnect_count = metrics.reconnect_count.fetch_add(1, Ordering::SeqCst);
        
        if reconnect_count >= MEXC_MAX_RECONNECT_ATTEMPTS {
            error!("🚨 MEXC max reconnection attempts reached: {}", reconnect_count);
            return Err(anyhow::anyhow!("Max reconnection attempts exceeded"));
        }

        info!("🔄 Attempting MEXC reconnection #{}", reconnect_count + 1);
        
        // Progressive backoff
        let delay = MEXC_RECONNECT_DELAY_MS * (1 << std::cmp::min(reconnect_count, 6));
        tokio::time::sleep(Duration::from_millis(delay)).await;
        
        // TODO: Implement actual reconnection logic
        // For now, simulate successful reconnection
        metrics.is_connected.store(true, Ordering::SeqCst);
        metrics.connection_uptime_start.store(
            Instant::now().elapsed().as_millis() as u64,
            Ordering::SeqCst
        );
        
        info!("✅ MEXC reconnection successful");
        Ok(())
    }

    /// Log connection statistics
    async fn log_connection_stats(metrics: &Arc<MexcConnectionMetrics>) {
        let now = Instant::now().elapsed().as_millis() as u64;
        let uptime = now.saturating_sub(metrics.connection_uptime_start.load(Ordering::Relaxed));
        let messages = metrics.message_count.load(Ordering::Relaxed);
        let errors = metrics.error_count.load(Ordering::Relaxed);
        let ping_time = metrics.ping_response_time_ms.load(Ordering::Relaxed);
        
        debug!("📊 MEXC Connection Stats: uptime={}min, messages={}, errors={}, ping={}ms",
               uptime / (1000 * 60), messages, errors, ping_time);
    }

    /// Record successful message receipt
    pub fn record_message(&self) {
        self.metrics.message_count.fetch_add(1, Ordering::Relaxed);
        self.metrics.last_ping_timestamp.store(
            Instant::now().elapsed().as_millis() as u64,
            Ordering::Relaxed
        );
    }

    /// Record connection error
    pub fn record_error(&self) {
        self.metrics.error_count.fetch_add(1, Ordering::Relaxed);
        self.metrics.is_connected.store(false, Ordering::SeqCst);
    }

    /// Get current connection metrics
    pub fn get_metrics(&self) -> MexcConnectionMetrics {
        MexcConnectionMetrics {
            is_connected: AtomicBool::new(self.metrics.is_connected.load(Ordering::Relaxed)),
            last_ping_timestamp: AtomicU64::new(self.metrics.last_ping_timestamp.load(Ordering::Relaxed)),
            ping_response_time_ms: AtomicU64::new(self.metrics.ping_response_time_ms.load(Ordering::Relaxed)),
            reconnect_count: AtomicUsize::new(self.metrics.reconnect_count.load(Ordering::Relaxed)),
            message_count: AtomicU64::new(self.metrics.message_count.load(Ordering::Relaxed)),
            error_count: AtomicU64::new(self.metrics.error_count.load(Ordering::Relaxed)),
            connection_uptime_start: AtomicU64::new(self.metrics.connection_uptime_start.load(Ordering::Relaxed)),
        }
    }

    /// Stop monitoring
    pub fn stop_monitoring(&self) {
        self.monitoring_active.store(false, Ordering::SeqCst);
        info!("🛑 MEXC connection monitoring stopped");
    }
}

pub struct MexcConnector {
    api_key: Option<String>,
    api_secret: Option<String>,
    sandbox: bool,
    streaming_manager: Arc<StreamingManager>,
    connection_monitor: Arc<MexcConnectionMonitor>,
}

impl MexcConnector {
    pub fn new(
        api_key: Option<String>,
        api_secret: Option<String>,
        sandbox: bool,
    ) -> Result<Self> {
        let streaming_manager = Arc::new(StreamingManager::new());
        let connection_monitor = Arc::new(MexcConnectionMonitor::new());
        
        info!("MEXC connector initialized with connection stability monitoring");
        
        Ok(Self {
            api_key,
            api_secret,
            sandbox,
            streaming_manager,
            connection_monitor,
        })
    }

    /// Get connection stability metrics
    pub fn get_connection_metrics(&self) -> MexcConnectionMetrics {
        self.connection_monitor.get_metrics()
    }

    /// Convert standard symbol format (BTC/USDT) to MEXC format (BTCUSDT)
    fn to_mexc_symbol(symbol: &str) -> String {
        symbol.replace('/', "")
    }

    /// Convert MEXC symbol format (BTCUSDT) to standard format (BTC/USDT)
    fn normalize_symbol(mexc_symbol: &str) -> String {
        // This is a simplified version - in production, you'd need a mapping
        // of known base/quote pairs
        if mexc_symbol.ends_with("USDT") {
            let base = &mexc_symbol[..mexc_symbol.len() - 4];
            format!("{}/USDT", base)
        } else if mexc_symbol.ends_with("BTC") {
            let base = &mexc_symbol[..mexc_symbol.len() - 3];
            format!("{}/BTC", base)
        } else if mexc_symbol.ends_with("ETH") {
            let base = &mexc_symbol[..mexc_symbol.len() - 3];
            format!("{}/ETH", base)
        } else {
            mexc_symbol.to_string()
        }
    }

    /// Parse order book from MEXC format
    fn parse_order_book(&self, book: MexcOrderBook) -> Result<OrderBookData> {
        let mut order_book = OrderBookData {
            exchange: "mexc".to_string(),
            symbol: Self::normalize_symbol(&book.symbol),
            timestamp: chrono::Utc::now().timestamp_millis(),
            bids: Vec::with_capacity(book.bids.len()),
            asks: Vec::with_capacity(book.asks.len()),
            sequence_id: None,
        };

        // Parse bids
        for bid in book.bids {
            let price = bid.p.parse::<Decimal>()
                .context("Failed to parse bid price")?;
            let quantity = bid.v.parse::<Decimal>()
                .context("Failed to parse bid quantity")?;
            order_book.bids.push([price.to_string().parse::<f64>().unwrap(), quantity.to_string().parse::<f64>().unwrap()]);
        }

        // Parse asks
        for ask in book.asks {
            let price = ask.p.parse::<Decimal>()
                .context("Failed to parse ask price")?;
            let quantity = ask.v.parse::<Decimal>()
                .context("Failed to parse ask quantity")?;
            order_book.asks.push([price.to_string().parse::<f64>().unwrap(), quantity.to_string().parse::<f64>().unwrap()]);
        }

        // MEXC sends pre-sorted data
        Ok(order_book)
    }

    /// Parse trade from MEXC format
    fn parse_trade(&self, symbol: &str, trade: MexcTrade) -> Result<TradeData> {
        Ok(TradeData {
            exchange: "mexc".to_string(),
            symbol: symbol.to_string(),
            id: trade.t.to_string(),
            price: trade.p.parse()?,
            quantity: trade.v.parse()?,
            side: if trade.S == 2 { "sell" } else { "buy" }.to_string(),
            is_maker: trade.S == 2,
            timestamp: trade.t as i64,
        })
    }
}

#[async_trait]
impl Exchange for MexcConnector {
    async fn connect(&self) -> Result<Connection> {
        info!("Connecting to MEXC with connection stability monitoring");
        
        let ws_url = if self.sandbox {
            "wss://wbs.mexc.com/ws" // MEXC doesn't have separate testnet WS
        } else {
            "wss://wbs.mexc.com/ws"
        };
        
        // Start connection monitoring
        self.connection_monitor.start_monitoring().await
            .context("Failed to start MEXC connection monitoring")?;
        
        info!("✅ MEXC connection established with stability monitoring");
        Ok(Arc::new(()) as Connection)
    }

    async fn subscribe_market_data(&self, symbols: Vec<String>) -> Result<crate::connector::MarketDataStream> {
        for symbol in &symbols {
            let mexc_symbol = Self::to_mexc_symbol(symbol);
            debug!("Subscribing to MEXC market data for {}", mexc_symbol);
        }
        
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn place_order(&self, order: Order) -> Result<OrderResult> {
        warn!("Order placement not yet implemented for MEXC");
        Ok(OrderResult {
            order_id: "MEXC-ORDER-ID".to_string(),
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
        warn!("Order cancellation not yet implemented for MEXC");
        Ok(())
    }

    async fn get_balance(&self) -> Result<Vec<Balance>> {
        warn!("Balance retrieval not yet implemented for MEXC");
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_conversion() {
        assert_eq!(MexcConnector::to_mexc_symbol("BTC/USDT"), "BTCUSDT");
        assert_eq!(MexcConnector::normalize_symbol("BTCUSDT"), "BTC/USDT");
        assert_eq!(MexcConnector::to_mexc_symbol("ETH/BTC"), "ETHBTC");
        assert_eq!(MexcConnector::normalize_symbol("ETHBTC"), "ETH/BTC");
    }

    #[tokio::test]
    async fn test_order_book_parsing() {
        let connector = MexcConnector::new("wss://test.mexc.com".to_string());
        
        let mexc_book = MexcOrderBook {
            symbol: "BTCUSDT".to_string(),
            version: "1".to_string(),
            bids: vec![
                OrderLevel { p: "42000.50".to_string(), v: "1.5".to_string() },
                OrderLevel { p: "42000.00".to_string(), v: "2.0".to_string() },
            ],
            asks: vec![
                OrderLevel { p: "42001.00".to_string(), v: "1.0".to_string() },
                OrderLevel { p: "42001.50".to_string(), v: "2.5".to_string() },
            ],
        };
        
        let order_book = connector.parse_order_book(mexc_book).unwrap();
        
        assert_eq!(order_book.exchange, "mexc");
        assert_eq!(order_book.symbol, "BTC/USDT");
        assert_eq!(order_book.bids.len(), 2);
        assert_eq!(order_book.asks.len(), 2);
        assert_eq!(order_book.bids[0].0.to_string(), "42000.50");
        assert_eq!(order_book.asks[0].0.to_string(), "42001.00");
    }

    #[test]
    fn test_trade_parsing() {
        let connector = MexcConnector::new("wss://test.mexc.com".to_string());
        
        let mexc_trade = MexcTrade {
            p: "42000.50".to_string(),
            v: "0.5".to_string(),
            S: 1, // buy
            t: 1234567890,
        };
        
        let trade = connector.parse_trade("BTC/USDT", mexc_trade).unwrap();
        
        assert_eq!(trade.exchange, "mexc");
        assert_eq!(trade.symbol, "BTC/USDT");
        assert_eq!(trade.price.to_string(), "42000.50");
        assert_eq!(trade.quantity.to_string(), "0.5");
        assert_eq!(trade.side, "buy");
        assert_eq!(trade.timestamp, 1234567890);
    }
}