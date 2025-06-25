use chrono::{DateTime, Utc};
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast, RwLock},
    time::interval,
};
use tracing::{debug, error, info, warn};

/// Real-time market data collector with multi-exchange support
#[derive(Debug)]
pub struct MarketDataCollector {
    /// Exchange connectors
    exchange_connectors: HashMap<ExchangeId, Arc<dyn ExchangeConnector + Send + Sync>>,
    /// Current market data cache
    market_data_cache: Arc<RwLock<MarketDataCache>>,
    /// Data broadcast channel
    data_broadcaster: broadcast::Sender<MarketDataUpdate>,
    /// Collection configuration
    config: DataCollectionConfig,
    /// Collection statistics
    stats: Arc<RwLock<CollectionStatistics>>,
    /// Active subscriptions
    subscriptions: Arc<RwLock<HashMap<ExchangeId, Vec<MarketDataSubscription>>>>,
}

/// Exchange connector trait for pluggable exchange integrations
pub trait ExchangeConnector: std::fmt::Debug + Send + Sync {
    /// Connect to exchange and start data streams
    fn connect<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>>;

    /// Disconnect from exchange
    fn disconnect<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>>;

    /// Subscribe to market data for specific instruments
    fn subscribe_market_data<'a>(
        &'a self,
        instruments: Vec<InstrumentNameExchange>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>>;

    /// Subscribe to order book updates
    fn subscribe_order_book<'a>(
        &'a self,
        instruments: Vec<InstrumentNameExchange>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>>;

    /// Subscribe to trade data
    fn subscribe_trades<'a>(
        &'a self,
        instruments: Vec<InstrumentNameExchange>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>>;

    /// Get current connection status
    fn connection_status(&self) -> ConnectionStatus;

    /// Get exchange ID
    fn exchange_id(&self) -> ExchangeId;
}

/// Market data cache for efficient data access
#[derive(Debug, Clone, Default)]
pub struct MarketDataCache {
    /// Latest prices by instrument
    pub latest_prices: HashMap<InstrumentKey, PriceData>,
    /// Order book snapshots
    pub order_books: HashMap<InstrumentKey, OrderBookSnapshot>,
    /// Recent trades
    pub recent_trades: HashMap<InstrumentKey, Vec<TradeData>>,
    /// Market statistics
    pub market_stats: HashMap<InstrumentKey, MarketStatistics>,
    /// Last update timestamps
    pub last_updates: HashMap<InstrumentKey, DateTime<Utc>>,
}

/// Instrument identifier for internal cache keys
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstrumentKey {
    pub exchange: ExchangeId,
    pub instrument: InstrumentNameExchange,
}

/// Real-time price data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    /// Current bid price
    pub bid: Decimal,
    /// Current ask price
    pub ask: Decimal,
    /// Last trade price
    pub last: Decimal,
    /// 24h volume
    pub volume_24h: Decimal,
    /// 24h price change
    pub change_24h: Decimal,
    /// Timestamp of last update
    pub timestamp: DateTime<Utc>,
}

/// Order book snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    /// Bid levels (price, quantity)
    pub bids: Vec<(Decimal, Decimal)>,
    /// Ask levels (price, quantity)
    pub asks: Vec<(Decimal, Decimal)>,
    /// Sequence number for ordering updates
    pub sequence: u64,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Trade data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeData {
    /// Trade ID
    pub id: String,
    /// Trade price
    pub price: Decimal,
    /// Trade quantity
    pub quantity: Decimal,
    /// Trade side (Buy/Sell)
    pub side: TradeSide,
    /// Trade timestamp
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradeSide {
    Buy,
    Sell,
}

/// Market statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketStatistics {
    /// High price (24h)
    pub high_24h: Decimal,
    /// Low price (24h)
    pub low_24h: Decimal,
    /// Opening price (24h)
    pub open_24h: Decimal,
    /// Total volume (24h)
    pub volume_24h: Decimal,
    /// Volume-weighted average price
    pub vwap: Decimal,
    /// Number of trades (24h)
    pub trade_count_24h: u64,
}

