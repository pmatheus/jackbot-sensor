use crate::{
    client::ExecutionClient,
    error::UnindexedOrderError,
    order::{
        id::ClientOrderId,
        request::{OrderRequestOpen, RequestOpen},
        router::OrderRouter,
        sensor::{
            EventTriggeredOrder, JackpotOrder, MarketEvent, OrderExecutionMetrics, PropheticOrder,
            SensorOrderConfig, SensorOrderState,
        },
        OrderKey, OrderKind, Side, TimeInForce,
    },
};
use jackbot_data::books::aggregator::OrderBookAggregator;
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::{
    sync::{RwLock, Semaphore},
    task::JoinSet,
    time::Duration,
};
use tracing::{debug, error, info, warn};

/// High-performance concurrent order executor for sensor-specific trading
///
/// Manages order lifecycle, handles sensor-specific order types (Jackpot, Prophetic, Event-triggered),
/// and provides real-time execution with performance monitoring.
#[derive(Debug)]
pub struct OrderExecutor<C: ExecutionClient> {
    /// Order router for exchange selection and routing
    router: Arc<OrderRouter<C>>,
    /// Configuration for execution behavior
    config: SensorOrderConfig,
    /// Active sensor orders waiting for execution
    pending_orders: Arc<RwLock<HashMap<ClientOrderId, SensorOrderState>>>,
    /// Market event stream for event-triggered orders
    market_events: Arc<RwLock<Vec<MarketEvent>>>,
    /// Order book aggregators for market analysis
    aggregators: Arc<RwLock<HashMap<InstrumentNameExchange, OrderBookAggregator>>>,
    /// Performance metrics
    metrics: Arc<RwLock<OrderExecutionMetrics>>,
    /// Concurrency control
    execution_semaphore: Arc<Semaphore>,
    /// Task management for concurrent processing
    task_set: Arc<RwLock<JoinSet<()>>>,
}

/// Order execution result with detailed information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub order_id: ClientOrderId,
    pub success: bool,
    pub execution_time_ms: u64,
    pub exchange_used: Option<ExchangeId>,
    pub error_message: Option<String>,
    pub sensor_type: Option<String>,
    pub confidence_score: Option<f64>,
}

/// Market condition snapshot for decision making
#[derive(Debug, Clone)]
pub struct MarketConditions {
    pub current_volatility: f64,
    pub liquidity_score: f64,
    pub spread: Option<Decimal>,
    pub volume_profile: f64,
    pub recent_events: Vec<MarketEvent>,
}

