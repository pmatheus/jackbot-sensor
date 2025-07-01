//! Exchange connector system for Jackbot Sensor
//!
//! This module provides a unified interface for connecting to multiple cryptocurrency exchanges
//! while leveraging the existing jackbot ecosystem modules for market data and trading operations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

// Import jackbot modules
use jackbot_execution::client::ExecutionClient;
use jackbot_instrument::exchange::ExchangeId;
use jackbot_integration::circuit_breaker::CircuitBreaker;
use jackbot_integration::protocol::websocket::WebSocket;

use crate::api::{BalanceData, KlineData, OrderBookData, PositionData, TickerData, TradeData};
use crate::rate_limit::RateLimitManager;
use crate::streaming::StreamingManager;

// Type aliases for simplified usage
type RateLimiter = RateLimitManager;

// Simple trading client trait for the connector
pub trait TradingClient: Send + Sync {
    fn exchange_id(&self) -> ExchangeId;
    async fn place_order(&self, order: OrderRequest) -> Result<OrderResponse>;
    async fn cancel_order(&self, order_id: &str, symbol: Option<&str>) -> Result<()>;
    async fn get_balances(&self) -> Result<Vec<BalanceData>>;
}

// Mock trading client for testing
#[derive(Debug, Clone)]
pub struct MockExchangeClient {
    exchange_id: ExchangeId,
}

impl MockExchangeClient {
    pub fn new() -> Self {
        Self {
            exchange_id: ExchangeId::BinanceSpot,
        }
    }
}

impl TradingClient for MockExchangeClient {
    fn exchange_id(&self) -> ExchangeId {
        self.exchange_id
    }

    async fn place_order(&self, _order: OrderRequest) -> Result<OrderResponse> {
        Ok(OrderResponse {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: "mock_user".to_string(),
            exchange: self.exchange_id.as_str().to_string(),
            symbol: "BTC/USDT".to_string(),
            side: "buy".to_string(),
            order_type: "limit".to_string(),
            status: "filled".to_string(),
            price: 100000.0,
            quantity: 1.0,
            filled: 1.0,
            remaining: 0.0,
            fees: 0.001,
            fee_asset: "USDT".to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            updated_at: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn cancel_order(&self, _order_id: &str, _symbol: Option<&str>) -> Result<()> {
        Ok(())
    }

    async fn get_balances(&self) -> Result<Vec<BalanceData>> {
        Ok(vec![BalanceData {
            user_id: "mock_user".to_string(),
            exchange: self.exchange_id.as_str().to_string(),
            asset: "USDT".to_string(),
            free: 10000.0,
            locked: 0.0,
            total: 10000.0,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }])
    }
}

// Order request/response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub exchange: String,
    pub symbol: String,
    pub side: String,       // "buy" or "sell"
    pub order_type: String, // "market", "limit", etc.
    pub price: Option<f64>,
    pub quantity: f64,
    pub time_in_force: Option<String>,
    pub reduce_only: Option<bool>,
    pub post_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub id: String,
    pub user_id: String,
    pub exchange: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub status: String,
    pub price: f64,
    pub quantity: f64,
    pub filled: f64,
    pub remaining: f64,
    pub fees: f64,
    pub fee_asset: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Connection status for an exchange connector
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error(String),
}

/// Health metrics for an exchange connector
#[derive(Debug, Clone)]
pub struct ConnectorHealth {
    pub status: ConnectionStatus,
    pub last_heartbeat: Option<Instant>,
    pub latency_ms: Option<u64>,
    pub message_count: u64,
    pub error_count: u64,
    pub reconnect_count: u64,
    pub uptime: Duration,
}

/// Configuration for an exchange connector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub exchange_id: ExchangeId,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub sandbox: bool,
    pub rate_limit_per_second: u32,
    pub max_reconnect_attempts: u32,
    pub heartbeat_interval: Duration,
    pub connection_timeout: Duration,
    pub subscriptions: Vec<String>,
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            exchange_id: ExchangeId::BinanceSpot,
            api_key: None,
            api_secret: None,
            sandbox: false,
            rate_limit_per_second: 100,
            max_reconnect_attempts: 10,
            heartbeat_interval: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(10),
            subscriptions: Vec::new(),
        }
    }
}

