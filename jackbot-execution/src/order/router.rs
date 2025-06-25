use crate::{
    client::ExecutionClient,
    error::UnindexedOrderError,
    order::{
        request::OrderRequestOpen,
        sensor::{OrderExecutionMetrics, SensorOrderConfig},
        state::ActiveOrderState,
        Order, Side,
    },
};
use jackbot_data::books::aggregator::OrderBookAggregator;
use jackbot_instrument::{
    exchange::{ExchangeId, ExchangeIndex},
    instrument::name::InstrumentNameExchange,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::{
    sync::{RwLock, Semaphore},
    time::Duration,
};
use tracing::{debug, error, info, warn};

/// High-performance order router for sensor-specific trading
///
/// Handles multi-exchange routing, latency optimization, and risk management
/// with performance targets of <500ms execution time.
#[derive(Debug)]
pub struct OrderRouter<C: ExecutionClient> {
    /// Client connections to exchanges
    clients: HashMap<ExchangeId, C>,
    /// Configuration for routing behavior
    config: SensorOrderConfig,
    /// Order book aggregators for each instrument
    aggregators: Arc<RwLock<HashMap<InstrumentNameExchange, OrderBookAggregator>>>,
    /// Exchange latency monitoring
    latency_monitor: LatencyMonitor,
    /// Risk management system
    risk_manager: RiskManager,
    /// Performance metrics tracking
    metrics: Arc<RwLock<OrderExecutionMetrics>>,
    /// Concurrency limiter for order execution
    execution_semaphore: Arc<Semaphore>,
    /// Exchange index to ID mapping
    exchange_mapping: HashMap<ExchangeIndex, ExchangeId>,
}

/// Configuration for order routing behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// Maximum number of concurrent orders per exchange
    pub max_concurrent_orders: usize,
    /// Preferred exchange ordering (highest priority first)
    pub exchange_priority: Vec<ExchangeId>,
    /// Maximum latency tolerance for each exchange (ms)
    pub max_latency_ms: HashMap<ExchangeId, u64>,
    /// Minimum order size thresholds per exchange
    pub min_order_sizes: HashMap<ExchangeId, Decimal>,
    /// Enable smart routing based on liquidity
    pub enable_smart_routing: bool,
    /// Fallback exchange when primary is unavailable
    pub fallback_exchanges: HashMap<ExchangeId, Vec<ExchangeId>>,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            max_concurrent_orders: 10,
            exchange_priority: Vec::new(),
            max_latency_ms: HashMap::new(),
            min_order_sizes: HashMap::new(),
            enable_smart_routing: true,
            fallback_exchanges: HashMap::new(),
        }
    }
}

/// Monitors exchange latency and health
#[derive(Debug, Clone)]
pub struct LatencyMonitor {
    /// Average latency per exchange (milliseconds)
    average_latency: HashMap<ExchangeId, f64>,
    /// Recent latency samples (limited to last 100 samples)
    recent_samples: HashMap<ExchangeId, Vec<u64>>,
    /// Exchange health status
    exchange_health: HashMap<ExchangeId, ExchangeHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExchangeHealth {
    Healthy,
    Degraded { reason: String },
    Unavailable { reason: String },
}

/// Risk management for order execution
#[derive(Debug, Clone)]
pub struct RiskManager {
    /// Position limits per exchange
    position_limits: HashMap<ExchangeId, Decimal>,
    /// Current position exposure
    current_positions: HashMap<ExchangeId, Decimal>,
    /// Daily volume limits
    daily_volume_limits: HashMap<ExchangeId, Decimal>,
    /// Current daily volume
    current_daily_volume: HashMap<ExchangeId, Decimal>,
    /// Maximum order value limits
    max_order_values: HashMap<ExchangeId, Decimal>,
}

/// Route selection result
#[derive(Debug, Clone)]
pub struct RouteSelection {
    /// Primary exchange for execution
    pub primary_exchange: ExchangeId,
    /// Fallback exchanges in priority order
    pub fallback_exchanges: Vec<ExchangeId>,
    /// Expected execution time (milliseconds)
    pub expected_latency_ms: u64,
    /// Confidence score (0.0 to 1.0)
    pub confidence_score: f64,
    /// Reason for route selection
    pub selection_reason: String,
}

impl<C: ExecutionClient + Clone + Send + Sync + 'static> OrderRouter<C> {
    /// Create a new OrderRouter with the specified configuration
    pub fn new(
        clients: HashMap<ExchangeId, C>,
        config: SensorOrderConfig,
        exchange_mapping: HashMap<ExchangeIndex, ExchangeId>,
    ) -> Self {
        let max_concurrent = clients.len() * 10; // 10 orders per exchange max

        Self {
            clients,
            config,
            aggregators: Arc::new(RwLock::new(HashMap::new())),
            latency_monitor: LatencyMonitor::new(),
            risk_manager: RiskManager::new(),
            metrics: Arc::new(RwLock::new(OrderExecutionMetrics::default())),
            execution_semaphore: Arc::new(Semaphore::new(max_concurrent)),
            exchange_mapping,
        }
    }

