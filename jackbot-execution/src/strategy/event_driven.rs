use crate::{
    data_gathering::{InstrumentKey, MarketDataUpdate, PriceData},
    order::{
        id::{ClientOrderId, StrategyId},
        request::{OrderRequestOpen, RequestOpen},
        OrderKey, OrderKind, TimeInForce,
    },
    testing::{OrderUpdateEvent, TestOrderExecutionEngine},
};
use chrono::{DateTime, Utc};
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange, Side};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{broadcast, RwLock},
    time::interval,
};
use tracing::{debug, error, info, warn};

/// Event-driven trading strategy framework
#[derive(Debug)]
pub struct EventDrivenStrategy {
    /// Strategy configuration
    config: StrategyConfig,
    /// Strategy state
    state: Arc<RwLock<StrategyState>>,
    /// Market data subscription
    market_data_receiver: Option<broadcast::Receiver<MarketDataUpdate>>,
    /// Order execution engine
    execution_engine: Arc<TestOrderExecutionEngine>,
    /// Order updates receiver
    order_updates_receiver: Option<broadcast::Receiver<OrderUpdateEvent>>,
    /// Active signals
    active_signals: Arc<RwLock<HashMap<SignalId, TradingSignal>>>,
    /// Strategy metrics
    metrics: Arc<RwLock<StrategyMetrics>>,
    /// Event processors
    event_processors: Vec<Box<dyn EventProcessor + Send + Sync>>,
}

/// Strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Strategy ID
    pub strategy_id: StrategyId,
    /// Strategy name
    pub name: String,
    /// Target instruments
    pub instruments: Vec<StrategyInstrument>,
    /// Position sizing configuration
    pub position_sizing: PositionSizingConfig,
    /// Risk management settings
    pub risk_management: RiskManagementConfig,
    /// Signal processing settings
    pub signal_processing: SignalProcessingConfig,
    /// Execution settings
    pub execution: ExecutionConfig,
    /// Performance tracking
    pub performance: PerformanceConfig,
}

/// Strategy instrument configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyInstrument {
    /// Exchange
    pub exchange: ExchangeId,
    /// Instrument
    pub instrument: InstrumentNameExchange,
    /// Weight in portfolio (0.0 to 1.0)
    pub weight: f64,
    /// Maximum position size
    pub max_position: Decimal,
    /// Minimum order size
    pub min_order_size: Decimal,
    /// Active trading enabled
    pub enabled: bool,
}

/// Position sizing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSizingConfig {
    /// Base position size
    pub base_size: Decimal,
    /// Maximum position size
    pub max_size: Decimal,
    /// Size scaling method
    pub scaling_method: SizingMethod,
    /// Volatility-based sizing
    pub volatility_based: bool,
    /// Kelly criterion factor
    pub kelly_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SizingMethod {
    Fixed,
    PercentageOfEquity,
    VolatilityTargeted,
    KellyOptimal,
}

/// Risk management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskManagementConfig {
    /// Maximum drawdown limit
    pub max_drawdown: Decimal,
    /// Daily loss limit
    pub daily_loss_limit: Decimal,
    /// Maximum open positions
    pub max_open_positions: u32,
    /// Stop loss percentage
    pub stop_loss_pct: Option<f64>,
    /// Take profit percentage
    pub take_profit_pct: Option<f64>,
    /// Enable trailing stops
    pub enable_trailing_stops: bool,
    /// Correlation limits
    pub correlation_limits: bool,
}