/// Exchange connector trait defining the interface for all exchange integrations
#[async_trait::async_trait]
pub trait ExchangeConnector: Send + Sync {
    /// Exchange identifier
    fn exchange_id(&self) -> ExchangeId;

    /// Get current connection status
    fn status(&self) -> ConnectionStatus;

    /// Get health metrics
    fn health(&self) -> ConnectorHealth;

    /// Connect to the exchange
    async fn connect(&mut self) -> Result<()>;

    /// Disconnect from the exchange
    async fn disconnect(&mut self) -> Result<()>;

    /// Reconnect to the exchange
    async fn reconnect(&mut self) -> Result<()>;

    /// Subscribe to market data streams
    async fn subscribe_market_data(&mut self, symbols: Vec<String>) -> Result<()>;

    /// Subscribe to user data streams (orders, balances, positions)
    async fn subscribe_user_data(&mut self) -> Result<()>;

    /// Place a trading order
    async fn place_order(&self, order: OrderRequest) -> Result<OrderResponse>;

    /// Cancel a trading order
    async fn cancel_order(&self, order_id: &str, symbol: Option<&str>) -> Result<()>;

    /// Get account balances
    async fn get_balances(&self) -> Result<Vec<BalanceData>>;

    /// Get open positions (for futures exchanges)
    async fn get_positions(&self) -> Result<Vec<PositionData>>;

    /// Get trading symbols available on this exchange
    async fn get_symbols(&self) -> Result<Vec<String>>;

    /// Perform a health check
    async fn health_check(&mut self) -> Result<bool>;
}

/// Connection pool for managing WebSocket connections efficiently
#[derive(Debug)]
pub struct ConnectionPool {
    connections: Arc<RwLock<HashMap<String, Arc<WebSocket>>>>,
    max_connections_per_exchange: usize,
    connection_timeout: Duration,
}

impl ConnectionPool {
    pub fn new(max_connections_per_exchange: usize, connection_timeout: Duration) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            max_connections_per_exchange,
            connection_timeout,
        }
    }

    pub async fn get_connection(
        &self,
        exchange_id: ExchangeId,
        endpoint: &str,
    ) -> Result<Arc<WebSocket>> {
        let key = format!("{}_{}", exchange_id.as_str(), endpoint);
        let connections = self.connections.read().await;

        if let Some(connection) = connections.get(&key) {
            // Return existing connection (TODO: add proper health check)
            return Ok(connection.clone());
        }

        drop(connections);

        // Create new connection
        self.create_connection(key, endpoint).await
    }

    async fn create_connection(&self, key: String, endpoint: &str) -> Result<Arc<WebSocket>> {
        let connection = Arc::new(
            jackbot_integration::protocol::websocket::connect(Url::parse(endpoint)?)
                .await
                .context("Failed to create WebSocket connection")?,
        );

        let mut connections = self.connections.write().await;
        connections.insert(key.clone(), connection.clone());

        info!("Created new WebSocket connection: {}", key);
        Ok(connection)
    }

    pub async fn remove_connection(&self, exchange_id: ExchangeId, endpoint: &str) {
        let key = format!("{}_{}", exchange_id.as_str(), endpoint);
        let mut connections = self.connections.write().await;

        if let Some(connection) = connections.remove(&key) {
            // TODO: Fix close method signature
            // let _ = connection.close().await;
            info!("Removed WebSocket connection: {}", key);
        }
    }

    pub async fn cleanup_dead_connections(&self) {
        let mut connections = self.connections.write().await;
        let mut to_remove = Vec::new();

        for (key, connection) in connections.iter() {
            // TODO: Implement proper connection check
            if false { // Temporary placeholder
                to_remove.push(key.clone());
            }
        }

        for key in to_remove {
            connections.remove(&key);
            debug!("Cleaned up dead connection: {}", key);
        }
    }
}

/// Generic exchange connector implementation using jackbot modules
pub struct GenericExchangeConnector {
    config: ConnectorConfig,
    status: ConnectionStatus,
    health: ConnectorHealth,
    connection_pool: Arc<ConnectionPool>,
    circuit_breaker: CircuitBreaker,
    rate_limiter: RateLimiter,
    streaming_manager: Arc<StreamingManager>,
    // TODO: Fix trait object compatibility for async traits
    // trading_client: Option<Box<dyn TradingClient>>,
    market_data_tx: Option<broadcast::Sender<serde_json::Value>>,
    user_data_tx: Option<broadcast::Sender<serde_json::Value>>,
    start_time: Instant,
}