/// Market data update event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketDataUpdate {
    /// Price update
    Price {
        instrument: InstrumentKey,
        data: PriceData,
    },
    /// Order book update
    OrderBook {
        instrument: InstrumentKey,
        snapshot: OrderBookSnapshot,
    },
    /// Trade update
    Trade {
        instrument: InstrumentKey,
        trade: TradeData,
    },
    /// Market statistics update
    Statistics {
        instrument: InstrumentKey,
        stats: MarketStatistics,
    },
    /// Connection status update
    ConnectionStatus {
        exchange: ExchangeId,
        status: ConnectionStatus,
    },
}

/// Connection status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    Reconnecting,
    Error(String),
}

/// Data collection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataCollectionConfig {
    /// Update frequency for polling exchanges (milliseconds)
    pub update_frequency_ms: u64,
    /// Maximum cache size per instrument
    pub max_cache_size: usize,
    /// Enable real-time order book updates
    pub enable_order_book_updates: bool,
    /// Enable trade stream
    pub enable_trade_stream: bool,
    /// Enable market statistics
    pub enable_market_statistics: bool,
    /// Reconnection settings
    pub reconnection: ReconnectionConfig,
    /// Data retention settings
    pub retention: DataRetentionConfig,
}

impl Default for DataCollectionConfig {
    fn default() -> Self {
        Self {
            update_frequency_ms: 1000,
            max_cache_size: 1000,
            enable_order_book_updates: true,
            enable_trade_stream: true,
            enable_market_statistics: true,
            reconnection: ReconnectionConfig::default(),
            retention: DataRetentionConfig::default(),
        }
    }
}

/// Reconnection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectionConfig {
    /// Maximum reconnection attempts
    pub max_attempts: u32,
    /// Base delay between attempts (milliseconds)
    pub base_delay_ms: u64,
    /// Maximum delay between attempts (milliseconds)
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for ReconnectionConfig {
    fn default() -> Self {
        Self {
            max_attempts: 10,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
        }
    }
}

/// Data retention configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRetentionConfig {
    /// Maximum age for trade data (seconds)
    pub trade_retention_seconds: u64,
    /// Maximum number of recent trades to keep
    pub max_recent_trades: usize,
    /// Order book snapshot retention (seconds)
    pub orderbook_retention_seconds: u64,
}

impl Default for DataRetentionConfig {
    fn default() -> Self {
        Self {
            trade_retention_seconds: 3600, // 1 hour
            max_recent_trades: 100,
            orderbook_retention_seconds: 300, // 5 minutes
        }
    }
}

/// Market data subscription
#[derive(Debug, Clone)]
pub struct MarketDataSubscription {
    /// Instrument to subscribe to
    pub instrument: InstrumentNameExchange,
    /// Data types to collect
    pub data_types: Vec<DataType>,
    /// Subscription timestamp
    pub subscribed_at: DateTime<Utc>,
}

/// Data type enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataType {
    Price,
    OrderBook,
    Trades,
    Statistics,
}

/// Collection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStatistics {
    /// Total updates received
    pub total_updates: u64,
    /// Updates per exchange
    pub updates_by_exchange: HashMap<ExchangeId, u64>,
    /// Updates per instrument
    pub updates_by_instrument: HashMap<InstrumentKey, u64>,
    /// Average update latency (milliseconds)
    pub avg_latency_ms: f64,
    /// Connection uptime percentage
    pub uptime_percentage: f64,
    /// Last statistics update
    pub last_updated: DateTime<Utc>,
}

impl Default for CollectionStatistics {
    fn default() -> Self {
        Self {
            total_updates: 0,
            updates_by_exchange: HashMap::new(),
            updates_by_instrument: HashMap::new(),
            avg_latency_ms: 0.0,
            uptime_percentage: 0.0,
            last_updated: Utc::now(),
        }
    }
}

/// Connector error types
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("Subscription failed: {0}")]
    SubscriptionFailed(String),
    #[error("Data parsing error: {0}")]
    DataParsingError(String),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Exchange API error: {0}")]
    ExchangeApiError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
}