/// Signal processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalProcessingConfig {
    /// Minimum signal strength
    pub min_signal_strength: f64,
    /// Signal timeout (seconds)
    pub signal_timeout_seconds: u64,
    /// Enable signal aggregation
    pub enable_aggregation: bool,
    /// Conflicting signals handling
    pub conflicting_signals: ConflictResolution,
    /// Signal filters
    pub filters: Vec<SignalFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictResolution {
    FirstWins,
    LastWins,
    StrongestWins,
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalFilter {
    pub filter_type: FilterType,
    pub parameters: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterType {
    MinimumVolume,
    MaximumSpread,
    TradingHours,
    MarketCondition,
}

/// Execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Order type preference
    pub preferred_order_type: OrderKind,
    /// Time in force
    pub time_in_force: TimeInForce,
    /// Allow partial fills
    pub allow_partial_fills: bool,
    /// Maximum slippage tolerance
    pub max_slippage_bps: u32,
    /// Order timeout (seconds)
    pub order_timeout_seconds: u64,
    /// Enable smart routing
    pub enable_smart_routing: bool,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Benchmark instrument
    pub benchmark: Option<InstrumentKey>,
    /// Track Sharpe ratio
    pub track_sharpe: bool,
    /// Track maximum drawdown
    pub track_max_drawdown: bool,
    /// Performance reporting frequency
    pub reporting_frequency_minutes: u64,
}

/// Strategy state
#[derive(Debug, Clone)]
pub struct StrategyState {
    /// Current positions
    pub positions: HashMap<InstrumentKey, Position>,
    /// Active orders
    pub active_orders: HashMap<ClientOrderId, ActiveOrder>,
    /// Current signals
    pub current_signals: HashMap<InstrumentKey, Vec<TradingSignal>>,
    /// Strategy status
    pub status: StrategyStatus,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
    /// Performance metrics
    pub current_pnl: Decimal,
    /// Maximum drawdown
    pub max_drawdown: Decimal,
    /// Total trades
    pub total_trades: u64,
    /// Winning trades
    pub winning_trades: u64,
}

/// Position information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Instrument
    pub instrument: InstrumentKey,
    /// Quantity (positive for long, negative for short)
    pub quantity: Decimal,
    /// Average entry price
    pub avg_entry_price: Decimal,
    /// Current market price
    pub current_price: Decimal,
    /// Unrealized PnL
    pub unrealized_pnl: Decimal,
    /// Realized PnL
    pub realized_pnl: Decimal,
    /// Position opened at
    pub opened_at: DateTime<Utc>,
    /// Last updated
    pub last_updated: DateTime<Utc>,
}

/// Active order tracking
#[derive(Debug, Clone)]
pub struct ActiveOrder {
    /// Order request
    pub request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    /// Submitted at
    pub submitted_at: DateTime<Utc>,
    /// Associated signal
    pub signal_id: Option<SignalId>,
    /// Order purpose
    pub purpose: OrderPurpose,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderPurpose {
    Entry,
    Exit,
    StopLoss,
    TakeProfit,
    Rebalance,
}

/// Strategy status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyStatus {
    Inactive,
    Starting,
    Active,
    Paused,
    Stopping,
    Error(String),
}

/// Trading signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingSignal {
    /// Signal ID
    pub signal_id: SignalId,
    /// Instrument
    pub instrument: InstrumentKey,
    /// Signal type
    pub signal_type: SignalType,
    /// Signal strength (0.0 to 1.0)
    pub strength: f64,
    /// Signal direction
    pub direction: SignalDirection,
    /// Suggested quantity
    pub suggested_quantity: Decimal,
    /// Suggested price
    pub suggested_price: Option<Decimal>,
    /// Signal timestamp
    pub timestamp: DateTime<Utc>,
    /// Signal source
    pub source: String,
    /// Signal metadata
    pub metadata: HashMap<String, String>,
    /// Expiry time
    pub expires_at: Option<DateTime<Utc>>,
}

/// Signal identifier
pub type SignalId = String;

/// Signal type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignalType {
    TechnicalIndicator,
    PriceAction,
    VolumeProfile,
    OrderBookImbalance,
    NewsTrending,
    ArbitrageOpportunity,
    MeanReversion,
    Momentum,
    Custom(String),
}

/// Signal direction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignalDirection {
    Buy,
    Sell,
    Hold,
    Close,
}