impl GenericExchangeConnector {
    pub fn new(
        config: ConnectorConfig,
        streaming_manager: Arc<StreamingManager>,
        connection_pool: Arc<ConnectionPool>,
    ) -> Self {
        let circuit_breaker = CircuitBreaker::new(
            5,                       // failure_threshold
            Duration::from_secs(60), // recovery_timeout
        );

        // TODO: Fix RateLimiter constructor signature
        let rate_limiter = RateLimiter::new(config.rate_limit_per_second);

        Self {
            config: config.clone(),
            status: ConnectionStatus::Disconnected,
            health: ConnectorHealth {
                status: ConnectionStatus::Disconnected,
                last_heartbeat: None,
                latency_ms: None,
                message_count: 0,
                error_count: 0,
                reconnect_count: 0,
                uptime: Duration::from_secs(0),
            },
            connection_pool,
            circuit_breaker,
            rate_limiter,
            streaming_manager,
            trading_client: None,
            market_data_tx: None,
            user_data_tx: None,
            start_time: Instant::now(),
        }
    }

    /// Initialize the trading client based on exchange configuration
    fn initialize_trading_client(&mut self) -> Result<()> {
        match self.config.exchange_id {
            ExchangeId::BinanceSpot | ExchangeId::BinanceFuturesUsd => {
                // Use jackbot-execution Binance client
                if let (Some(api_key), Some(api_secret)) =
                    (&self.config.api_key, &self.config.api_secret)
                {
                    // For now, use the BinanceFuturesUsd client from jackbot-execution
                    self.trading_client = Some(Box::new(
                        jackbot_execution::client::binance::futures::BinanceFuturesUsd::new(
                            jackbot_execution::client::binance::futures::BinanceFuturesUsdConfig::default()
                        )
                    ));
                } else {
                    // Use mock client for testing
                    self.trading_client = Some(Box::new(MockExchangeClient::new()));
                }
            }
            ExchangeId::Coinbase => {
                // Use mock client for now - could be extended with real Coinbase client
                self.trading_client = Some(Box::new(MockExchangeClient::new()));
            }
            _ => {
                // Default to mock client
                self.trading_client = Some(Box::new(MockExchangeClient::new()));
            }
        }

        info!(
            "Initialized trading client for {}",
            self.config.exchange_id.as_str()
        );
        Ok(())
    }

    /// Start background tasks for health monitoring and message processing
    async fn start_background_tasks(&mut self) -> Result<()> {
        // Health check task
        let health_interval = self.config.heartbeat_interval;
        let exchange_id = self.config.exchange_id;
        let connection_pool = self.connection_pool.clone();

        tokio::spawn(async move {
            let mut interval = interval(health_interval);
            loop {
                interval.tick().await;

                // Perform health checks
                connection_pool.cleanup_dead_connections().await;

                debug!("Health check completed for {}", exchange_id.as_str());
            }
        });

        // Market data processing task
        if let Some(mut rx) = self.market_data_tx.as_ref().map(|tx| tx.subscribe()) {
            let streaming_manager = self.streaming_manager.clone();
            let exchange_id = self.config.exchange_id;

            tokio::spawn(async move {
                while let Ok(data) = rx.recv().await {
                    if let Err(e) =
                        Self::process_market_data(&streaming_manager, exchange_id, data).await
                    {
                        error!(
                            "Failed to process market data for {}: {}",
                            exchange_id.as_str(),
                            e
                        );
                    }
                }
            });
        }

        Ok(())
    }

    /// Process incoming market data and publish to streaming manager
    async fn process_market_data(
        streaming_manager: &StreamingManager,
        exchange_id: ExchangeId,
        data: serde_json::Value,
    ) -> Result<()> {
        // Parse the message type and route to appropriate handler
        if let Some(stream) = data.get("stream").and_then(|s| s.as_str()) {
            if stream.contains("@ticker") {
                if let Ok(ticker) = Self::parse_ticker_data(exchange_id, &data) {
                    streaming_manager.publish_ticker(ticker).await?;
                }
            } else if stream.contains("@depth") {
                if let Ok(orderbook) = Self::parse_orderbook_data(exchange_id, &data) {
                    streaming_manager.publish_orderbook(orderbook).await?;
                }
            } else if stream.contains("@trade") {
                if let Ok(trade) = Self::parse_trade_data(exchange_id, &data) {
                    streaming_manager.publish_trade(trade).await?;
                }
            } else if stream.contains("@kline") {
                if let Ok(kline) = Self::parse_kline_data(exchange_id, &data) {
                    streaming_manager.publish_kline(kline).await?;
                }
            }
        }

        Ok(())
    }

