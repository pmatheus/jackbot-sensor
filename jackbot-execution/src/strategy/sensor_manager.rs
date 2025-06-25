use crate::strategy::events::{
    EventDrivenStrategy, EventDrivenStrategyEngine, MarketEvent, StrategyConfig, StrategyError,
    StrategySignal,
};
use crate::strategy::sensor_strategies::{
    SensorIcebergStrategy, SensorPovStrategy, SensorTwapStrategy, SensorVwapStrategy,
};
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};
use rust_decimal::Decimal;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    time::timeout,
};
use tracing::{debug, error, info, warn};

/// High-performance strategy manager for sensor operations
pub struct SensorStrategyManager {
    /// Event-driven strategy engine
    engine: EventDrivenStrategyEngine,
    /// Signal receiver for processing strategy signals
    signal_receiver: Arc<Mutex<mpsc::UnboundedReceiver<StrategySignal>>>,
    /// Performance metrics
    metrics: Arc<SensorManagerMetrics>,
    /// Active strategy configurations
    active_strategies: Arc<RwLock<HashMap<String, StrategyInfo>>>,
    /// Signal processing tasks
    signal_processors: Arc<RwLock<Vec<tokio::task::JoinHandle<()>>>>,
}

impl std::fmt::Debug for SensorStrategyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SensorStrategyManager")
            .field("engine", &"EventDrivenStrategyEngine")
            .field("signal_receiver", &"<channel>")
            .field("signal_processors", &"<JoinHandle collection>")
            .finish()
    }
}