/// Strategy metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyMetrics {
    /// Total return
    pub total_return: Decimal,
    /// Annualized return
    pub annualized_return: f64,
    /// Sharpe ratio
    pub sharpe_ratio: f64,
    /// Maximum drawdown
    pub max_drawdown: Decimal,
    /// Win rate
    pub win_rate: f64,
    /// Average win
    pub avg_win: Decimal,
    /// Average loss
    pub avg_loss: Decimal,
    /// Profit factor
    pub profit_factor: f64,
    /// Total trades
    pub total_trades: u64,
    /// Last updated
    pub last_updated: DateTime<Utc>,
}

/// Event processor trait for handling different types of market events
pub trait EventProcessor: std::fmt::Debug + Send + Sync {
    /// Process market data update
    fn process_market_data<'a>(
        &'a mut self,
        update: &'a MarketDataUpdate,
        strategy_state: &'a mut StrategyState,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TradingSignal>, StrategyError>> + Send + 'a>>;

    /// Process order update
    fn process_order_update<'a>(
        &'a mut self,
        update: &'a OrderUpdateEvent,
        strategy_state: &'a mut StrategyState,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TradingSignal>, StrategyError>> + Send + 'a>>;

    /// Get processor name
    fn name(&self) -> &str;

    /// Get processor priority (higher priority processors run first)
    fn priority(&self) -> u32;
}