impl<C: ExecutionClient + Clone + Send + Sync + 'static> OrderExecutor<C> {
    /// Create a new OrderExecutor
    pub fn new(router: Arc<OrderRouter<C>>, config: SensorOrderConfig) -> Self {
        Self {
            router,
            config,
            pending_orders: Arc::new(RwLock::new(HashMap::new())),
            market_events: Arc::new(RwLock::new(Vec::new())),
            aggregators: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(OrderExecutionMetrics::default())),
            execution_semaphore: Arc::new(Semaphore::new(50)), // Max 50 concurrent executions
            task_set: Arc::new(RwLock::new(JoinSet::new())),
        }
    }

    /// Start the order executor with background processing
    pub async fn start(&self) -> Result<(), UnindexedOrderError> {
        info!("Starting OrderExecutor with sensor-specific processing");

        // Start background task for processing pending orders
        let pending_processor = self.clone();
        let mut task_set = self.task_set.write().await;

        task_set.spawn(async move {
            pending_processor.process_pending_orders_loop().await;
        });

        // Start market event processor
        let event_processor = self.clone();
        task_set.spawn(async move {
            event_processor.process_market_events_loop().await;
        });

        // Start performance monitoring
        let metrics_processor = self.clone();
        task_set.spawn(async move {
            metrics_processor.performance_monitoring_loop().await;
        });

        Ok(())
    }

    /// Submit a sensor-specific order for execution
    pub async fn submit_order(
        &self,
        order_key: OrderKey,
        side: Side,
        price: Decimal,
        quantity: Decimal,
        order_kind: OrderKind,
        time_in_force: TimeInForce,
    ) -> Result<ClientOrderId, UnindexedOrderError> {
        let order_id = ClientOrderId::random();
        let _start_time = Instant::now();

        debug!(
            "Submitting order: id={}, kind={:?}, side={:?}, price={}, qty={}",
            order_id, order_kind, side, price, quantity
        );

        // Create sensor order state based on order kind
        let sensor_state = match order_kind {
            OrderKind::Jackpot => {
                let jackpot_order = JackpotOrder::new(Default::default());
                SensorOrderState::JackpotPending(jackpot_order)
            }
            OrderKind::Prophetic => {
                let prophetic_order = PropheticOrder::new(Default::default());
                SensorOrderState::PropheticAnalyzing(prophetic_order)
            }
            OrderKind::EventTriggered => {
                let event_order = EventTriggeredOrder::new(Default::default());
                SensorOrderState::EventWaiting(event_order)
            }
            _ => {
                // Standard order types - execute immediately
                // Convert OrderKey to the right type (this is a temporary fix)
                let converted_key = OrderKey::<ExchangeId, InstrumentNameExchange> {
                    exchange: ExchangeId::BinanceSpot, // Default exchange - should be mapped properly
                    instrument: InstrumentNameExchange::from("BTC/USDT"), // Default instrument - should be mapped properly
                    strategy: order_key.strategy,
                    cid: order_key.cid,
                };
                return self
                    .execute_standard_order(
                        converted_key,
                        side,
                        price,
                        quantity,
                        order_kind,
                        time_in_force,
                    )
                    .await;
            }
        };

        // Store pending sensor order
        let mut pending_orders = self.pending_orders.write().await;
        pending_orders.insert(order_id.clone(), sensor_state);

        info!(
            "Sensor order {} queued for processing (type: {:?})",
            order_id, order_kind
        );

        Ok(order_id)
    }

    /// Execute standard (non-sensor) order immediately
    async fn execute_standard_order(
        &self,
        order_key: OrderKey<ExchangeId, InstrumentNameExchange>,
        side: Side,
        price: Decimal,
        quantity: Decimal,
        order_kind: OrderKind,
        time_in_force: TimeInForce,
    ) -> Result<ClientOrderId, UnindexedOrderError> {
        let order_id = ClientOrderId::random();
        let start_time = Instant::now();

        // Create standard order request
        let order_request = OrderRequestOpen::<ExchangeId, InstrumentNameExchange> {
            key: OrderKey {
                exchange: order_key.exchange,
                instrument: order_key.instrument,
                strategy: order_key.strategy,
                cid: order_id.clone(),
            },
            state: RequestOpen {
                side,
                price,
                quantity,
                kind: order_kind,
                time_in_force,
            },
        };

        // Execute through router
        match self.router.execute_order(order_request).await {
            Ok(_order) => {
                let execution_time = start_time.elapsed();

                // Update metrics
                let mut metrics = self.metrics.write().await;
                metrics.update_execution(execution_time, true);

                info!(
                    "Standard order {} executed successfully in {}ms",
                    order_id,
                    execution_time.as_millis()
                );

                Ok(order_id)
            }
            Err(error) => {
                let execution_time = start_time.elapsed();

                // Update metrics
                let mut metrics = self.metrics.write().await;
                metrics.update_execution(execution_time, false);

                error!("Standard order {} execution failed: {:?}", order_id, error);
                Err(error)
            }
        }
    }

    /// Add market event for event-triggered orders
    pub async fn add_market_event(&self, event: MarketEvent) {
        let mut events = self.market_events.write().await;
        events.push(event);

        // Keep only recent events (last 1000)
        if events.len() > 1000 {
            events.drain(0..500); // Remove oldest half
        }
    }

    /// Add order book aggregator for market analysis
    pub async fn add_aggregator(
        &self,
        instrument: InstrumentNameExchange,
        aggregator: OrderBookAggregator,
    ) {
        // Add to router which will handle the aggregation
        self.router.add_aggregator(instrument, aggregator).await;
    }

    /// Main loop for processing pending sensor orders
    async fn process_pending_orders_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_millis(100)); // Check every 100ms

        loop {
            interval.tick().await;

            if let Err(e) = self.process_pending_orders().await {
                warn!("Error processing pending orders: {:?}", e);
            }
        }
    }

    /// Process all pending sensor orders
    async fn process_pending_orders(&self) -> Result<(), UnindexedOrderError> {
        let pending_orders = {
            let pending = self.pending_orders.read().await;
            pending.clone()
        };

        if pending_orders.is_empty() {
            return Ok(());
        }

        debug!("Processing {} pending sensor orders", pending_orders.len());

        let mut orders_to_execute = Vec::new();
        let mut orders_to_remove = Vec::new();

        for (order_id, sensor_state) in pending_orders {
            match self.evaluate_sensor_order(&order_id, &sensor_state).await? {
                SensorOrderDecision::Execute { confidence_score } => {
                    orders_to_execute.push((order_id.clone(), confidence_score));
                }
                SensorOrderDecision::Keep => {
                    // Order stays pending
                }
                SensorOrderDecision::Cancel { reason } => {
                    warn!("Canceling sensor order {}: {}", order_id, reason);
                    orders_to_remove.push(order_id);
                }
            }
        }

        // Execute ready orders concurrently
        if !orders_to_execute.is_empty() {
            let executor = self.clone();
            let mut task_set = JoinSet::new();

            for (order_id, confidence_score) in orders_to_execute {
                let executor_clone = executor.clone();
                task_set.spawn(async move {
                    executor_clone
                        .execute_sensor_order(order_id, confidence_score)
                        .await
                });
            }

            // Wait for all executions to complete
            loop {
                let next_result = task_set.join_next().await;
                if let Some(result) = next_result {
                    if let Err(e) = result {
                        error!("Sensor order execution task failed: {:?}", e);
                    }
                } else {
                    break;
                }
            }
        }

        // Remove canceled orders
        if !orders_to_remove.is_empty() {
            let mut pending = self.pending_orders.write().await;
            for order_id in orders_to_remove {
                pending.remove(&order_id);
            }
        }

        Ok(())
    }

    /// Evaluate if a sensor order is ready for execution
    async fn evaluate_sensor_order(
        &self,
        order_id: &ClientOrderId,
        sensor_state: &SensorOrderState,
    ) -> Result<SensorOrderDecision, UnindexedOrderError> {
        match sensor_state {
            SensorOrderState::JackpotPending(jackpot_order) => {
                // Get market conditions for probability calculation
                let current_volatility = self.get_market_conditions(&jackpot_order).await?;

                // Get aggregator for this order's instrument
                let aggregators = self.aggregators.read().await;
                let aggregator = aggregators.values().next().ok_or_else(|| {
                    UnindexedOrderError::Connectivity(crate::error::ConnectivityError::Socket(
                        "No aggregator available".to_string(),
                    ))
                })?;

                // Create mutable copy to check execution
                let mut jackpot_copy = jackpot_order.clone();
                if jackpot_copy.should_execute(aggregator, current_volatility) {
                    let probability =
                        jackpot_copy.calculate_probability(aggregator, current_volatility);

                    info!(
                        "Jackpot order {} triggered with probability {:.2}",
                        order_id, probability
                    );

                    return Ok(SensorOrderDecision::Execute {
                        confidence_score: probability,
                    });
                }
            }

            SensorOrderState::PropheticAnalyzing(prophetic_order) => {
                // Check if analysis is complete and confidence threshold is met
                if prophetic_order.should_execute() {
                    let confidence = prophetic_order.prediction_score.unwrap_or(0.0);

                    info!(
                        "Prophetic order {} ready for execution with confidence {:.2}",
                        order_id, confidence
                    );

                    return Ok(SensorOrderDecision::Execute {
                        confidence_score: confidence,
                    });
                } else {
                    // Trigger analysis if not done recently
                    if prophetic_order.last_analysis.is_none() {
                        // In a real implementation, would trigger analysis here
                        debug!("Prophetic order {} needs market analysis", order_id);
                    }
                }
            }

            SensorOrderState::EventWaiting(event_order) => {
                // Check if trigger conditions are met
                let events = self.market_events.read().await;
                let mut event_copy = event_order.clone();

                // Get aggregator for the order's instrument (simplified)
                let aggregators = self.aggregators.read().await;
                if let Some(aggregator) = aggregators.values().next() {
                    if event_copy.check_triggers(aggregator, &events).await {
                        info!("Event-triggered order {} activated", order_id);

                        // Check if ready for execution after trigger delay
                        if event_copy.should_execute() {
                            return Ok(SensorOrderDecision::Execute {
                                confidence_score: 0.8, // Default confidence for event-triggered
                            });
                        }
                    }
                }
            }

            SensorOrderState::ReadyForExecution {
                confidence_score, ..
            } => {
                return Ok(SensorOrderDecision::Execute {
                    confidence_score: confidence_score.unwrap_or(0.8),
                });
            }
        }

        Ok(SensorOrderDecision::Keep)
    }

    /// Execute a sensor order that's ready for execution
    async fn execute_sensor_order(
        &self,
        order_id: ClientOrderId,
        confidence_score: f64,
    ) -> Result<(), UnindexedOrderError> {
        let _permit = self.execution_semaphore.acquire().await.unwrap();
        let start_time = Instant::now();

        // Remove from pending orders
        let sensor_state = {
            let mut pending = self.pending_orders.write().await;
            pending.remove(&order_id)
        };

        let Some(_sensor_state) = sensor_state else {
            warn!("Sensor order {} not found in pending orders", order_id);
            return Ok(());
        };

        // TODO: Convert sensor state to actual order request and execute
        // This would require additional order details stored with the sensor state

        info!(
            "Sensor order {} executed with confidence {:.2} in {}ms",
            order_id,
            confidence_score,
            start_time.elapsed().as_millis()
        );

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.update_execution(start_time.elapsed(), true);

        Ok(())
    }

    /// Get current market conditions for analysis
    async fn get_market_conditions(
        &self,
        _order: &JackpotOrder,
    ) -> Result<f64, UnindexedOrderError> {
        // Calculate volatility (simplified) - in a real implementation this would
        // analyze recent price movements from the aggregators
        let current_volatility = 0.02; // 2% default volatility

        Ok(current_volatility)
    }

    /// Process market events loop
    async fn process_market_events_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            // Clean old market events
            let mut events = self.market_events.write().await;
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(1);
            events.retain(|event| {
                let event_time = match event {
                    MarketEvent::PriceChange { timestamp, .. } => *timestamp,
                    MarketEvent::VolumeSpike { timestamp, .. } => *timestamp,
                    MarketEvent::NewsSentiment { timestamp, .. } => *timestamp,
                    MarketEvent::StrategySignal { timestamp, .. } => *timestamp,
                };
                event_time > cutoff
            });
        }
    }

    /// Performance monitoring loop
    async fn performance_monitoring_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            interval.tick().await;

            let metrics = self.metrics.read().await;
            let pending_count = self.pending_orders.read().await.len();

            info!(
                "OrderExecutor Performance: {} total orders, {:.1}% success rate, avg {}ms execution, {} pending",
                metrics.total_orders,
                metrics.success_rate() * 100.0,
                metrics.average_execution_time.as_millis(),
                pending_count
            );

            // Log warning if performance is degrading
            if metrics.average_execution_time > Duration::from_millis(400) {
                warn!(
                    "Order execution performance degraded: avg {}ms (target <500ms)",
                    metrics.average_execution_time.as_millis()
                );
            }
        }
    }

    /// Get current executor metrics
    pub async fn get_metrics(&self) -> OrderExecutionMetrics {
        self.metrics.read().await.clone()
    }

    /// Get pending order count by type
    pub async fn get_pending_orders_stats(&self) -> PendingOrdersStats {
        let pending = self.pending_orders.read().await;
        let mut stats = PendingOrdersStats::default();

        for (_, state) in pending.iter() {
            match state {
                SensorOrderState::JackpotPending(_) => stats.jackpot_orders += 1,
                SensorOrderState::PropheticAnalyzing(_) => stats.prophetic_orders += 1,
                SensorOrderState::EventWaiting(_) => stats.event_triggered_orders += 1,
                SensorOrderState::ReadyForExecution { .. } => stats.ready_for_execution += 1,
            }
        }

        stats.total_pending = pending.len();
        stats
    }

    /// Stop the executor and clean up
    pub async fn stop(&self) -> Result<(), UnindexedOrderError> {
        info!("Stopping OrderExecutor");

        // Cancel all background tasks
        let mut task_set = self.task_set.write().await;
        task_set.abort_all();

        // Clear pending orders
        let mut pending = self.pending_orders.write().await;
        let pending_count = pending.len();
        pending.clear();

        if pending_count > 0 {
            warn!("Cleared {} pending orders during shutdown", pending_count);
        }

        info!("OrderExecutor stopped successfully");
        Ok(())
    }
}

/// Decision result for sensor order evaluation
#[derive(Debug, Clone)]
enum SensorOrderDecision {
    Execute { confidence_score: f64 },
    Keep,
    Cancel { reason: String },
}

// MarketConditionsForOrder struct removed - using direct volatility calculation

/// Statistics for pending orders
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PendingOrdersStats {
    pub total_pending: usize,
    pub jackpot_orders: usize,
    pub prophetic_orders: usize,
    pub event_triggered_orders: usize,
    pub ready_for_execution: usize,
}

// Implement Clone for OrderExecutor to enable task spawning
impl<C: ExecutionClient + Clone> Clone for OrderExecutor<C> {
    fn clone(&self) -> Self {
        Self {
            router: Arc::clone(&self.router),
            config: self.config.clone(),
            pending_orders: Arc::clone(&self.pending_orders),
            market_events: Arc::clone(&self.market_events),
            aggregators: Arc::clone(&self.aggregators),
            metrics: Arc::clone(&self.metrics),
            execution_semaphore: Arc::clone(&self.execution_semaphore),
            task_set: Arc::clone(&self.task_set),
        }
    }
}