/// Strategy information and metadata
#[derive(Debug, Clone)]
pub struct StrategyInfo {
    pub strategy_type: String,
    pub exchange: ExchangeId,
    pub instrument: InstrumentNameExchange,
    pub status: StrategyStatus,
    pub created_at: Instant,
    pub last_signal: Option<Instant>,
    pub signals_generated: u64,
    pub avg_execution_time_us: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrategyStatus {
    Active,
    Paused,
    Completed,
    Error(String),
}

/// Performance metrics for the strategy manager
#[derive(Debug)]
pub struct SensorManagerMetrics {
    pub strategies_created: AtomicU64,
    pub strategies_active: AtomicU64,
    pub total_signals_processed: AtomicU64,
    pub total_events_processed: AtomicU64,
    pub avg_signal_processing_time_us: AtomicU64,
    pub error_count: AtomicU64,
    pub start_time: Instant,
}

/// Configuration for creating sensor strategies
#[derive(Debug, Clone)]
pub struct SensorStrategyRequest {
    pub strategy_id: String,
    pub strategy_type: SensorStrategyType,
    pub exchange: ExchangeId,
    pub instrument: InstrumentNameExchange,
    pub parameters: SensorStrategyParameters,
}

#[derive(Debug, Clone)]
pub enum SensorStrategyType {
    Twap,
    Vwap,
    Iceberg,
    Pov,
}

#[derive(Debug, Clone)]
pub struct SensorStrategyParameters {
    pub target_quantity: Decimal,
    pub duration: Option<Duration>,
    pub slice_count: Option<usize>,
    pub participation_rate: Option<f64>,
    pub chunk_size: Option<Decimal>,
    pub max_concurrent_orders: Option<usize>,
    pub assessment_interval: Option<Duration>,
    pub custom_params: HashMap<String, serde_json::Value>,
}

impl SensorStrategyManager {
    /// Create new sensor strategy manager
    pub fn new() -> Result<Self, StrategyError> {
        let (signal_sender, signal_receiver) = mpsc::unbounded_channel();

        let config = StrategyConfig {
            max_concurrent_strategies: 100,
            performance_monitoring: true,
            error_recovery: true,
            circuit_breaker_threshold: 10,
            custom_settings: HashMap::new(),
        };

        let engine = EventDrivenStrategyEngine::new(config, signal_sender);

        let metrics = Arc::new(SensorManagerMetrics {
            strategies_created: AtomicU64::new(0),
            strategies_active: AtomicU64::new(0),
            total_signals_processed: AtomicU64::new(0),
            total_events_processed: AtomicU64::new(0),
            avg_signal_processing_time_us: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            start_time: Instant::now(),
        });

        Ok(Self {
            engine,
            signal_receiver: Arc::new(Mutex::new(signal_receiver)),
            metrics,
            active_strategies: Arc::new(RwLock::new(HashMap::new())),
            signal_processors: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Start the strategy manager and signal processing
    pub async fn start(&self) -> Result<(), StrategyError> {
        info!("Starting sensor strategy manager");

        // Start signal processing tasks
        self.start_signal_processors().await;

        info!("Sensor strategy manager started successfully");
        Ok(())
    }

    /// Create and register a new sensor strategy
    pub async fn create_strategy(
        &self,
        request: SensorStrategyRequest,
    ) -> Result<String, StrategyError> {
        let strategy_id = request.strategy_id.clone();

        // Create the appropriate strategy based on type
        let strategy: Box<dyn EventDrivenStrategy> = match request.strategy_type {
            SensorStrategyType::Twap => {
                let duration = request
                    .parameters
                    .duration
                    .unwrap_or(Duration::from_secs(300)); // Default 5 minutes
                let slice_count = request.parameters.slice_count.unwrap_or(10);

                Box::new(SensorTwapStrategy::new(
                    strategy_id.clone(),
                    request.exchange.clone(),
                    request.instrument.clone(),
                    request.parameters.target_quantity,
                    duration,
                    slice_count,
                ))
            }
            SensorStrategyType::Vwap => {
                let participation_rate = request.parameters.participation_rate.unwrap_or(0.1);

                Box::new(SensorVwapStrategy::new(
                    strategy_id.clone(),
                    request.exchange.clone(),
                    request.instrument.clone(),
                    request.parameters.target_quantity,
                    participation_rate,
                ))
            }
            SensorStrategyType::Iceberg => {
                let chunk_size = request
                    .parameters
                    .chunk_size
                    .unwrap_or(request.parameters.target_quantity / Decimal::from(10));
                let max_concurrent = request.parameters.max_concurrent_orders.unwrap_or(3);

                Box::new(SensorIcebergStrategy::new(
                    strategy_id.clone(),
                    request.exchange.clone(),
                    request.instrument.clone(),
                    request.parameters.target_quantity,
                    chunk_size,
                    max_concurrent,
                ))
            }
            SensorStrategyType::Pov => {
                let participation_rate = request.parameters.participation_rate.unwrap_or(0.1);
                let assessment_interval = request
                    .parameters
                    .assessment_interval
                    .unwrap_or(Duration::from_secs(30));

                Box::new(SensorPovStrategy::new(
                    strategy_id.clone(),
                    request.exchange.clone(),
                    request.instrument.clone(),
                    request.parameters.target_quantity,
                    participation_rate,
                    assessment_interval,
                ))
            }
        };

        // Register strategy with engine
        self.engine.register_strategy(strategy).await?;

        // Track strategy info
        let strategy_info = StrategyInfo {
            strategy_type: format!("{:?}", request.strategy_type),
            exchange: request.exchange.clone(),
            instrument: request.instrument.clone(),
            status: StrategyStatus::Active,
            created_at: Instant::now(),
            last_signal: None,
            signals_generated: 0,
            avg_execution_time_us: 0.0,
        };

        {
            let mut active_strategies = self.active_strategies.write().await;
            active_strategies.insert(strategy_id.clone(), strategy_info);
        }

        // Update metrics
        self.metrics
            .strategies_created
            .fetch_add(1, Ordering::Relaxed);
        self.metrics
            .strategies_active
            .fetch_add(1, Ordering::Relaxed);

        info!(
            strategy_id = %strategy_id,
            strategy_type = ?request.strategy_type,
            exchange = ?request.exchange,
            instrument = %request.instrument,
            target_quantity = %request.parameters.target_quantity,
            "Created sensor strategy"
        );

        Ok(strategy_id)
    }

    /// Remove a strategy
    pub async fn remove_strategy(&self, strategy_id: &str) -> Result<(), StrategyError> {
        // Unregister from engine
        self.engine.unregister_strategy(strategy_id).await?;

        // Update tracking
        {
            let mut active_strategies = self.active_strategies.write().await;
            if active_strategies.remove(strategy_id).is_some() {
                self.metrics
                    .strategies_active
                    .fetch_sub(1, Ordering::Relaxed);
            }
        }

        info!(strategy_id = %strategy_id, "Removed sensor strategy");
        Ok(())
    }

    /// Publish market event to all strategies
    pub async fn publish_event(&self, event: MarketEvent) -> Result<(), StrategyError> {
        self.engine.publish_event(event).await?;
        self.metrics
            .total_events_processed
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Get current strategy status
    pub async fn get_strategy_status(&self, strategy_id: &str) -> Option<StrategyInfo> {
        let active_strategies = self.active_strategies.read().await;
        active_strategies.get(strategy_id).cloned()
    }

    /// List all active strategies
    pub async fn list_strategies(&self) -> HashMap<String, StrategyInfo> {
        let active_strategies = self.active_strategies.read().await;
        active_strategies.clone()
    }

    /// Get manager performance metrics
    pub fn get_metrics(&self) -> SensorManagerMetrics {
        SensorManagerMetrics {
            strategies_created: AtomicU64::new(
                self.metrics.strategies_created.load(Ordering::Relaxed),
            ),
            strategies_active: AtomicU64::new(
                self.metrics.strategies_active.load(Ordering::Relaxed),
            ),
            total_signals_processed: AtomicU64::new(
                self.metrics.total_signals_processed.load(Ordering::Relaxed),
            ),
            total_events_processed: AtomicU64::new(
                self.metrics.total_events_processed.load(Ordering::Relaxed),
            ),
            avg_signal_processing_time_us: AtomicU64::new(
                self.metrics
                    .avg_signal_processing_time_us
                    .load(Ordering::Relaxed),
            ),
            error_count: AtomicU64::new(self.metrics.error_count.load(Ordering::Relaxed)),
            start_time: self.metrics.start_time,
        }
    }

    /// Health check for the manager
    pub async fn health_check(&self) -> bool {
        // Check engine health
        if !self.engine.health_check().await {
            return false;
        }

        // Check if signal processing is working
        let signal_processors = self.signal_processors.read().await;
        for processor in signal_processors.iter() {
            if processor.is_finished() {
                warn!("Signal processor task has finished unexpectedly");
                return false;
            }
        }

        true
    }

    /// Start signal processing tasks
    async fn start_signal_processors(&self) {
        let num_processors = 4; // Configurable number of signal processors
        let mut processors = Vec::new();

        for i in 0..num_processors {
            let signal_receiver = self.signal_receiver.clone();
            let metrics = self.metrics.clone();
            let active_strategies = self.active_strategies.clone();

            let processor = tokio::spawn(async move {
                Self::signal_processor_task(i, signal_receiver, metrics, active_strategies).await;
            });

            processors.push(processor);
        }

        let mut signal_processors = self.signal_processors.write().await;
        *signal_processors = processors;

        info!(
            processors = num_processors,
            "Started signal processing tasks"
        );
    }

    /// Signal processor task
    async fn signal_processor_task(
        processor_id: usize,
        signal_receiver: Arc<Mutex<mpsc::UnboundedReceiver<StrategySignal>>>,
        metrics: Arc<SensorManagerMetrics>,
        active_strategies: Arc<RwLock<HashMap<String, StrategyInfo>>>,
    ) {
        debug!(
            processor_id = processor_id,
            "Starting signal processor task"
        );

        loop {
            // Try to receive a signal with timeout to avoid blocking indefinitely
            let signal_opt = {
                let mut receiver = signal_receiver.lock().await;

                // Use timeout to prevent blocking forever
                let timeout_result = timeout(Duration::from_millis(100), receiver.recv()).await;
                match timeout_result {
                    Ok(signal) => signal,
                    Err(_) => continue, // Timeout, continue loop
                }
            };

            match signal_opt {
                Some(signal) => {
                    let start_time = Instant::now();

                    // Process the signal
                    if let Err(e) = Self::process_signal(signal, &active_strategies).await {
                        error!(
                            processor_id = processor_id,
                            error = %e,
                            "Failed to process strategy signal"
                        );
                        metrics.error_count.fetch_add(1, Ordering::Relaxed);
                    }

                    // Update metrics
                    let processing_time = start_time.elapsed().as_micros() as u64;
                    let total_signals = metrics
                        .total_signals_processed
                        .fetch_add(1, Ordering::Relaxed)
                        + 1;

                    // Update rolling average
                    let current_avg = metrics
                        .avg_signal_processing_time_us
                        .load(Ordering::Relaxed);
                    let new_avg =
                        ((current_avg * (total_signals - 1)) + processing_time) / total_signals;
                    metrics
                        .avg_signal_processing_time_us
                        .store(new_avg, Ordering::Relaxed);
                }
                None => {
                    // Channel closed, exit processor
                    warn!(
                        processor_id = processor_id,
                        "Signal channel closed, stopping processor"
                    );
                    break;
                }
            }
        }
    }

    /// Process a strategy signal
    async fn process_signal(
        signal: StrategySignal,
        active_strategies: &Arc<RwLock<HashMap<String, StrategyInfo>>>,
    ) -> Result<(), StrategyError> {
        match signal {
            StrategySignal::Execute {
                strategy_id,
                exchange,
                instrument,
                parameters,
                urgency,
                timestamp,
            } => {
                debug!(
                    strategy_id = %strategy_id,
                    exchange = ?exchange,
                    instrument = %instrument,
                    quantity = %parameters.quantity,
                    price = ?parameters.price,
                    side = ?parameters.side,
                    urgency = ?urgency,
                    timestamp = timestamp,
                    "Processing execute signal"
                );

                // Update strategy info
                {
                    let mut strategies = active_strategies.write().await;
                    if let Some(info) = strategies.get_mut(&strategy_id) {
                        info.last_signal = Some(Instant::now());
                        info.signals_generated += 1;
                    }
                }

                // Here you would implement the actual order execution
                // For now, we'll just log the signal
                info!(
                    strategy_id = %strategy_id,
                    strategy_type = %parameters.strategy_type,
                    quantity = %parameters.quantity,
                    "Executed strategy signal"
                );

                Ok(())
            }
            StrategySignal::Cancel {
                strategy_id,
                orders,
                timestamp: _,
            } => {
                debug!(
                    strategy_id = %strategy_id,
                    orders = orders.len(),
                    "Processing cancel signal"
                );

                // Here you would implement order cancellation
                info!(
                    strategy_id = %strategy_id,
                    orders_cancelled = orders.len(),
                    "Cancelled orders"
                );

                Ok(())
            }
            StrategySignal::UpdateParameters {
                strategy_id,
                parameters: _,
                timestamp: _,
            } => {
                debug!(
                    strategy_id = %strategy_id,
                    "Processing parameter update signal"
                );

                // Here you would implement parameter updates
                info!(strategy_id = %strategy_id, "Updated strategy parameters");

                Ok(())
            }
            StrategySignal::AdjustPosition {
                strategy_id,
                exchange: _,
                instrument: _,
                target_quantity,
                timestamp: _,
            } => {
                debug!(
                    strategy_id = %strategy_id,
                    target_quantity = %target_quantity,
                    "Processing position adjustment signal"
                );

                // Here you would implement position adjustment
                info!(
                    strategy_id = %strategy_id,
                    target_quantity = %target_quantity,
                    "Adjusted position"
                );

                Ok(())
            }
        }
    }

    /// Shutdown the strategy manager
    pub async fn shutdown(&self) -> Result<(), StrategyError> {
        info!("Shutting down sensor strategy manager");

        // Cancel all signal processing tasks
        {
            let mut signal_processors = self.signal_processors.write().await;
            for processor in signal_processors.drain(..) {
                processor.abort();
            }
        }

        // Remove all strategies
        let strategy_ids: Vec<String> = {
            let strategies = self.active_strategies.read().await;
            strategies.keys().cloned().collect()
        };

        for strategy_id in strategy_ids {
            if let Err(e) = self.remove_strategy(&strategy_id).await {
                warn!(
                    strategy_id = %strategy_id,
                    error = %e,
                    "Failed to remove strategy during shutdown"
                );
            }
        }

        info!("Sensor strategy manager shutdown complete");
        Ok(())
    }
}

impl Default for SensorStrategyManager {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

/// Convenience functions for creating common sensor strategies
impl SensorStrategyManager {
    /// Create a TWAP strategy with default parameters
    pub async fn create_twap_strategy(
        &self,
        strategy_id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        target_quantity: Decimal,
        duration: Duration,
    ) -> Result<String, StrategyError> {
        let request = SensorStrategyRequest {
            strategy_id,
            strategy_type: SensorStrategyType::Twap,
            exchange,
            instrument,
            parameters: SensorStrategyParameters {
                target_quantity,
                duration: Some(duration),
                slice_count: Some(10),
                participation_rate: None,
                chunk_size: None,
                max_concurrent_orders: None,
                assessment_interval: None,
                custom_params: HashMap::new(),
            },
        };

        self.create_strategy(request).await
    }

    /// Create a VWAP strategy with default parameters
    pub async fn create_vwap_strategy(
        &self,
        strategy_id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        target_quantity: Decimal,
        participation_rate: f64,
    ) -> Result<String, StrategyError> {
        let request = SensorStrategyRequest {
            strategy_id,
            strategy_type: SensorStrategyType::Vwap,
            exchange,
            instrument,
            parameters: SensorStrategyParameters {
                target_quantity,
                duration: None,
                slice_count: None,
                participation_rate: Some(participation_rate),
                chunk_size: None,
                max_concurrent_orders: None,
                assessment_interval: None,
                custom_params: HashMap::new(),
            },
        };

        self.create_strategy(request).await
    }

    /// Create an Iceberg strategy with default parameters
    pub async fn create_iceberg_strategy(
        &self,
        strategy_id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        target_quantity: Decimal,
        chunk_size: Decimal,
    ) -> Result<String, StrategyError> {
        let request = SensorStrategyRequest {
            strategy_id,
            strategy_type: SensorStrategyType::Iceberg,
            exchange,
            instrument,
            parameters: SensorStrategyParameters {
                target_quantity,
                duration: None,
                slice_count: None,
                participation_rate: None,
                chunk_size: Some(chunk_size),
                max_concurrent_orders: Some(3),
                assessment_interval: None,
                custom_params: HashMap::new(),
            },
        };

        self.create_strategy(request).await
    }

    /// Create a POV strategy with default parameters
    pub async fn create_pov_strategy(
        &self,
        strategy_id: String,
        exchange: ExchangeId,
        instrument: InstrumentNameExchange,
        target_quantity: Decimal,
        participation_rate: f64,
    ) -> Result<String, StrategyError> {
        let request = SensorStrategyRequest {
            strategy_id,
            strategy_type: SensorStrategyType::Pov,
            exchange,
            instrument,
            parameters: SensorStrategyParameters {
                target_quantity,
                duration: None,
                slice_count: None,
                participation_rate: Some(participation_rate),
                chunk_size: None,
                max_concurrent_orders: None,
                assessment_interval: Some(Duration::from_secs(30)),
                custom_params: HashMap::new(),
            },
        };

        self.create_strategy(request).await
    }
}
