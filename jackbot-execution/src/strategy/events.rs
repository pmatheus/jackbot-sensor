use async_trait::async_trait;
use jackbot_data::books::aggregator::OrderBookAggregator;
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};
use rust_decimal::Decimal;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{broadcast, mpsc, RwLock},
    time::timeout,
};
use tracing::{error, info, warn};

/// Maximum time allowed for strategy evaluation (50ms target)
pub const MAX_STRATEGY_EVALUATION_TIME: Duration = Duration::from_millis(50);

/// Market event types for high-frequency trading
#[derive(Debug, Clone, PartialEq)]
pub enum MarketEvent {
    /// Order book update
    OrderBookUpdate {
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        timestamp: u64,
        bids: Vec<(Decimal, Decimal)>,
        asks: Vec<(Decimal, Decimal)>,
    },
    /// Trade execution
    Trade {
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        timestamp: u64,
        price: Decimal,
        volume: Decimal,
        side: TradeSide,
    },
    /// Price tick update
    PriceTick {
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        timestamp: u64,
        price: Decimal,
        volume: Decimal,
    },
    /// Spread change
    SpreadChange {
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        timestamp: u64,
        bid: Decimal,
        ask: Decimal,
        prev_spread: Decimal,
        new_spread: Decimal,
    },
    /// Volume spike detection
    VolumeSpike {
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        timestamp: u64,
        volume: Decimal,
        threshold_multiplier: f64,
    },
    /// Latency measurement
    LatencyUpdate {
        exchange: ExchangeId,
        latency_ms: f64,
        timestamp: u64,
    },
    /// System health event
    SystemHealth {
        exchange: ExchangeId,
        is_healthy: bool,
        timestamp: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TradeSide {
    Buy,
    Sell,
}

/// Strategy signal generated in response to market events
#[derive(Debug, Clone)]
pub enum StrategySignal {
    /// Execute a strategy with specific parameters
    Execute {
        strategy_id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        parameters: StrategyParameters,
        urgency: SignalUrgency,
        timestamp: u64,
    },
    /// Cancel existing orders
    Cancel {
        strategy_id: String,
        orders: Vec<String>,
        timestamp: u64,
    },
    /// Update strategy parameters
    UpdateParameters {
        strategy_id: String,
        parameters: StrategyParameters,
        timestamp: u64,
    },
    /// Request position adjustment
    AdjustPosition {
        strategy_id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        target_quantity: Decimal,
        timestamp: u64,
    },
}

#[derive(Debug, Clone)]
pub enum SignalUrgency {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct StrategyParameters {
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub side: TradeSide,
    pub strategy_type: String,
    pub custom_params: HashMap<String, serde_json::Value>,
}

/// Event subscription filter for selective processing
#[derive(Debug, Clone)]
pub struct EventFilter {
    /// Exchanges to monitor
    pub exchanges: Option<Vec<ExchangeId>>,
    /// Instruments to monitor
    pub instruments: Option<Vec<InstrumentNameExchange>>,
    /// Event types to subscribe to
    pub event_types: Vec<MarketEventType>,
    /// Minimum volume threshold for trades
    pub min_volume: Option<Decimal>,
    /// Maximum latency for real-time processing
    pub max_latency_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarketEventType {
    OrderBookUpdate,
    Trade,
    PriceTick,
    SpreadChange,
    VolumeSpike,
    LatencyUpdate,
    SystemHealth,
}

/// Performance metrics for strategy evaluation
#[derive(Debug, Clone)]
pub struct StrategyMetrics {
    pub evaluation_time_us: u64,
    pub events_processed: u64,
    pub signals_generated: u64,
    pub errors: u64,
    pub last_update: Instant,
}

/// Event-driven strategy trait for high-frequency operations
#[async_trait]
pub trait EventDrivenStrategy: Send + Sync {
    /// Strategy identifier
    fn id(&self) -> &str;

    /// Get event subscription filter
    fn event_filter(&self) -> EventFilter;

    /// Process market event and generate strategy signals
    /// Must complete within MAX_STRATEGY_EVALUATION_TIME
    async fn process_event(
        &mut self,
        event: &MarketEvent,
        context: &StrategyContext,
    ) -> Result<Vec<StrategySignal>, StrategyError>;

    /// Initialize strategy with market data
    async fn initialize(&mut self, context: &StrategyContext) -> Result<(), StrategyError>;

    /// Clean up strategy resources
    async fn shutdown(&mut self) -> Result<(), StrategyError>;

    /// Get current strategy metrics
    fn metrics(&self) -> StrategyMetrics;

    /// Health check for strategy
    async fn health_check(&self) -> bool {
        true // Default implementation
    }
}

/// Strategy execution context
#[derive(Debug, Clone)]
pub struct StrategyContext {
    /// Order book aggregators per exchange
    pub aggregators: Arc<RwLock<HashMap<ExchangeId, OrderBookAggregator>>>,
    /// Current market state
    pub market_state: Arc<RwLock<MarketState>>,
    /// Strategy configuration
    pub config: StrategyConfig,
    /// Performance tracking
    pub metrics: Arc<Mutex<HashMap<String, StrategyMetrics>>>,
}

/// Current market state snapshot
#[derive(Debug, Clone)]
pub struct MarketState {
    /// Best bid/ask per exchange/instrument
    pub quotes: HashMap<(ExchangeId, InstrumentNameExchange), (Option<Decimal>, Option<Decimal>)>,
    /// Recent trade history
    pub recent_trades: VecDeque<MarketEvent>,
    /// Volume profiles
    pub volume_profiles: HashMap<(ExchangeId, InstrumentNameExchange), VolumeProfile>,
    /// Latency measurements
    pub latencies: HashMap<ExchangeId, LatencyProfile>,
    /// Last update timestamp
    pub last_update: Instant,
}

#[derive(Debug, Clone)]
pub struct VolumeProfile {
    pub total_volume: Decimal,
    pub recent_volume: VecDeque<(Instant, Decimal)>,
    pub avg_volume_per_minute: Decimal,
    pub volume_spike_threshold: Decimal,
}

#[derive(Debug, Clone)]
pub struct LatencyProfile {
    pub current_latency_ms: f64,
    pub avg_latency_ms: f64,
    pub latency_history: VecDeque<(Instant, f64)>,
    pub is_healthy: bool,
}

/// Strategy configuration
#[derive(Debug, Clone)]
pub struct StrategyConfig {
    pub max_concurrent_strategies: usize,
    pub performance_monitoring: bool,
    pub error_recovery: bool,
    pub circuit_breaker_threshold: u64,
    pub custom_settings: HashMap<String, serde_json::Value>,
}

/// Strategy execution errors
#[derive(Debug, thiserror::Error)]
pub enum StrategyError {
    #[error("Strategy evaluation timeout")]
    Timeout,
    #[error("Invalid market data: {0}")]
    InvalidMarketData(String),
    #[error("Strategy initialization failed: {0}")]
    InitializationFailed(String),
    #[error("Signal generation failed: {0}")]
    SignalGenerationFailed(String),
    #[error("Circuit breaker triggered")]
    CircuitBreakerTriggered,
    #[error("Resource unavailable: {0}")]
    ResourceUnavailable(String),
}

/// High-performance event-driven strategy engine
pub struct EventDrivenStrategyEngine {
    /// Registered strategies
    strategies: Arc<RwLock<HashMap<String, Box<dyn EventDrivenStrategy>>>>,
    /// Event broadcaster
    event_sender: broadcast::Sender<MarketEvent>,
    /// Strategy signal sender
    signal_sender: mpsc::UnboundedSender<StrategySignal>,
    /// Strategy context
    context: StrategyContext,
    /// Performance metrics
    metrics: Arc<Mutex<EngineMetrics>>,
    /// Circuit breaker state
    circuit_breaker: Arc<Mutex<CircuitBreakerState>>,
}

impl std::fmt::Debug for EventDrivenStrategyEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventDrivenStrategyEngine")
            .field("strategies_count", &"<HashMap>")
            .field("event_sender", &"<broadcast::Sender>")
            .field("signal_sender", &"<mpsc::UnboundedSender>")
            .field("context", &"<StrategyContext>")
            .field("metrics", &self.metrics)
            .field("circuit_breaker", &self.circuit_breaker)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct EngineMetrics {
    pub events_processed: u64,
    pub signals_generated: u64,
    pub strategies_active: u64,
    pub avg_processing_time_us: f64,
    pub errors: u64,
    pub circuit_breaker_trips: u64,
    pub start_time: Instant,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerState {
    pub error_count: u64,
    pub last_error: Option<Instant>,
    pub is_open: bool,
    pub threshold: u64,
    pub recovery_time: Duration,
}

impl EventDrivenStrategyEngine {
    /// Create new strategy engine
    pub fn new(
        config: StrategyConfig,
        signal_sender: mpsc::UnboundedSender<StrategySignal>,
    ) -> Self {
        let (event_sender, _) = broadcast::channel(10000); // High-capacity channel

        let context = StrategyContext {
            aggregators: Arc::new(RwLock::new(HashMap::new())),
            market_state: Arc::new(RwLock::new(MarketState {
                quotes: HashMap::new(),
                recent_trades: VecDeque::new(),
                volume_profiles: HashMap::new(),
                latencies: HashMap::new(),
                last_update: Instant::now(),
            })),
            config: config.clone(),
            metrics: Arc::new(Mutex::new(HashMap::new())),
        };

        Self {
            strategies: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            signal_sender,
            context,
            metrics: Arc::new(Mutex::new(EngineMetrics {
                events_processed: 0,
                signals_generated: 0,
                strategies_active: 0,
                avg_processing_time_us: 0.0,
                errors: 0,
                circuit_breaker_trips: 0,
                start_time: Instant::now(),
            })),
            circuit_breaker: Arc::new(Mutex::new(CircuitBreakerState {
                error_count: 0,
                last_error: None,
                is_open: false,
                threshold: config.circuit_breaker_threshold,
                recovery_time: Duration::from_secs(60),
            })),
        }
    }

    /// Register a new strategy
    pub async fn register_strategy(
        &self,
        strategy: Box<dyn EventDrivenStrategy>,
    ) -> Result<(), StrategyError> {
        let strategy_id = strategy.id().to_string();

        // Initialize strategy
        let mut strategy_mut = strategy;
        strategy_mut.initialize(&self.context).await?;

        // Register with engine
        let mut strategies = self.strategies.write().await;
        strategies.insert(strategy_id.clone(), strategy_mut);

        // Start event processing for this strategy
        self.start_strategy_processor(strategy_id.clone()).await;

        info!(strategy_id = %strategy_id, "Strategy registered successfully");
        Ok(())
    }

    /// Start event processing for a specific strategy
    async fn start_strategy_processor(&self, strategy_id: String) {
        let strategies = self.strategies.clone();
        let event_receiver = self.event_sender.subscribe();
        let signal_sender = self.signal_sender.clone();
        let context = self.context.clone();
        let metrics = self.metrics.clone();
        let circuit_breaker = self.circuit_breaker.clone();
        let strategy_id_clone = strategy_id.clone();

        tokio::spawn(async move {
            let mut event_rx = event_receiver;

            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        // Check circuit breaker
                        {
                            let cb = circuit_breaker.lock().unwrap();
                            if cb.is_open {
                                if let Some(last_error) = cb.last_error {
                                    if last_error.elapsed() < cb.recovery_time {
                                        continue; // Skip processing while circuit breaker is open
                                    }
                                }
                            }
                        }

                        let process_start = Instant::now();

                        // Process event with timeout
                        let result = timeout(
                            MAX_STRATEGY_EVALUATION_TIME,
                            Self::process_strategy_event(
                                &strategies,
                                &strategy_id_clone,
                                &event,
                                &context,
                                &signal_sender,
                            ),
                        )
                        .await;

                        let processing_time = process_start.elapsed();

                        // Update metrics
                        {
                            let mut metrics = metrics.lock().unwrap();
                            metrics.events_processed += 1;

                            let processing_time_us = processing_time.as_micros() as f64;
                            metrics.avg_processing_time_us = (metrics.avg_processing_time_us
                                * (metrics.events_processed - 1) as f64
                                + processing_time_us)
                                / metrics.events_processed as f64;
                        }

                        match result {
                            Ok(Ok(_)) => {
                                // Success - reset circuit breaker error count
                                let mut cb = circuit_breaker.lock().unwrap();
                                cb.error_count = 0;
                                cb.is_open = false;
                            }
                            Ok(Err(e)) => {
                                error!(
                                    strategy_id = %strategy_id_clone,
                                    error = %e,
                                    processing_time_ms = processing_time.as_millis(),
                                    "Strategy processing error"
                                );
                                Self::handle_strategy_error(&circuit_breaker, &metrics);
                            }
                            Err(_) => {
                                warn!(
                                    strategy_id = %strategy_id_clone,
                                    processing_time_ms = processing_time.as_millis(),
                                    max_time_ms = MAX_STRATEGY_EVALUATION_TIME.as_millis(),
                                    "Strategy evaluation timeout"
                                );
                                Self::handle_strategy_error(&circuit_breaker, &metrics);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!(strategy_id = %strategy_id_clone, "Event channel closed, stopping processor");
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        warn!(
                            strategy_id = %strategy_id_clone,
                            missed_events = missed,
                            "Strategy processor lagged, missed events"
                        );
                        Self::handle_strategy_error(&circuit_breaker, &metrics);
                    }
                }
            }
        });
    }

    /// Process single event for a strategy
    async fn process_strategy_event(
        strategies: &Arc<RwLock<HashMap<String, Box<dyn EventDrivenStrategy>>>>,
        strategy_id: &str,
        event: &MarketEvent,
        context: &StrategyContext,
        signal_sender: &mpsc::UnboundedSender<StrategySignal>,
    ) -> Result<(), StrategyError> {
        let mut strategies_guard = strategies.write().await;

        if let Some(strategy) = strategies_guard.get_mut(strategy_id) {
            // Check if strategy should process this event
            let filter = strategy.event_filter();
            if !Self::event_matches_filter(event, &filter) {
                return Ok(());
            }

            // Process event and generate signals
            let signals = strategy.process_event(event, context).await?;

            // Drop the guard early to avoid hold conflicts
            drop(strategies_guard);

            // Send generated signals
            for signal in signals {
                if let Err(e) = signal_sender.send(signal) {
                    error!(
                        strategy_id = %strategy_id,
                        error = %e,
                        "Failed to send strategy signal"
                    );
                    return Err(StrategyError::SignalGenerationFailed(e.to_string()));
                }
            }
            Ok(())
        } else {
            Err(StrategyError::ResourceUnavailable(format!(
                "Strategy not found: {}",
                strategy_id
            )))
        }
    }

    /// Check if event matches strategy filter
    fn event_matches_filter(event: &MarketEvent, filter: &EventFilter) -> bool {
        // Check event type
        let event_type = match event {
            MarketEvent::OrderBookUpdate { .. } => MarketEventType::OrderBookUpdate,
            MarketEvent::Trade { .. } => MarketEventType::Trade,
            MarketEvent::PriceTick { .. } => MarketEventType::PriceTick,
            MarketEvent::SpreadChange { .. } => MarketEventType::SpreadChange,
            MarketEvent::VolumeSpike { .. } => MarketEventType::VolumeSpike,
            MarketEvent::LatencyUpdate { .. } => MarketEventType::LatencyUpdate,
            MarketEvent::SystemHealth { .. } => MarketEventType::SystemHealth,
        };

        if !filter.event_types.contains(&event_type) {
            return false;
        }

        // Check exchange filter
        let event_exchange = match event {
            MarketEvent::OrderBookUpdate { exchange, .. }
            | MarketEvent::Trade { exchange, .. }
            | MarketEvent::PriceTick { exchange, .. }
            | MarketEvent::SpreadChange { exchange, .. }
            | MarketEvent::VolumeSpike { exchange, .. }
            | MarketEvent::LatencyUpdate { exchange, .. }
            | MarketEvent::SystemHealth { exchange, .. } => exchange,
        };

        if let Some(ref exchanges) = filter.exchanges {
            if !exchanges.contains(event_exchange) {
                return false;
            }
        }

        // Check instrument filter
        if let Some(ref instruments) = filter.instruments {
            let event_instrument = match event {
                MarketEvent::OrderBookUpdate { instrument, .. }
                | MarketEvent::Trade { instrument, .. }
                | MarketEvent::PriceTick { instrument, .. }
                | MarketEvent::SpreadChange { instrument, .. }
                | MarketEvent::VolumeSpike { instrument, .. } => Some(instrument),
                _ => None,
            };

            if let Some(instrument) = event_instrument {
                if !instruments.contains(instrument) {
                    return false;
                }
            }
        }

        // Check volume filter
        if let Some(min_volume) = filter.min_volume {
            let event_volume = match event {
                MarketEvent::Trade { volume, .. } => Some(*volume),
                MarketEvent::PriceTick { volume, .. } => Some(*volume),
                MarketEvent::VolumeSpike { volume, .. } => Some(*volume),
                _ => None,
            };

            if let Some(volume) = event_volume {
                if volume < min_volume {
                    return false;
                }
            }
        }

        true
    }

    /// Handle strategy processing error
    fn handle_strategy_error(
        circuit_breaker: &Arc<Mutex<CircuitBreakerState>>,
        metrics: &Arc<Mutex<EngineMetrics>>,
    ) {
        // Update metrics
        {
            let mut metrics = metrics.lock().unwrap();
            metrics.errors += 1;
        }

        // Update circuit breaker
        {
            let mut cb = circuit_breaker.lock().unwrap();
            cb.error_count += 1;
            cb.last_error = Some(Instant::now());

            if cb.error_count >= cb.threshold {
                cb.is_open = true;
                warn!(
                    error_count = cb.error_count,
                    threshold = cb.threshold,
                    "Circuit breaker triggered - stopping strategy processing"
                );

                // Update metrics
                let mut metrics = metrics.lock().unwrap();
                metrics.circuit_breaker_trips += 1;
            }
        }
    }

    /// Publish market event to all strategies
    pub async fn publish_event(&self, event: MarketEvent) -> Result<(), StrategyError> {
        // Update market state
        self.update_market_state(&event).await;

        // Broadcast event to all strategy processors
        if let Err(_) = self.event_sender.send(event) {
            // All receivers have been dropped
            warn!("No active strategy processors to receive event");
        }

        Ok(())
    }

    /// Update internal market state
    async fn update_market_state(&self, event: &MarketEvent) {
        let mut market_state = self.context.market_state.write().await;

        match event {
            MarketEvent::OrderBookUpdate {
                exchange,
                instrument,
                bids,
                asks,
                timestamp: _,
            } => {
                let best_bid = bids.first().map(|(price, _)| *price);
                let best_ask = asks.first().map(|(price, _)| *price);
                market_state
                    .quotes
                    .insert((exchange.clone(), instrument.clone()), (best_bid, best_ask));
            }
            MarketEvent::Trade {
                exchange,
                instrument,
                volume,
                timestamp: _,
                ..
            } => {
                // Update volume profile
                let key = (exchange.clone(), instrument.clone());
                let profile =
                    market_state
                        .volume_profiles
                        .entry(key)
                        .or_insert_with(|| VolumeProfile {
                            total_volume: Decimal::ZERO,
                            recent_volume: VecDeque::new(),
                            avg_volume_per_minute: Decimal::ZERO,
                            volume_spike_threshold: Decimal::ZERO,
                        });

                profile.total_volume += *volume;
                profile.recent_volume.push_back((Instant::now(), *volume));

                // Keep only recent trades (last 5 minutes)
                let cutoff = Instant::now() - Duration::from_secs(300);
                profile.recent_volume.retain(|(time, _)| *time >= cutoff);

                // Add to recent trades
                market_state.recent_trades.push_back(event.clone());
                if market_state.recent_trades.len() > 1000 {
                    market_state.recent_trades.pop_front();
                }
            }
            MarketEvent::LatencyUpdate {
                exchange,
                latency_ms,
                ..
            } => {
                let profile = market_state
                    .latencies
                    .entry(exchange.clone())
                    .or_insert_with(|| LatencyProfile {
                        current_latency_ms: 0.0,
                        avg_latency_ms: 0.0,
                        latency_history: VecDeque::new(),
                        is_healthy: true,
                    });

                profile.current_latency_ms = *latency_ms;
                profile
                    .latency_history
                    .push_back((Instant::now(), *latency_ms));

                // Keep only recent measurements (last 10 minutes)
                let cutoff = Instant::now() - Duration::from_secs(600);
                profile.latency_history.retain(|(time, _)| *time >= cutoff);

                // Update average
                if !profile.latency_history.is_empty() {
                    profile.avg_latency_ms = profile
                        .latency_history
                        .iter()
                        .map(|(_, latency)| *latency)
                        .sum::<f64>()
                        / profile.latency_history.len() as f64;
                }

                profile.is_healthy = *latency_ms < 100.0; // Consider healthy if < 100ms
            }
            _ => {}
        }

        market_state.last_update = Instant::now();
    }

    /// Unregister a strategy
    pub async fn unregister_strategy(&self, strategy_id: &str) -> Result<(), StrategyError> {
        let mut strategies = self.strategies.write().await;

        let strategy_option = strategies.remove(strategy_id);
        match strategy_option {
            Some(mut strategy) => {
                strategy.shutdown().await?;
                info!(strategy_id = %strategy_id, "Strategy unregistered successfully");
                Ok(())
            }
            None => Err(StrategyError::ResourceUnavailable(format!(
                "Strategy not found: {}",
                strategy_id
            ))),
        }
    }

    /// Get engine metrics
    pub fn get_metrics(&self) -> EngineMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Get strategy metrics
    pub async fn get_strategy_metrics(&self) -> HashMap<String, StrategyMetrics> {
        let strategies = self.strategies.read().await;
        let mut metrics = HashMap::new();

        for (id, strategy) in strategies.iter() {
            metrics.insert(id.clone(), strategy.metrics());
        }

        metrics
    }

    /// Health check for all strategies
    pub async fn health_check(&self) -> bool {
        let strategies = self.strategies.read().await;

        for strategy in strategies.values() {
            if !strategy.health_check().await {
                return false;
            }
        }

        // Check circuit breaker
        let cb = self.circuit_breaker.lock().unwrap();
        !cb.is_open
    }
}

/// Utility function to get current timestamp in microseconds
pub fn current_timestamp_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

/// Utility function to get current timestamp in milliseconds
pub fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