    /// Add order book aggregator for an instrument
    pub async fn add_aggregator(
        &self,
        instrument: InstrumentNameExchange,
        aggregator: OrderBookAggregator,
    ) {
        let mut aggregators = self.aggregators.write().await;
        aggregators.insert(instrument, aggregator);
    }

    /// Route and execute an order with performance monitoring
    pub async fn execute_order(
        &self,
        order_request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Order<ExchangeId, InstrumentNameExchange, ActiveOrderState>, UnindexedOrderError>
    {
        let start_time = Instant::now();
        let _permit = self.execution_semaphore.acquire().await.unwrap();

        // Performance check - must complete within configured time limit
        let timeout = tokio::time::timeout(
            self.config.max_execution_time,
            self.execute_order_internal(order_request.clone()),
        );

        let timeout_result = timeout.await;
        match timeout_result {
            Ok(result) => {
                let execution_time = start_time.elapsed();
                let success = result.is_ok();

                // Update metrics
                let mut metrics = self.metrics.write().await;
                metrics.update_execution(execution_time, success);

                if execution_time > Duration::from_millis(400) {
                    warn!(
                        "Order execution took {}ms (>400ms warning threshold)",
                        execution_time.as_millis()
                    );
                }

                debug!(
                    "Order execution completed in {}ms, success: {}",
                    execution_time.as_millis(),
                    success
                );

                result
            }
            Err(_) => {
                error!(
                    "Order execution timed out after {}ms",
                    self.config.max_execution_time.as_millis()
                );
                let mut metrics = self.metrics.write().await;
                metrics.update_execution(self.config.max_execution_time, false);

                Err(UnindexedOrderError::Connectivity(
                    crate::error::ConnectivityError::Timeout,
                ))
            }
        }
    }

    /// Internal order execution logic
    async fn execute_order_internal(
        &self,
        order_request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Order<ExchangeId, InstrumentNameExchange, ActiveOrderState>, UnindexedOrderError>
    {
        // 1. Risk check
        self.risk_manager
            .check_order_risk(&order_request, &self.exchange_mapping)?;

        // 2. Route selection
        let route = self.select_optimal_route(&order_request).await?;

        info!(
            "Selected route for order: exchange={:?}, expected_latency={}ms, confidence={:.2}",
            route.primary_exchange, route.expected_latency_ms, route.confidence_score
        );

        // 3. Execute on primary exchange
        match self
            .execute_on_exchange(&order_request, route.primary_exchange)
            .await
        {
            Ok(order) => {
                // Note: In a more complete implementation, we would track latency here
                // self.latency_monitor.record_success(route.primary_exchange, route.expected_latency_ms).await;
                Ok(order)
            }
            Err(primary_error) => {
                warn!(
                    "Primary exchange {:?} failed: {:?}, trying fallbacks",
                    route.primary_exchange, primary_error
                );

                // Note: In a more complete implementation, we would track failures here
                // self.latency_monitor.record_failure(route.primary_exchange, "execution_failed").await;

                // 4. Try fallback exchanges
                for fallback_exchange in route.fallback_exchanges {
                    match self
                        .execute_on_exchange(&order_request, fallback_exchange)
                        .await
                    {
                        Ok(order) => {
                            info!(
                                "Order executed successfully on fallback exchange: {:?}",
                                fallback_exchange
                            );
                            return Ok(order);
                        }
                        Err(fallback_error) => {
                            warn!(
                                "Fallback exchange {:?} also failed: {:?}",
                                fallback_exchange, fallback_error
                            );
                        }
                    }
                }

                // All exchanges failed
                error!("All exchanges failed for order execution");
                Err(primary_error)
            }
        }
    }

    /// Select the optimal exchange route for the order
    async fn select_optimal_route(
        &self,
        order_request: &OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Result<RouteSelection, UnindexedOrderError> {
        let aggregators = self.aggregators.read().await;
        let instrument = &order_request.key.instrument;

        let aggregator = aggregators.get(instrument).ok_or_else(|| {
            UnindexedOrderError::Connectivity(crate::error::ConnectivityError::Socket(
                "No aggregator found for instrument".to_string(),
            ))
        })?;

        // Get available exchanges with their current status
        let mut candidates = Vec::new();

        for (exchange_id, _client) in &self.clients {
            let health = self.latency_monitor.get_exchange_health(exchange_id);
            let avg_latency = self.latency_monitor.get_average_latency(exchange_id);

            // Skip unavailable exchanges
            if matches!(health, ExchangeHealth::Unavailable { .. }) {
                continue;
            }

            // Check if exchange has adequate liquidity
            let liquidity_score = self
                .calculate_liquidity_score(aggregator, exchange_id)
                .await;

            // Calculate overall routing score
            let routing_score = self.calculate_routing_score(
                exchange_id,
                avg_latency,
                liquidity_score,
                &health,
                order_request,
            );

            candidates.push((*exchange_id, routing_score, avg_latency));
        }

        if candidates.is_empty() {
            return Err(UnindexedOrderError::Connectivity(
                crate::error::ConnectivityError::Socket(
                    "No available exchanges for routing".to_string(),
                ),
            ));
        }

        // Sort by routing score (highest first)
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let (primary_exchange, confidence_score, expected_latency) = candidates[0];
        let fallback_exchanges: Vec<ExchangeId> = candidates
            .iter()
            .skip(1)
            .take(3) // Top 3 fallbacks
            .map(|(ex, _, _)| *ex)
            .collect();

        Ok(RouteSelection {
            primary_exchange,
            fallback_exchanges,
            expected_latency_ms: expected_latency as u64,
            confidence_score,
            selection_reason: format!(
                "Selected based on latency ({}ms) and liquidity score ({:.2})",
                expected_latency, confidence_score
            ),
        })
    }

    /// Calculate liquidity score for an exchange
    async fn calculate_liquidity_score(
        &self,
        aggregator: &OrderBookAggregator,
        _exchange_id: &ExchangeId,
    ) -> f64 {
        // Use the aggregator to get liquidity information
        // This is a simplified implementation
        let aggregated = aggregator.aggregate(10);
        let total_volume = aggregated
            .bids()
            .levels()
            .iter()
            .map(|l| l.amount)
            .sum::<Decimal>()
            + aggregated
                .asks()
                .levels()
                .iter()
                .map(|l| l.amount)
                .sum::<Decimal>();

        // Normalize volume to a 0-1 score (simplified)
        (total_volume.to_f64().unwrap_or(0.0) / 100000.0).min(1.0)
    }

    /// Calculate routing score for exchange selection
    fn calculate_routing_score(
        &self,
        exchange_id: &ExchangeId,
        avg_latency: f64,
        liquidity_score: f64,
        health: &ExchangeHealth,
        order_request: &OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> f64 {
        let mut score = 0.0;

        // Latency component (lower is better, max weight 0.4)
        let latency_score = (500.0 - avg_latency.min(500.0)) / 500.0;
        score += latency_score * 0.4;

        // Liquidity component (higher is better, max weight 0.3)
        score += liquidity_score * 0.3;

        // Health component (max weight 0.2)
        let health_score = match health {
            ExchangeHealth::Healthy => 1.0,
            ExchangeHealth::Degraded { .. } => 0.6,
            ExchangeHealth::Unavailable { .. } => 0.0,
        };
        score += health_score * 0.2;

        // Order size compatibility (max weight 0.1)
        let size_score = if self.is_order_compatible(exchange_id, order_request) {
            1.0
        } else {
            0.0
        };
        score += size_score * 0.1;

        score
    }

    /// Check if order is compatible with exchange requirements
    fn is_order_compatible(
        &self,
        _exchange_id: &ExchangeId,
        _order_request: &OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> bool {
        // Simplified check - would verify minimum order sizes, supported order types, etc.
        true
    }

    /// Execute order on specific exchange
    async fn execute_on_exchange(
        &self,
        order_request: &OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
        exchange_id: ExchangeId,
    ) -> Result<Order<ExchangeId, InstrumentNameExchange, ActiveOrderState>, UnindexedOrderError>
    {
        let client = self.clients.get(&exchange_id).ok_or_else(|| {
            UnindexedOrderError::Connectivity(crate::error::ConnectivityError::Socket(
                "Exchange client not found".to_string(),
            ))
        })?;

        // Create order request for this exchange
        let exchange_request = order_request.clone();

        // Execute the order through the client
        let order_result = client.open_order(exchange_request).await;

        // Convert the order result from Order<..., Result<Open, ...>> to Result<Order<..., ActiveOrderState>, ...>
        match order_result.state {
            Ok(open_state) => Ok(Order {
                key: order_result.key,
                side: order_result.side,
                price: order_result.price,
                quantity: order_result.quantity,
                kind: order_result.kind,
                time_in_force: order_result.time_in_force,
                state: ActiveOrderState::Open(open_state),
            }),
            Err(error) => Err(error),
        }
    }

    /// Get current performance metrics
    pub async fn get_metrics(&self) -> OrderExecutionMetrics {
        self.metrics.read().await.clone()
    }

    /// Get routing statistics
    pub async fn get_routing_stats(&self) -> RoutingStats {
        RoutingStats {
            total_routes: self.metrics.read().await.total_orders,
            successful_routes: self.metrics.read().await.successful_executions,
            average_latency: self.metrics.read().await.average_execution_time,
            exchange_health: self.latency_monitor.exchange_health.clone(),
        }
    }

    /// Convert ExchangeId to ExchangeIndex using the reverse mapping
    fn get_exchange_index_for_id(
        &self,
        exchange_id: &ExchangeId,
    ) -> Result<ExchangeIndex, UnindexedOrderError> {
        // Find the ExchangeIndex for the given ExchangeId
        for (index, id) in &self.exchange_mapping {
            if id == exchange_id {
                return Ok(*index);
            }
        }

        Err(UnindexedOrderError::Connectivity(
            crate::error::ConnectivityError::Socket(format!(
                "No exchange index found for exchange ID {:?}",
                exchange_id
            )),
        ))
    }
}

/// Routing performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStats {
    pub total_routes: u64,
    pub successful_routes: u64,
    pub average_latency: Duration,
    pub exchange_health: HashMap<ExchangeId, ExchangeHealth>,
}

impl LatencyMonitor {
    pub fn new() -> Self {
        Self {
            average_latency: HashMap::new(),
            recent_samples: HashMap::new(),
            exchange_health: HashMap::new(),
        }
    }

    pub async fn record_success(&mut self, exchange_id: ExchangeId, latency_ms: u64) {
        // Update recent samples
        let samples = self
            .recent_samples
            .entry(exchange_id)
            .or_insert_with(Vec::new);
        samples.push(latency_ms);
        if samples.len() > 100 {
            samples.remove(0); // Keep only last 100 samples
        }

        // Update average latency
        let avg = samples.iter().sum::<u64>() as f64 / samples.len() as f64;
        self.average_latency.insert(exchange_id, avg);

        // Update health status
        let health = if avg < 100.0 {
            ExchangeHealth::Healthy
        } else if avg < 300.0 {
            ExchangeHealth::Degraded {
                reason: format!("High latency: {:.0}ms", avg),
            }
        } else {
            ExchangeHealth::Unavailable {
                reason: format!("Excessive latency: {:.0}ms", avg),
            }
        };

        self.exchange_health.insert(exchange_id, health);
    }

    pub async fn record_failure(&mut self, exchange_id: ExchangeId, reason: &str) {
        self.exchange_health.insert(
            exchange_id,
            ExchangeHealth::Unavailable {
                reason: reason.to_string(),
            },
        );
    }

    pub fn get_average_latency(&self, exchange_id: &ExchangeId) -> f64 {
        self.average_latency
            .get(exchange_id)
            .copied()
            .unwrap_or(200.0)
    }

    pub fn get_exchange_health(&self, exchange_id: &ExchangeId) -> ExchangeHealth {
        self.exchange_health
            .get(exchange_id)
            .cloned()
            .unwrap_or(ExchangeHealth::Healthy)
    }
}

impl RiskManager {
    pub fn new() -> Self {
        Self {
            position_limits: HashMap::new(),
            current_positions: HashMap::new(),
            daily_volume_limits: HashMap::new(),
            current_daily_volume: HashMap::new(),
            max_order_values: HashMap::new(),
        }
    }

    /// Check if order passes risk management rules
    pub fn check_order_risk(
        &self,
        order_request: &OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
        _exchange_mapping: &HashMap<ExchangeIndex, ExchangeId>,
    ) -> Result<(), UnindexedOrderError> {
        let exchange_id = &order_request.key.exchange;

        let order_value = order_request.state.price * order_request.state.quantity;

        // Check maximum order value
        if let Some(max_value) = self.max_order_values.get(exchange_id) {
            if order_value > *max_value {
                return Err(UnindexedOrderError::Connectivity(
                    crate::error::ConnectivityError::Socket(format!(
                        "Order value {} exceeds maximum {} for exchange {:?}",
                        order_value, max_value, exchange_id
                    )),
                ));
            }
        }

        // Check position limits
        if let Some(position_limit) = self.position_limits.get(exchange_id) {
            let current_position = self
                .current_positions
                .get(exchange_id)
                .copied()
                .unwrap_or(Decimal::ZERO);
            let new_position = match order_request.state.side {
                Side::Buy => current_position + order_request.state.quantity,
                Side::Sell => current_position - order_request.state.quantity,
            };

            if new_position.abs() > *position_limit {
                return Err(UnindexedOrderError::Connectivity(
                    crate::error::ConnectivityError::Socket(format!(
                        "Order would exceed position limit {} for exchange {:?}",
                        position_limit, exchange_id
                    )),
                ));
            }
        }

        // Check daily volume limits
        if let Some(daily_limit) = self.daily_volume_limits.get(exchange_id) {
            let current_daily = self
                .current_daily_volume
                .get(exchange_id)
                .copied()
                .unwrap_or(Decimal::ZERO);
            if current_daily + order_value > *daily_limit {
                return Err(UnindexedOrderError::Connectivity(
                    crate::error::ConnectivityError::Socket(format!(
                        "Order would exceed daily volume limit {} for exchange {:?}",
                        daily_limit, exchange_id
                    )),
                ));
            }
        }

        Ok(())
    }

    /// Update position tracking after order execution
    pub fn update_position(
        &mut self,
        exchange_id: ExchangeId,
        side: Side,
        quantity: Decimal,
        value: Decimal,
    ) {
        // Update position
        let current_position = self
            .current_positions
            .entry(exchange_id)
            .or_insert(Decimal::ZERO);
        match side {
            Side::Buy => *current_position += quantity,
            Side::Sell => *current_position -= quantity,
        }

        // Update daily volume
        let daily_volume = self
            .current_daily_volume
            .entry(exchange_id)
            .or_insert(Decimal::ZERO);
        *daily_volume += value;
    }
}