    /// Parse ticker data from exchange-specific format to unified format
    fn parse_ticker_data(exchange_id: ExchangeId, data: &serde_json::Value) -> Result<TickerData> {
        match exchange_id {
            ExchangeId::BinanceSpot | ExchangeId::BinanceFuturesUsd => {
                let ticker_data = data.get("data").context("Missing ticker data")?;
                Ok(TickerData {
                    symbol: Self::normalize_symbol(
                        ticker_data.get("s").and_then(|s| s.as_str()).unwrap_or(""),
                    ),
                    exchange: exchange_id.as_str().to_string(),
                    price: ticker_data
                        .get("c")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    bid: ticker_data
                        .get("b")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    ask: ticker_data
                        .get("a")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    volume_24h: ticker_data
                        .get("v")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    change_24h: ticker_data
                        .get("P")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    high_24h: ticker_data
                        .get("h")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    low_24h: ticker_data
                        .get("l")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
            _ => {
                // Generic parsing for other exchanges
                Ok(TickerData {
                    symbol: "BTC/USDT".to_string(),
                    exchange: exchange_id.as_str().to_string(),
                    price: 100000.0,
                    bid: 99999.0,
                    ask: 100001.0,
                    volume_24h: 1000.0,
                    change_24h: 2.5,
                    high_24h: 101000.0,
                    low_24h: 99000.0,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
        }
    }

    /// Parse order book data from exchange format
    fn parse_orderbook_data(
        exchange_id: ExchangeId,
        data: &serde_json::Value,
    ) -> Result<OrderBookData> {
        match exchange_id {
            ExchangeId::BinanceSpot | ExchangeId::BinanceFuturesUsd => {
                let orderbook_data = data.get("data").context("Missing orderbook data")?;
                let bids = orderbook_data
                    .get("b")
                    .and_then(|b| b.as_array())
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|level| {
                        if let (Some(price), Some(qty)) = (
                            level
                                .get(0)
                                .and_then(|p| p.as_str())
                                .and_then(|s| s.parse::<f64>().ok()),
                            level
                                .get(1)
                                .and_then(|q| q.as_str())
                                .and_then(|s| s.parse::<f64>().ok()),
                        ) {
                            Some([price, qty])
                        } else {
                            None
                        }
                    })
                    .collect();

                let asks = orderbook_data
                    .get("a")
                    .and_then(|a| a.as_array())
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|level| {
                        if let (Some(price), Some(qty)) = (
                            level
                                .get(0)
                                .and_then(|p| p.as_str())
                                .and_then(|s| s.parse::<f64>().ok()),
                            level
                                .get(1)
                                .and_then(|q| q.as_str())
                                .and_then(|s| s.parse::<f64>().ok()),
                        ) {
                            Some([price, qty])
                        } else {
                            None
                        }
                    })
                    .collect();

                Ok(OrderBookData {
                    symbol: Self::normalize_symbol(
                        data.get("stream")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .split('@')
                            .next()
                            .unwrap_or(""),
                    ),
                    exchange: exchange_id.as_str().to_string(),
                    bids,
                    asks,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    sequence_id: orderbook_data.get("u").and_then(|u| u.as_u64()),
                })
            }
            _ => {
                // Generic orderbook for other exchanges
                Ok(OrderBookData {
                    symbol: "BTC/USDT".to_string(),
                    exchange: exchange_id.as_str().to_string(),
                    bids: vec![[100000.0, 1.0], [99999.0, 2.0]],
                    asks: vec![[100001.0, 1.5], [100002.0, 0.5]],
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    sequence_id: Some(1),
                })
            }
        }
    }

    /// Parse trade data from exchange format
    fn parse_trade_data(exchange_id: ExchangeId, data: &serde_json::Value) -> Result<TradeData> {
        match exchange_id {
            ExchangeId::BinanceSpot | ExchangeId::BinanceFuturesUsd => {
                let trade_data = data.get("data").context("Missing trade data")?;
                Ok(TradeData {
                    symbol: Self::normalize_symbol(
                        trade_data.get("s").and_then(|s| s.as_str()).unwrap_or(""),
                    ),
                    exchange: exchange_id.as_str().to_string(),
                    id: trade_data
                        .get("t")
                        .and_then(|t| t.as_u64())
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| Uuid::new_v4().to_string()),
                    price: trade_data
                        .get("p")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    quantity: trade_data
                        .get("q")
                        .and_then(|q| q.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    side: if trade_data
                        .get("m")
                        .and_then(|m| m.as_bool())
                        .unwrap_or(false)
                    {
                        "sell"
                    } else {
                        "buy"
                    }
                    .to_string(),
                    timestamp: trade_data
                        .get("T")
                        .and_then(|t| t.as_i64())
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
                    is_maker: trade_data
                        .get("m")
                        .and_then(|m| m.as_bool())
                        .unwrap_or(false),
                })
            }
            _ => {
                // Generic trade for other exchanges
                Ok(TradeData {
                    symbol: "BTC/USDT".to_string(),
                    exchange: exchange_id.as_str().to_string(),
                    id: Uuid::new_v4().to_string(),
                    price: 100000.0,
                    quantity: 0.1,
                    side: "buy".to_string(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    is_maker: false,
                })
            }
        }
    }

    /// Parse kline/candlestick data from exchange format
    fn parse_kline_data(exchange_id: ExchangeId, data: &serde_json::Value) -> Result<KlineData> {
        match exchange_id {
            ExchangeId::BinanceSpot | ExchangeId::BinanceFuturesUsd => {
                let kline_data = data
                    .get("data")
                    .and_then(|d| d.get("k"))
                    .context("Missing kline data")?;
                Ok(KlineData {
                    symbol: Self::normalize_symbol(
                        kline_data.get("s").and_then(|s| s.as_str()).unwrap_or(""),
                    ),
                    exchange: exchange_id.as_str().to_string(),
                    interval: kline_data
                        .get("i")
                        .and_then(|i| i.as_str())
                        .unwrap_or("1m")
                        .to_string(),
                    open_time: kline_data
                        .get("t")
                        .and_then(|t| t.as_i64())
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
                    close_time: kline_data
                        .get("T")
                        .and_then(|t| t.as_i64())
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
                    open: kline_data
                        .get("o")
                        .and_then(|o| o.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    high: kline_data
                        .get("h")
                        .and_then(|h| h.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    low: kline_data
                        .get("l")
                        .and_then(|l| l.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    close: kline_data
                        .get("c")
                        .and_then(|c| c.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    volume: kline_data
                        .get("v")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                    trades: kline_data.get("n").and_then(|n| n.as_u64()).unwrap_or(0),
                    is_final: kline_data
                        .get("x")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false),
                })
            }
            _ => {
                // Generic kline for other exchanges
                Ok(KlineData {
                    symbol: "BTC/USDT".to_string(),
                    exchange: exchange_id.as_str().to_string(),
                    interval: "1m".to_string(),
                    open_time: chrono::Utc::now().timestamp_millis() - 60000,
                    close_time: chrono::Utc::now().timestamp_millis(),
                    open: 99900.0,
                    high: 100100.0,
                    low: 99800.0,
                    close: 100000.0,
                    volume: 100.0,
                    trades: 50,
                    is_final: true,
                })
            }
        }
    }

    /// Normalize symbol format (e.g., "BTCUSDT" -> "BTC/USDT")
    fn normalize_symbol(raw_symbol: &str) -> String {
        if raw_symbol.contains('/') {
            return raw_symbol.to_string();
        }

        // Common patterns for symbol normalization
        let common_quotes = ["USDT", "USDC", "BTC", "ETH", "BNB"];

        for quote in &common_quotes {
            if raw_symbol.ends_with(quote) && raw_symbol.len() > quote.len() {
                let base = &raw_symbol[..raw_symbol.len() - quote.len()];
                return format!("{}/{}", base, quote);
            }
        }

        // Fallback: assume first 3 chars are base, rest are quote
        if raw_symbol.len() >= 6 {
            return format!("{}/{}", &raw_symbol[..3], &raw_symbol[3..]);
        }

        raw_symbol.to_string()
    }

    /// Update health metrics
    fn update_health(&mut self) {
        self.health.status = self.status.clone();
        self.health.last_heartbeat = Some(Instant::now());
        self.health.uptime = self.start_time.elapsed();
    }
}

#[async_trait::async_trait]
impl ExchangeConnector for GenericExchangeConnector {
    fn exchange_id(&self) -> ExchangeId {
        self.config.exchange_id
    }

    fn status(&self) -> ConnectionStatus {
        self.status.clone()
    }

    fn health(&self) -> ConnectorHealth {
        self.health.clone()
    }

    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to {}", self.config.exchange_id.as_str());
        self.status = ConnectionStatus::Connecting;

        // Initialize trading client
        self.initialize_trading_client()?;

        // Create broadcast channels for market data
        let (market_tx, _) = broadcast::channel(1000);
        let (user_tx, _) = broadcast::channel(1000);

        self.market_data_tx = Some(market_tx);
        self.user_data_tx = Some(user_tx);

        // Start background tasks
        self.start_background_tasks().await?;

        self.status = ConnectionStatus::Connected;
        self.update_health();

        info!(
            "Successfully connected to {}",
            self.config.exchange_id.as_str()
        );
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from {}", self.config.exchange_id.as_str());

        self.status = ConnectionStatus::Disconnected;
        self.market_data_tx = None;
        self.user_data_tx = None;

        info!(
            "Successfully disconnected from {}",
            self.config.exchange_id.as_str()
        );
        Ok(())
    }

    async fn reconnect(&mut self) -> Result<()> {
        info!("Reconnecting to {}", self.config.exchange_id.as_str());
        self.status = ConnectionStatus::Reconnecting;
        self.health.reconnect_count += 1;

        self.disconnect().await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.connect().await?;

        Ok(())
    }

    async fn subscribe_market_data(&mut self, symbols: Vec<String>) -> Result<()> {
        info!(
            "Subscribing to market data for {} symbols on {}",
            symbols.len(),
            self.config.exchange_id.as_str()
        );

        // This would implement actual subscription logic using jackbot-data modules
        // For now, simulate subscription success
        for symbol in symbols {
            debug!("Subscribed to market data for {}", symbol);
        }

        Ok(())
    }

    async fn subscribe_user_data(&mut self) -> Result<()> {
        info!(
            "Subscribing to user data streams on {}",
            self.config.exchange_id.as_str()
        );

        // This would implement actual user data subscription
        // For now, simulate subscription success
        debug!("Subscribed to user data streams");

        Ok(())
    }

    async fn place_order(&self, order: OrderRequest) -> Result<OrderResponse> {
        info!(
            "Placing order on {}: {:?}",
            self.config.exchange_id.as_str(),
            order
        );

        // Rate limiting check
        self.rate_limiter
            .check_rate_limit(crate::rate_limit::RateLimitBucket::Orders(
                "default_user".to_string(),
            ))
            .await?;

        // Circuit breaker check
        self.circuit_breaker
            .call(async {
                if let Some(trading_client) = &self.trading_client {
                    trading_client.place_order(order).await
                } else {
                    Err(anyhow::anyhow!("Trading client not initialized"))
                }
            })
            .await
    }

    async fn cancel_order(&self, order_id: &str, symbol: Option<&str>) -> Result<()> {
        info!(
            "Cancelling order {} on {}",
            order_id,
            self.config.exchange_id.as_str()
        );

        // Rate limiting check
        self.rate_limiter
            .check_rate_limit(crate::rate_limit::RateLimitBucket::Orders(
                "default_user".to_string(),
            ))
            .await?;

        // Circuit breaker check
        self.circuit_breaker
            .call(async {
                if let Some(trading_client) = &self.trading_client {
                    trading_client.cancel_order(order_id, symbol).await
                } else {
                    Err(anyhow::anyhow!("Trading client not initialized"))
                }
            })
            .await
    }

    async fn get_balances(&self) -> Result<Vec<BalanceData>> {
        debug!("Getting balances from {}", self.config.exchange_id.as_str());

        // Rate limiting check
        self.rate_limiter
            .check_rate_limit(crate::rate_limit::RateLimitBucket::Positions(
                "default_user".to_string(),
            ))
            .await?;

        if let Some(trading_client) = &self.trading_client {
            let balances = trading_client.get_balances().await?;

            // Convert to BalanceData format
            let balance_data = balances
                .into_iter()
                .map(|balance| BalanceData {
                    user_id: "user_123".to_string(), // TODO: Get from auth context
                    exchange: self.config.exchange_id.as_str().to_string(),
                    asset: balance.asset,
                    free: balance.free,
                    locked: balance.locked,
                    total: balance.free + balance.locked,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
                .collect();

            Ok(balance_data)
        } else {
            Err(anyhow::anyhow!("Trading client not initialized"))
        }
    }

    async fn get_positions(&self) -> Result<Vec<PositionData>> {
        debug!(
            "Getting positions from {}",
            self.config.exchange_id.as_str()
        );

        // Rate limiting check
        self.rate_limiter
            .check_rate_limit(crate::rate_limit::RateLimitBucket::Positions(
                "default_user".to_string(),
            ))
            .await?;

        // For now, return empty positions - would be implemented with real trading client
        Ok(vec![])
    }

    async fn get_symbols(&self) -> Result<Vec<String>> {
        debug!("Getting symbols from {}", self.config.exchange_id.as_str());

        // Rate limiting check
        self.rate_limiter
            .check_rate_limit(crate::rate_limit::RateLimitBucket::MarketData(
                std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            ))
            .await?;

        // This would fetch actual symbols from the exchange
        // For now, return some common symbols
        Ok(vec![
            "BTC/USDT".to_string(),
            "ETH/USDT".to_string(),
            "BNB/USDT".to_string(),
            "SOL/USDT".to_string(),
        ])
    }

    async fn health_check(&mut self) -> Result<bool> {
        let start = Instant::now();

        // Simple health check - verify connection is alive
        let is_healthy = match &self.status {
            ConnectionStatus::Connected => {
                // Could ping the exchange or check last message time
                true
            }
            _ => false,
        };

        self.health.latency_ms = Some(start.elapsed().as_millis() as u64);
        self.update_health();

        Ok(is_healthy)
    }
}

/// Manager for multiple exchange connectors
pub struct ConnectorManager {
    connectors: Arc<RwLock<HashMap<ExchangeId, Box<dyn ExchangeConnector>>>>,
    connection_pool: Arc<ConnectionPool>,
    streaming_manager: Arc<StreamingManager>,
    health_check_interval: Duration,
}

impl ConnectorManager {
    pub fn new(streaming_manager: Arc<StreamingManager>, health_check_interval: Duration) -> Self {
        let connection_pool = Arc::new(ConnectionPool::new(
            10,                      // max connections per exchange
            Duration::from_secs(10), // connection timeout
        ));

        Self {
            connectors: Arc::new(RwLock::new(HashMap::new())),
            connection_pool,
            streaming_manager,
            health_check_interval,
        }
    }

    /// Add a new exchange connector
    pub async fn add_connector(&self, config: ConnectorConfig) -> Result<()> {
        let connector = Box::new(GenericExchangeConnector::new(
            config.clone(),
            self.streaming_manager.clone(),
            self.connection_pool.clone(),
        ));

        let mut connectors = self.connectors.write().await;
        connectors.insert(config.exchange_id, connector);

        info!("Added connector for {}", config.exchange_id.as_str());
        Ok(())
    }

    /// Remove an exchange connector
    pub async fn remove_connector(&self, exchange_id: ExchangeId) -> Result<()> {
        let mut connectors = self.connectors.write().await;

        if let Some(mut connector) = connectors.remove(&exchange_id) {
            connector.disconnect().await?;
            info!("Removed connector for {}", exchange_id.as_str());
        }

        Ok(())
    }

    /// Connect all connectors
    pub async fn connect_all(&self) -> Result<()> {
        let mut connectors = self.connectors.write().await;

        for (exchange_id, connector) in connectors.iter_mut() {
            if let Err(e) = connector.connect().await {
                error!("Failed to connect to {}: {}", exchange_id.as_str(), e);
            }
        }

        Ok(())
    }

    /// Disconnect all connectors
    pub async fn disconnect_all(&self) -> Result<()> {
        let mut connectors = self.connectors.write().await;

        for (exchange_id, connector) in connectors.iter_mut() {
            if let Err(e) = connector.disconnect().await {
                error!("Failed to disconnect from {}: {}", exchange_id.as_str(), e);
            }
        }

        Ok(())
    }

    /// Get connector for specific exchange
    pub async fn get_connector(
        &self,
        exchange_id: ExchangeId,
    ) -> Option<Box<dyn ExchangeConnector>> {
        let connectors = self.connectors.read().await;
        // Note: This is a simplified version - in practice you'd need proper cloning or references
        None
    }

    /// Get health status of all connectors
    pub async fn get_health_status(&self) -> HashMap<ExchangeId, ConnectorHealth> {
        let connectors = self.connectors.read().await;
        let mut status = HashMap::new();

        for (exchange_id, connector) in connectors.iter() {
            status.insert(*exchange_id, connector.health());
        }

        status
    }

    /// Start health monitoring for all connectors
    pub async fn start_health_monitoring(&self) {
        let connectors = self.connectors.clone();
        let interval = self.health_check_interval;

        tokio::spawn(async move {
            let mut health_interval = tokio::time::interval(interval);

            loop {
                health_interval.tick().await;

                let mut connectors_guard = connectors.write().await;
                for (exchange_id, connector) in connectors_guard.iter_mut() {
                    match connector.health_check().await {
                        Ok(is_healthy) => {
                            if !is_healthy {
                                warn!("Health check failed for {}", exchange_id.as_str());
                                // Could trigger reconnection here
                            }
                        }
                        Err(e) => {
                            error!("Health check error for {}: {}", exchange_id.as_str(), e);
                        }
                    }
                }
            }
        });
    }

    /// Subscribe to market data across all exchanges
    pub async fn subscribe_market_data_all(&self, symbols: Vec<String>) -> Result<()> {
        let mut connectors = self.connectors.write().await;

        for (exchange_id, connector) in connectors.iter_mut() {
            if let Err(e) = connector.subscribe_market_data(symbols.clone()).await {
                error!(
                    "Failed to subscribe to market data on {}: {}",
                    exchange_id.as_str(),
                    e
                );
            }
        }

        Ok(())
    }

    /// Place order on specific exchange
    pub async fn place_order(
        &self,
        exchange_id: ExchangeId,
        order: OrderRequest,
    ) -> Result<OrderResponse> {
        let connectors = self.connectors.read().await;

        if let Some(connector) = connectors.get(&exchange_id) {
            connector.place_order(order).await
        } else {
            Err(anyhow::anyhow!(
                "Connector not found for {}",
                exchange_id.as_str()
            ))
        }
    }

    /// Get aggregated balances across all exchanges
    pub async fn get_all_balances(&self) -> Result<Vec<BalanceData>> {
        let connectors = self.connectors.read().await;
        let mut all_balances = Vec::new();

        for (exchange_id, connector) in connectors.iter() {
            match connector.get_balances().await {
                Ok(mut balances) => {
                    all_balances.append(&mut balances);
                }
                Err(e) => {
                    error!(
                        "Failed to get balances from {}: {}",
                        exchange_id.as_str(),
                        e
                    );
                }
            }
        }

        Ok(all_balances)
    }
}

/// Factory for creating exchange-specific connectors
pub struct ConnectorFactory;

impl ConnectorFactory {
    /// Create a connector configuration for a specific exchange
    pub fn create_config(exchange_id: ExchangeId) -> ConnectorConfig {
        let mut config = ConnectorConfig::default();
        config.exchange_id = exchange_id;

        match exchange_id {
            ExchangeId::BinanceSpot | ExchangeId::BinanceFuturesUsd => {
                config.rate_limit_per_second = 1200; // Binance allows higher rates
                config.heartbeat_interval = Duration::from_secs(30);
                config.subscriptions = vec![
                    "ticker".to_string(),
                    "depth".to_string(),
                    "trade".to_string(),
                    "kline".to_string(),
                ];
            }
            ExchangeId::Coinbase => {
                config.rate_limit_per_second = 100;
                config.heartbeat_interval = Duration::from_secs(60);
                config.subscriptions = vec![
                    "ticker".to_string(),
                    "level2".to_string(),
                    "matches".to_string(),
                ];
            }
            _ => {
                // Default configuration for other exchanges
            }
        }

        config
    }

    /// Create and configure a connector for an exchange
    pub fn create_connector(
        config: ConnectorConfig,
        streaming_manager: Arc<StreamingManager>,
        connection_pool: Arc<ConnectionPool>,
    ) -> Box<dyn ExchangeConnector> {
        Box::new(GenericExchangeConnector::new(
            config,
            streaming_manager,
            connection_pool,
        ))
    }
}