impl EventDrivenStrategy {
    /// Create new event-driven strategy
    pub fn new(config: StrategyConfig, execution_engine: Arc<TestOrderExecutionEngine>) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(StrategyState {
                positions: HashMap::new(),
                active_orders: HashMap::new(),
                current_signals: HashMap::new(),
                status: StrategyStatus::Inactive,
                last_updated: Utc::now(),
                current_pnl: Decimal::ZERO,
                max_drawdown: Decimal::ZERO,
                total_trades: 0,
                winning_trades: 0,
            })),
            market_data_receiver: None,
            execution_engine,
            order_updates_receiver: None,
            active_signals: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(StrategyMetrics {
                total_return: Decimal::ZERO,
                annualized_return: 0.0,
                sharpe_ratio: 0.0,
                max_drawdown: Decimal::ZERO,
                win_rate: 0.0,
                avg_win: Decimal::ZERO,
                avg_loss: Decimal::ZERO,
                profit_factor: 0.0,
                total_trades: 0,
                last_updated: Utc::now(),
            })),
            event_processors: Vec::new(),
        }
    }

    /// Add event processor
    pub fn add_event_processor(&mut self, processor: Box<dyn EventProcessor + Send + Sync>) {
        self.event_processors.push(processor);
        // Sort by priority (highest first)
        self.event_processors
            .sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    /// Set market data receiver
    pub fn set_market_data_receiver(&mut self, receiver: broadcast::Receiver<MarketDataUpdate>) {
        self.market_data_receiver = Some(receiver);
    }

    /// Set order updates receiver
    pub fn set_order_updates_receiver(&mut self, receiver: broadcast::Receiver<OrderUpdateEvent>) {
        self.order_updates_receiver = Some(receiver);
    }

    /// Start the strategy
    pub async fn start(&mut self) -> Result<(), StrategyError> {
        info!("Starting event-driven strategy: {}", self.config.name);

        // Update status
        {
            let mut state = self.state.write().await;
            state.status = StrategyStatus::Starting;
            state.last_updated = Utc::now();
        }

        // Start market data processing
        self.start_market_data_processing().await?;

        // Start order update processing
        self.start_order_update_processing().await?;

        // Start performance monitoring
        self.start_performance_monitoring().await;

        // Start signal timeout monitoring
        self.start_signal_timeout_monitoring().await;

        // Update status to active
        {
            let mut state = self.state.write().await;
            state.status = StrategyStatus::Active;
            state.last_updated = Utc::now();
        }

        info!("Event-driven strategy started: {}", self.config.name);
        Ok(())
    }

    /// Start market data processing task
    async fn start_market_data_processing(&mut self) -> Result<(), StrategyError> {
        if let Some(mut receiver) = self.market_data_receiver.take() {
            let state = Arc::clone(&self.state);
            let signals = Arc::clone(&self.active_signals);
            let mut processors = std::mem::take(&mut self.event_processors);
            let execution_engine = Arc::clone(&self.execution_engine);
            let config = self.config.clone();

            tokio::spawn(async move {
                while let Ok(market_update) = receiver.recv().await {
                    // Process with each event processor
                    for processor in &mut processors {
                        let mut state_guard = state.write().await;
                        let process_result = processor
                            .process_market_data(&market_update, &mut state_guard)
                            .await;
                        match process_result {
                            Ok(new_signals) => {
                                // Handle new signals
                                for signal in new_signals {
                                    if let Err(e) = Self::handle_trading_signal(
                                        &signal,
                                        &state,
                                        &signals,
                                        &execution_engine,
                                        &config,
                                    )
                                    .await
                                    {
                                        error!("Error handling signal: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!(
                                    "Error processing market data with {}: {}",
                                    processor.name(),
                                    e
                                );
                            }
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Start order update processing task
    async fn start_order_update_processing(&mut self) -> Result<(), StrategyError> {
        if let Some(mut receiver) = self.order_updates_receiver.take() {
            let state = Arc::clone(&self.state);
            let _signals = Arc::clone(&self.active_signals);
            let mut _processors: Vec<Box<dyn EventProcessor>> = Vec::new(); // We'd need to clone processors here in a real implementation
            let _execution_engine = Arc::clone(&self.execution_engine);
            let _config = self.config.clone();

            tokio::spawn(async move {
                while let Ok(order_update) = receiver.recv().await {
                    // Update state based on order updates
                    let mut state_guard = state.write().await;

                    match &order_update {
                        OrderUpdateEvent::OrderFilled {
                            order_id,
                            execution_result,
                            ..
                        } => {
                            // Remove from active orders
                            state_guard.active_orders.remove(order_id);

                            // Update positions
                            let instrument_key = InstrumentKey::new(
                                execution_result.exchange,
                                execution_result.instrument.clone(),
                            );

                            let position = state_guard
                                .positions
                                .entry(instrument_key.clone())
                                .or_insert(Position {
                                    instrument: instrument_key,
                                    quantity: Decimal::ZERO,
                                    avg_entry_price: Decimal::ZERO,
                                    current_price: Decimal::ZERO,
                                    unrealized_pnl: Decimal::ZERO,
                                    realized_pnl: Decimal::ZERO,
                                    opened_at: Utc::now(),
                                    last_updated: Utc::now(),
                                });

                            // Update position based on fill
                            match execution_result.side {
                                Side::Buy => position.quantity += execution_result.filled_quantity,
                                Side::Sell => position.quantity -= execution_result.filled_quantity,
                            }

                            position.last_updated = Utc::now();
                            state_guard.total_trades += 1;
                        }
                        OrderUpdateEvent::OrderCancelled { order_id, .. }
                        | OrderUpdateEvent::OrderRejected { order_id, .. } => {
                            // Remove from active orders
                            state_guard.active_orders.remove(order_id);
                        }
                        _ => {}
                    }

                    state_guard.last_updated = Utc::now();
                }
            });
        }

        Ok(())
    }

    /// Start performance monitoring task
    async fn start_performance_monitoring(&self) {
        let state = Arc::clone(&self.state);
        let metrics = Arc::clone(&self.metrics);
        let frequency =
            Duration::from_secs(self.config.performance.reporting_frequency_minutes * 60);
        let mut interval = interval(frequency);

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                let state_guard = state.read().await;
                let mut metrics_guard = metrics.write().await;

                // Calculate performance metrics
                let total_pnl = state_guard
                    .positions
                    .values()
                    .map(|p| p.unrealized_pnl + p.realized_pnl)
                    .sum::<Decimal>();

                metrics_guard.total_return = total_pnl;
                metrics_guard.max_drawdown = state_guard.max_drawdown;
                metrics_guard.total_trades = state_guard.total_trades;

                if state_guard.total_trades > 0 {
                    metrics_guard.win_rate =
                        state_guard.winning_trades as f64 / state_guard.total_trades as f64;
                }

                metrics_guard.last_updated = Utc::now();

                debug!(
                    "Updated strategy metrics: PnL = {}, Total Trades = {}",
                    metrics_guard.total_return, metrics_guard.total_trades
                );
            }
        });
    }

    /// Start signal timeout monitoring
    async fn start_signal_timeout_monitoring(&self) {
        let signals = Arc::clone(&self.active_signals);
        let mut interval = interval(Duration::from_secs(30)); // Check every 30 seconds

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                let now = Utc::now();
                let mut signals_guard = signals.write().await;

                // Remove expired signals
                signals_guard
                    .retain(|_, signal| signal.expires_at.map_or(true, |expiry| now < expiry));
            }
        });
    }

    /// Handle trading signal
    async fn handle_trading_signal(
        signal: &TradingSignal,
        state: &Arc<RwLock<StrategyState>>,
        signals: &Arc<RwLock<HashMap<SignalId, TradingSignal>>>,
        execution_engine: &Arc<TestOrderExecutionEngine>,
        config: &StrategyConfig,
    ) -> Result<(), StrategyError> {
        // Check signal strength
        if signal.strength < config.signal_processing.min_signal_strength {
            debug!(
                "Ignoring weak signal: strength {} < {}",
                signal.strength, config.signal_processing.min_signal_strength
            );
            return Ok(());
        }

        // Store signal
        {
            let mut signals_guard = signals.write().await;
            signals_guard.insert(signal.signal_id.clone(), signal.clone());
        }

        // Determine order parameters
        let (side, order_type) = match signal.direction {
            SignalDirection::Buy => (Side::Buy, config.execution.preferred_order_type),
            SignalDirection::Sell => (Side::Sell, config.execution.preferred_order_type),
            SignalDirection::Hold => return Ok(()), // No action for hold signals
            SignalDirection::Close => {
                // Handle position closing logic
                return Self::handle_close_signal(signal, state, execution_engine, config).await;
            }
        };

        // Create order request
        let order_request = OrderRequestOpen {
            key: OrderKey {
                exchange: signal.instrument.exchange,
                instrument: signal.instrument.instrument.clone(),
                strategy: config.strategy_id.clone(),
                cid: ClientOrderId::new(
                    "signal_order_".to_string() + &uuid::Uuid::new_v4().to_string()[..8],
                ),
            },
            state: RequestOpen {
                side,
                price: signal.suggested_price.unwrap_or(Decimal::ZERO),
                quantity: signal.suggested_quantity,
                kind: order_type,
                time_in_force: config.execution.time_in_force,
            },
        };

        // Submit order
        match execution_engine.submit_order(order_request.clone()).await {
            Ok(order_id) => {
                // Track active order
                let mut state_guard = state.write().await;
                state_guard.active_orders.insert(
                    order_id.clone(),
                    ActiveOrder {
                        request: order_request,
                        submitted_at: Utc::now(),
                        signal_id: Some(signal.signal_id.clone()),
                        purpose: OrderPurpose::Entry,
                    },
                );

                info!(
                    "Submitted order {} for signal {}",
                    order_id, signal.signal_id
                );
            }
            Err(e) => {
                error!(
                    "Failed to submit order for signal {}: {}",
                    signal.signal_id, e
                );
                return Err(StrategyError::OrderSubmissionFailed(e.to_string()));
            }
        }

        Ok(())
    }

    /// Handle close signal
    async fn handle_close_signal(
        signal: &TradingSignal,
        state: &Arc<RwLock<StrategyState>>,
        execution_engine: &Arc<TestOrderExecutionEngine>,
        config: &StrategyConfig,
    ) -> Result<(), StrategyError> {
        let state_guard = state.read().await;

        if let Some(position) = state_guard.positions.get(&signal.instrument) {
            if position.quantity != Decimal::ZERO {
                let close_side = if position.quantity > Decimal::ZERO {
                    Side::Sell
                } else {
                    Side::Buy
                };

                let close_quantity = position.quantity.abs();

                let order_request = OrderRequestOpen {
                    key: OrderKey {
                        exchange: signal.instrument.exchange,
                        instrument: signal.instrument.instrument.clone(),
                        strategy: config.strategy_id.clone(),
                        cid: ClientOrderId::new(
                            "close_order_".to_string() + &uuid::Uuid::new_v4().to_string()[..8],
                        ),
                    },
                    state: RequestOpen {
                        side: close_side,
                        price: signal.suggested_price.unwrap_or(Decimal::ZERO),
                        quantity: close_quantity,
                        kind: OrderKind::Market, // Use market orders for closing
                        time_in_force: TimeInForce::ImmediateOrCancel,
                    },
                };

                match execution_engine.submit_order(order_request).await {
                    Ok(order_id) => {
                        info!(
                            "Submitted close order {} for position in {}",
                            order_id, signal.instrument.instrument
                        );
                    }
                    Err(e) => {
                        error!("Failed to submit close order: {}", e);
                        return Err(StrategyError::OrderSubmissionFailed(e.to_string()));
                    }
                }
            }
        }

        Ok(())
    }

    /// Get current strategy state
    pub async fn get_state(&self) -> StrategyState {
        let state = self.state.read().await;
        state.clone()
    }

    /// Get strategy metrics
    pub async fn get_metrics(&self) -> StrategyMetrics {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    /// Stop the strategy
    pub async fn stop(&mut self) -> Result<(), StrategyError> {
        info!("Stopping event-driven strategy: {}", self.config.name);

        let mut state = self.state.write().await;
        state.status = StrategyStatus::Stopping;
        state.last_updated = Utc::now();

        // Cancel all active orders
        for order_id in state.active_orders.keys() {
            if let Err(e) = self.execution_engine.cancel_order(order_id.clone()).await {
                warn!("Failed to cancel order {}: {}", order_id, e);
            }
        }

        state.active_orders.clear();
        state.status = StrategyStatus::Inactive;

        info!("Event-driven strategy stopped: {}", self.config.name);
        Ok(())
    }
}

/// Strategy error types
#[derive(Debug, thiserror::Error)]
pub enum StrategyError {
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    #[error("Order submission failed: {0}")]
    OrderSubmissionFailed(String),
    #[error("Market data error: {0}")]
    MarketDataError(String),
    #[error("Risk violation: {0}")]
    RiskViolation(String),
    #[error("Strategy not active: {0}")]
    StrategyNotActive(String),
    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Simple momentum event processor example
#[derive(Debug)]
pub struct MomentumEventProcessor {
    /// Price history for momentum calculation
    price_history: HashMap<InstrumentKey, VecDeque<PriceData>>,
    /// Momentum threshold
    momentum_threshold: f64,
    /// Lookback period
    lookback_period: usize,
}

impl MomentumEventProcessor {
    pub fn new(momentum_threshold: f64, lookback_period: usize) -> Self {
        Self {
            price_history: HashMap::new(),
            momentum_threshold,
            lookback_period,
        }
    }

    /// Calculate price momentum
    fn calculate_momentum(&self, prices: &VecDeque<PriceData>) -> Option<f64> {
        if prices.len() < 2 {
            return None;
        }

        let latest_price = prices.back()?.last.to_f64()?;
        let earlier_price = prices.front()?.last.to_f64()?;

        Some((latest_price - earlier_price) / earlier_price)
    }
}

impl EventProcessor for MomentumEventProcessor {
    fn process_market_data<'a>(
        &'a mut self,
        update: &'a MarketDataUpdate,
        _strategy_state: &'a mut StrategyState,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TradingSignal>, StrategyError>> + Send + 'a>> {
        Box::pin(async move {
            let mut signals = Vec::new();

            if let MarketDataUpdate::Price { instrument, data } = update {
                // Update price history
                let history = self
                    .price_history
                    .entry(instrument.clone())
                    .or_insert_with(VecDeque::new);
                history.push_back(data.clone());

                // Keep only recent prices
                while history.len() > self.lookback_period {
                    history.pop_front();
                }

                // Calculate momentum (clone history to avoid borrow conflicts)
                let history_clone = history.clone();
                if let Some(momentum) = self.calculate_momentum(&history_clone) {
                    if momentum.abs() > self.momentum_threshold {
                        let signal = TradingSignal {
                            signal_id: format!(
                                "momentum_{}_{}",
                                instrument.exchange, instrument.instrument
                            ),
                            instrument: instrument.clone(),
                            signal_type: SignalType::Momentum,
                            strength: momentum.abs().min(1.0),
                            direction: if momentum > 0.0 {
                                SignalDirection::Buy
                            } else {
                                SignalDirection::Sell
                            },
                            suggested_quantity: Decimal::new(100, 0), // Example fixed quantity
                            suggested_price: Some(data.last),
                            timestamp: Utc::now(),
                            source: "MomentumEventProcessor".to_string(),
                            metadata: {
                                let mut meta = HashMap::new();
                                meta.insert("momentum".to_string(), momentum.to_string());
                                meta
                            },
                            expires_at: Some(Utc::now() + chrono::Duration::minutes(5)),
                        };

                        signals.push(signal);
                    }
                }
            }

            Ok(signals)
        })
    }

    fn process_order_update<'a>(
        &'a mut self,
        _update: &'a OrderUpdateEvent,
        _strategy_state: &'a mut StrategyState,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TradingSignal>, StrategyError>> + Send + 'a>> {
        Box::pin(async move {
            // Momentum processor doesn't generate signals from order updates
            Ok(Vec::new())
        })
    }

    fn name(&self) -> &str {
        "MomentumEventProcessor"
    }

    fn priority(&self) -> u32 {
        100
    }
}

// Re-export key types for convenience

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            strategy_id: StrategyId::from(smol_str::SmolStr::new("default_strategy")),
            name: "Default Event-Driven Strategy".to_string(),
            instruments: Vec::new(),
            position_sizing: PositionSizingConfig {
                base_size: Decimal::new(100, 0),
                max_size: Decimal::new(1000, 0),
                scaling_method: SizingMethod::Fixed,
                volatility_based: false,
                kelly_factor: 0.25,
            },
            risk_management: RiskManagementConfig {
                max_drawdown: Decimal::new(10, 2), // 10%
                daily_loss_limit: Decimal::new(500, 0),
                max_open_positions: 5,
                stop_loss_pct: Some(2.0),   // 2%
                take_profit_pct: Some(5.0), // 5%
                enable_trailing_stops: false,
                correlation_limits: true,
            },
            signal_processing: SignalProcessingConfig {
                min_signal_strength: 0.5,
                signal_timeout_seconds: 300,
                enable_aggregation: false,
                conflicting_signals: ConflictResolution::StrongestWins,
                filters: Vec::new(),
            },
            execution: ExecutionConfig {
                preferred_order_type: OrderKind::Limit,
                time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
                allow_partial_fills: true,
                max_slippage_bps: 50,
                order_timeout_seconds: 60,
                enable_smart_routing: true,
            },
            performance: PerformanceConfig {
                benchmark: None,
                track_sharpe: true,
                track_max_drawdown: true,
                reporting_frequency_minutes: 60,
            },
        }
    }
}