impl MarketDataCollector {
    /// Create new market data collector
    pub fn new(config: DataCollectionConfig) -> Self {
        let (data_sender, _) = broadcast::channel(10000);

        Self {
            exchange_connectors: HashMap::new(),
            market_data_cache: Arc::new(RwLock::new(MarketDataCache {
                latest_prices: HashMap::new(),
                order_books: HashMap::new(),
                recent_trades: HashMap::new(),
                market_stats: HashMap::new(),
                last_updates: HashMap::new(),
            })),
            data_broadcaster: data_sender,
            config,
            stats: Arc::new(RwLock::new(CollectionStatistics::default())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add exchange connector
    pub async fn add_exchange_connector(
        &mut self,
        connector: Arc<dyn ExchangeConnector + Send + Sync>,
    ) -> Result<(), ConnectorError> {
        let exchange_id = connector.exchange_id();
        info!("Adding exchange connector for {}", exchange_id);

        self.exchange_connectors.insert(exchange_id, connector);
        Ok(())
    }

    /// Start data collection
    pub async fn start(&mut self) -> Result<(), ConnectorError> {
        info!("Starting market data collection");

        // Connect to all exchanges
        for (exchange_id, connector) in &self.exchange_connectors {
            info!("Connecting to exchange: {}", exchange_id);
            match connector.connect().await {
                Ok(()) => {
                    info!("Successfully connected to {}", exchange_id);
                }
                Err(e) => {
                    error!("Failed to connect to {}: {}", exchange_id, e);
                    return Err(e);
                }
            }
        }

        // Start periodic statistics updates
        self.start_statistics_updater().await;

        // Start data cleanup task
        self.start_data_cleanup_task().await;

        info!("Market data collection started successfully");
        Ok(())
    }

    /// Subscribe to market data for specific instrument
    pub async fn subscribe_instrument(
        &self,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        data_types: Vec<DataType>,
    ) -> Result<(), ConnectorError> {
        let connector = self.exchange_connectors.get(&exchange).ok_or_else(|| {
            ConnectorError::ConfigurationError(format!(
                "No connector found for exchange: {}",
                exchange
            ))
        })?;

        // Subscribe to different data types
        for data_type in &data_types {
            match data_type {
                DataType::Price | DataType::Statistics => {
                    connector
                        .subscribe_market_data(vec![instrument.clone()])
                        .await?;
                }
                DataType::OrderBook => {
                    connector
                        .subscribe_order_book(vec![instrument.clone()])
                        .await?;
                }
                DataType::Trades => {
                    connector.subscribe_trades(vec![instrument.clone()]).await?;
                }
            }
        }

        // Track subscription
        let subscription = MarketDataSubscription {
            instrument: instrument.clone(),
            data_types,
            subscribed_at: Utc::now(),
        };

        let mut subscriptions = self.subscriptions.write().await;
        subscriptions
            .entry(exchange)
            .or_insert_with(Vec::new)
            .push(subscription);

        info!("Subscribed to {} on {}", instrument, exchange);
        Ok(())
    }

    /// Get current market data for instrument
    pub async fn get_market_data(&self, instrument_key: &InstrumentKey) -> Option<PriceData> {
        let cache = self.market_data_cache.read().await;
        cache.latest_prices.get(instrument_key).cloned()
    }

    /// Get current order book
    pub async fn get_order_book(
        &self,
        instrument_key: &InstrumentKey,
    ) -> Option<OrderBookSnapshot> {
        let cache = self.market_data_cache.read().await;
        cache.order_books.get(instrument_key).cloned()
    }

    /// Get recent trades
    pub async fn get_recent_trades(
        &self,
        instrument_key: &InstrumentKey,
    ) -> Option<Vec<TradeData>> {
        let cache = self.market_data_cache.read().await;
        cache.recent_trades.get(instrument_key).cloned()
    }

    /// Subscribe to market data updates
    pub fn subscribe_updates(&self) -> broadcast::Receiver<MarketDataUpdate> {
        self.data_broadcaster.subscribe()
    }

    /// Get collection statistics
    pub async fn get_statistics(&self) -> CollectionStatistics {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Update market data cache with new data
    pub async fn update_price_data(
        &self,
        instrument_key: InstrumentKey,
        price_data: PriceData,
    ) -> Result<(), ConnectorError> {
        // Update cache
        {
            let mut cache = self.market_data_cache.write().await;
            cache
                .latest_prices
                .insert(instrument_key.clone(), price_data.clone());
            cache
                .last_updates
                .insert(instrument_key.clone(), Utc::now());
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_updates += 1;
            *stats
                .updates_by_exchange
                .entry(instrument_key.exchange)
                .or_insert(0) += 1;
            *stats
                .updates_by_instrument
                .entry(instrument_key.clone())
                .or_insert(0) += 1;
        }

        // Broadcast update
        let update = MarketDataUpdate::Price {
            instrument: instrument_key,
            data: price_data,
        };

        let _ = self.data_broadcaster.send(update);
        Ok(())
    }

    /// Update order book
    pub async fn update_order_book(
        &self,
        instrument_key: InstrumentKey,
        order_book: OrderBookSnapshot,
    ) -> Result<(), ConnectorError> {
        // Update cache
        {
            let mut cache = self.market_data_cache.write().await;
            cache
                .order_books
                .insert(instrument_key.clone(), order_book.clone());
            cache
                .last_updates
                .insert(instrument_key.clone(), Utc::now());
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_updates += 1;
            *stats
                .updates_by_exchange
                .entry(instrument_key.exchange)
                .or_insert(0) += 1;
            *stats
                .updates_by_instrument
                .entry(instrument_key.clone())
                .or_insert(0) += 1;
        }

        // Broadcast update
        let update = MarketDataUpdate::OrderBook {
            instrument: instrument_key,
            snapshot: order_book,
        };

        let _ = self.data_broadcaster.send(update);
        Ok(())
    }

    /// Add trade data
    pub async fn add_trade_data(
        &self,
        instrument_key: InstrumentKey,
        trade: TradeData,
    ) -> Result<(), ConnectorError> {
        // Update cache
        {
            let mut cache = self.market_data_cache.write().await;
            let trades = cache
                .recent_trades
                .entry(instrument_key.clone())
                .or_insert_with(Vec::new);
            trades.push(trade.clone());

            // Keep only recent trades
            if trades.len() > self.config.retention.max_recent_trades {
                trades.remove(0);
            }

            cache
                .last_updates
                .insert(instrument_key.clone(), Utc::now());
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_updates += 1;
            *stats
                .updates_by_exchange
                .entry(instrument_key.exchange)
                .or_insert(0) += 1;
            *stats
                .updates_by_instrument
                .entry(instrument_key.clone())
                .or_insert(0) += 1;
        }

        // Broadcast update
        let update = MarketDataUpdate::Trade {
            instrument: instrument_key,
            trade,
        };

        let _ = self.data_broadcaster.send(update);
        Ok(())
    }

    /// Start periodic statistics updates
    async fn start_statistics_updater(&self) {
        let stats = Arc::clone(&self.stats);
        let mut interval = interval(Duration::from_secs(60)); // Update every minute

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                let mut stats_guard = stats.write().await;
                stats_guard.last_updated = Utc::now();
                // Calculate uptime, latency, etc.
                // This is a simplified implementation
                stats_guard.uptime_percentage = 95.0; // Stub value
                stats_guard.avg_latency_ms = 10.0; // Stub value
            }
        });
    }

    /// Start data cleanup task
    async fn start_data_cleanup_task(&self) {
        let cache = Arc::clone(&self.market_data_cache);
        let retention_config = self.config.retention.clone();
        let mut interval = interval(Duration::from_secs(300)); // Clean up every 5 minutes

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                let mut cache_guard = cache.write().await;
                let now = Utc::now();

                // Clean up old trade data
                for trades in cache_guard.recent_trades.values_mut() {
                    trades.retain(|trade| {
                        now.signed_duration_since(trade.timestamp).num_seconds()
                            < retention_config.trade_retention_seconds as i64
                    });
                }

                // Clean up old order book snapshots
                cache_guard.order_books.retain(|_key, snapshot| {
                    now.signed_duration_since(snapshot.timestamp).num_seconds()
                        < retention_config.orderbook_retention_seconds as i64
                });

                debug!("Completed data cleanup cycle");
            }
        });
    }

    /// Stop data collection
    pub async fn stop(&self) -> Result<(), ConnectorError> {
        info!("Stopping market data collection");

        // Disconnect from all exchanges
        for (exchange_id, connector) in &self.exchange_connectors {
            info!("Disconnecting from exchange: {}", exchange_id);
            if let Err(e) = connector.disconnect().await {
                warn!("Error disconnecting from {}: {}", exchange_id, e);
            }
        }

        info!("Market data collection stopped");
        Ok(())
    }
}

impl InstrumentKey {
    pub fn new(exchange: ExchangeId, instrument: InstrumentNameExchange) -> Self {
        Self {
            exchange,
            instrument,
        }
    }
}

/// Mock exchange connector for testing and development
#[derive(Debug)]
pub struct MockExchangeConnector {
    exchange_id: ExchangeId,
    connection_status: Arc<RwLock<ConnectionStatus>>,
    market_data_collector: Option<Arc<MarketDataCollector>>,
}

impl MockExchangeConnector {
    pub fn new(exchange_id: ExchangeId) -> Self {
        Self {
            exchange_id,
            connection_status: Arc::new(RwLock::new(ConnectionStatus::Disconnected)),
            market_data_collector: None,
        }
    }

    /// Start mock data generation
    pub async fn start_mock_data_generation(&self) {
        if let Some(collector) = &self.market_data_collector {
            let collector = Arc::clone(collector);
            let exchange_id = self.exchange_id;

            tokio::spawn(async move {
                let mut interval = interval(Duration::from_millis(1000));
                let instrument = InstrumentNameExchange::from("BTC/USD");
                let instrument_key = InstrumentKey::new(exchange_id, instrument);

                loop {
                    interval.tick().await;

                    // Generate mock price data
                    let price_data = PriceData {
                        bid: Decimal::new(50000, 0),
                        ask: Decimal::new(50001, 0),
                        last: Decimal::new(50000, 0),
                        volume_24h: Decimal::new(1000, 0),
                        change_24h: Decimal::new(250, 0),
                        timestamp: Utc::now(),
                    };

                    let _ = collector
                        .update_price_data(instrument_key.clone(), price_data)
                        .await;
                }
            });
        }
    }
}

impl ExchangeConnector for MockExchangeConnector {
    fn connect<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>> {
        Box::pin(async move {
            info!("Connecting to mock exchange: {}", self.exchange_id);
            let mut status = self.connection_status.write().await;
            *status = ConnectionStatus::Connected;
            Ok(())
        })
    }

    fn disconnect<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>> {
        Box::pin(async move {
            info!("Disconnecting from mock exchange: {}", self.exchange_id);
            let mut status = self.connection_status.write().await;
            *status = ConnectionStatus::Disconnected;
            Ok(())
        })
    }

    fn subscribe_market_data<'a>(
        &'a self,
        instruments: Vec<InstrumentNameExchange>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>> {
        Box::pin(async move {
            info!(
                "Subscribing to market data for {} instruments on {}",
                instruments.len(),
                self.exchange_id
            );
            Ok(())
        })
    }

    fn subscribe_order_book<'a>(
        &'a self,
        instruments: Vec<InstrumentNameExchange>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>> {
        Box::pin(async move {
            info!(
                "Subscribing to order books for {} instruments on {}",
                instruments.len(),
                self.exchange_id
            );
            Ok(())
        })
    }

    fn subscribe_trades<'a>(
        &'a self,
        instruments: Vec<InstrumentNameExchange>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + 'a>> {
        Box::pin(async move {
            info!(
                "Subscribing to trades for {} instruments on {}",
                instruments.len(),
                self.exchange_id
            );
            Ok(())
        })
    }

    fn connection_status(&self) -> ConnectionStatus {
        // This is a simplified synchronous access for demo purposes
        ConnectionStatus::Connected
    }

    fn exchange_id(&self) -> ExchangeId {
        self.exchange_id
    }
}
