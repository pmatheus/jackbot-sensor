use crate::{
    data_gathering::{InstrumentKey, MarketDataCollector},
    order::{
        id::{ClientOrderId, StrategyId},
        request::{OrderRequestOpen, RequestOpen},
        state::ActiveOrderState,
        Order, OrderKey, OrderKind, TimeInForce,
    },
};
use chrono::{DateTime, Utc};
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange, Side};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast, RwLock},
    time::interval,
};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Test order execution engine for validating trading strategies
#[derive(Debug)]
pub struct TestOrderExecutionEngine {
    /// Market data collector for real-time data
    market_data_collector: Arc<MarketDataCollector>,
    /// Active test orders
    active_orders: Arc<RwLock<HashMap<ClientOrderId, TestOrder>>>,
    /// Order execution results
    execution_results: Arc<RwLock<Vec<OrderExecutionResult>>>,
    /// Execution configuration
    config: TestExecutionConfig,
    /// Order update broadcaster
    order_updates: broadcast::Sender<OrderUpdateEvent>,
    /// Execution statistics
    stats: Arc<RwLock<ExecutionStatistics>>,
}

/// Test order with execution metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestOrder {
    /// Base order information
    pub order: Order<ExchangeId, InstrumentNameExchange, ActiveOrderState>,
    /// Order submission timestamp
    pub submitted_at: DateTime<Utc>,
    /// Expected execution timestamp (for simulation)
    pub expected_execution_at: Option<DateTime<Utc>>,
    /// Execution status
    pub execution_status: ExecutionStatus,
    /// Partial fills
    pub partial_fills: Vec<PartialFill>,
    /// Total filled quantity
    pub filled_quantity: Decimal,
    /// Average fill price
    pub average_fill_price: Option<Decimal>,
    /// Execution latency (when filled)
    pub execution_latency: Option<Duration>,
}

/// Order execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pending,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Failed(String),
}

/// Partial fill information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialFill {
    /// Fill ID
    pub fill_id: String,
    /// Filled quantity
    pub quantity: Decimal,
    /// Fill price
    pub price: Decimal,
    /// Fill timestamp
    pub timestamp: DateTime<Utc>,
    /// Fill side
    pub side: Side,
}

/// Order execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderExecutionResult {
    /// Order ID
    pub order_id: ClientOrderId,
    /// Strategy ID
    pub strategy_id: StrategyId,
    /// Exchange
    pub exchange: ExchangeId,
    /// Instrument
    pub instrument: InstrumentNameExchange,
    /// Order side
    pub side: Side,
    /// Order quantity
    pub quantity: Decimal,
    /// Order price
    pub price: Decimal,
    /// Order type
    pub order_type: OrderKind,
    /// Time in force
    pub time_in_force: TimeInForce,
    /// Execution status
    pub execution_status: ExecutionStatus,
    /// Total filled quantity
    pub filled_quantity: Decimal,
    /// Average execution price
    pub average_execution_price: Option<Decimal>,
    /// Order submission time
    pub submitted_at: DateTime<Utc>,
    /// First fill time
    pub first_fill_at: Option<DateTime<Utc>>,
    /// Last fill time
    pub last_fill_at: Option<DateTime<Utc>>,
    /// Total execution time
    pub execution_duration: Option<Duration>,
    /// All partial fills
    pub fills: Vec<PartialFill>,
    /// Execution fees
    pub fees: Option<Decimal>,
    /// Execution quality metrics
    pub quality_metrics: ExecutionQualityMetrics,
}

/// Execution quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionQualityMetrics {
    /// Slippage from expected price
    pub slippage: Option<Decimal>,
    /// Time to first fill
    pub time_to_first_fill: Option<Duration>,
    /// Time to complete fill
    pub time_to_complete_fill: Option<Duration>,
    /// Fill rate (percentage of order filled)
    pub fill_rate: Decimal,
    /// Price improvement (if any)
    pub price_improvement: Option<Decimal>,
    /// Market impact estimate
    pub market_impact: Option<Decimal>,
}

/// Order update event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderUpdateEvent {
    /// Order submitted
    OrderSubmitted {
        order_id: ClientOrderId,
        order_details: TestOrder,
    },
    /// Order partially filled
    PartialFill {
        order_id: ClientOrderId,
        fill: PartialFill,
        remaining_quantity: Decimal,
    },
    /// Order completely filled
    OrderFilled {
        order_id: ClientOrderId,
        final_fill: PartialFill,
        execution_result: OrderExecutionResult,
    },
    /// Order cancelled
    OrderCancelled {
        order_id: ClientOrderId,
        reason: String,
    },
    /// Order rejected
    OrderRejected {
        order_id: ClientOrderId,
        reason: String,
    },
}

/// Test execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestExecutionConfig {
    /// Simulate realistic execution delays
    pub simulate_execution_delays: bool,
    /// Base execution delay (milliseconds)
    pub base_execution_delay_ms: u64,
    /// Maximum execution delay (milliseconds)
    pub max_execution_delay_ms: u64,
    /// Probability of partial fills (0.0 to 1.0)
    pub partial_fill_probability: f64,
    /// Minimum partial fill size (percentage of order)
    pub min_partial_fill_percentage: f64,
    /// Simulate slippage
    pub simulate_slippage: bool,
    /// Maximum slippage (basis points)
    pub max_slippage_bps: u32,
    /// Simulate order rejections
    pub simulate_rejections: bool,
    /// Rejection probability (0.0 to 1.0)
    pub rejection_probability: f64,
    /// Enable market impact simulation
    pub enable_market_impact: bool,
    /// Market impact factor
    pub market_impact_factor: f64,
}

impl Default for TestExecutionConfig {
    fn default() -> Self {
        Self {
            simulate_execution_delays: true,
            base_execution_delay_ms: 10,
            max_execution_delay_ms: 100,
            partial_fill_probability: 0.3,
            min_partial_fill_percentage: 0.1,
            simulate_slippage: true,
            max_slippage_bps: 10,
            simulate_rejections: false,
            rejection_probability: 0.01,
            enable_market_impact: true,
            market_impact_factor: 0.001,
        }
    }
}

/// Execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStatistics {
    /// Total orders submitted
    pub total_orders: u64,
    /// Orders filled
    pub orders_filled: u64,
    /// Orders partially filled
    pub orders_partially_filled: u64,
    /// Orders cancelled
    pub orders_cancelled: u64,
    /// Orders rejected
    pub orders_rejected: u64,
    /// Average execution time
    pub avg_execution_time_ms: f64,
    /// Average slippage
    pub avg_slippage_bps: f64,
    /// Average fill rate
    pub avg_fill_rate: f64,
    /// Total volume executed
    pub total_volume_executed: Decimal,
    /// Last updated
    pub last_updated: DateTime<Utc>,
}

impl Default for ExecutionStatistics {
    fn default() -> Self {
        Self {
            total_orders: 0,
            orders_filled: 0,
            orders_partially_filled: 0,
            orders_cancelled: 0,
            orders_rejected: 0,
            avg_execution_time_ms: 0.0,
            avg_slippage_bps: 0.0,
            avg_fill_rate: 0.0,
            total_volume_executed: Decimal::ZERO,
            last_updated: Utc::now(),
        }
    }
}

impl TestOrderExecutionEngine {
    /// Create new test execution engine
    pub fn new(
        market_data_collector: Arc<MarketDataCollector>,
        config: TestExecutionConfig,
    ) -> Self {
        let (order_sender, _) = broadcast::channel(1000);

        Self {
            market_data_collector,
            active_orders: Arc::new(RwLock::new(HashMap::new())),
            execution_results: Arc::new(RwLock::new(Vec::new())),
            config,
            order_updates: order_sender,
            stats: Arc::new(RwLock::new(ExecutionStatistics::default())),
        }
    }

    /// Start the execution engine
    pub async fn start(&mut self) -> Result<(), ExecutionError> {
        info!("Starting test order execution engine");

        // Start order processing task
        self.start_order_processor().await;

        // Start statistics updater
        self.start_statistics_updater().await;

        info!("Test order execution engine started");
        Ok(())
    }

    /// Submit a test order for execution
    pub async fn submit_order(
        &self,
        order_request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Result<ClientOrderId, ExecutionError> {
        let order_id = order_request.key.cid.clone();
        let timestamp = Utc::now();

        info!(
            "Submitting test order: {} for {}",
            order_id, order_request.key.instrument
        );

        // Create test order
        let test_order = TestOrder {
            order: Order::from(&order_request),
            submitted_at: timestamp,
            expected_execution_at: self.calculate_expected_execution_time(timestamp).await,
            execution_status: ExecutionStatus::Pending,
            partial_fills: Vec::new(),
            filled_quantity: Decimal::ZERO,
            average_fill_price: None,
            execution_latency: None,
        };

        // Store order
        {
            let mut orders = self.active_orders.write().await;
            orders.insert(order_id.clone(), test_order.clone());
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_orders += 1;
            stats.last_updated = Utc::now();
        }

        // Broadcast order submission event
        let event = OrderUpdateEvent::OrderSubmitted {
            order_id: order_id.clone(),
            order_details: test_order,
        };
        let _ = self.order_updates.send(event);

        // Check for immediate rejection
        if self.should_reject_order().await {
            self.reject_order(order_id.clone(), "Simulated rejection".to_string())
                .await?;
        }

        Ok(order_id)
    }

    /// Cancel an active order
    pub async fn cancel_order(&self, order_id: ClientOrderId) -> Result<(), ExecutionError> {
        let mut orders = self.active_orders.write().await;

        if let Some(mut order) = orders.remove(&order_id) {
            order.execution_status = ExecutionStatus::Cancelled;

            // Update statistics
            {
                let mut stats = self.stats.write().await;
                stats.orders_cancelled += 1;
                stats.last_updated = Utc::now();
            }

            // Create execution result
            let execution_result = self.create_execution_result(&order).await;

            // Store result
            {
                let mut results = self.execution_results.write().await;
                results.push(execution_result);
            }

            // Broadcast cancellation event
            let event = OrderUpdateEvent::OrderCancelled {
                order_id: order_id.clone(),
                reason: "User cancellation".to_string(),
            };
            let _ = self.order_updates.send(event);

            info!("Order {} cancelled", order_id);
            Ok(())
        } else {
            Err(ExecutionError::OrderNotFound(order_id))
        }
    }

    /// Get order execution result
    pub async fn get_execution_result(
        &self,
        order_id: ClientOrderId,
    ) -> Option<OrderExecutionResult> {
        let results = self.execution_results.read().await;
        results.iter().find(|r| r.order_id == order_id).cloned()
    }

    /// Get all execution results
    pub async fn get_all_execution_results(&self) -> Vec<OrderExecutionResult> {
        let results = self.execution_results.read().await;
        results.clone()
    }

    /// Get current execution statistics
    pub async fn get_statistics(&self) -> ExecutionStatistics {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Subscribe to order updates
    pub fn subscribe_to_updates(&self) -> broadcast::Receiver<OrderUpdateEvent> {
        self.order_updates.subscribe()
    }

    /// Start order processing task
    async fn start_order_processor(&self) {
        let orders = Arc::clone(&self.active_orders);
        let execution_results = Arc::clone(&self.execution_results);
        let market_data = Arc::clone(&self.market_data_collector);
        let order_updates = self.order_updates.clone();
        let config = self.config.clone();
        let stats = Arc::clone(&self.stats);

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100)); // Check every 100ms

            loop {
                interval.tick().await;

                // Get orders ready for execution
                let orders_to_process: Vec<_> = {
                    let orders_guard = orders.read().await;
                    orders_guard
                        .iter()
                        .filter(|(_, order)| {
                            order.execution_status == ExecutionStatus::Pending
                                && order.expected_execution_at.is_none_or(|t| Utc::now() >= t)
                        })
                        .map(|(id, order)| (id.clone(), order.clone()))
                        .collect()
                };

                for (order_id, order) in orders_to_process {
                    if let Err(e) = Self::process_order_execution(
                        order_id.clone(),
                        &order,
                        &orders,
                        &execution_results,
                        &market_data,
                        &order_updates,
                        &config,
                        &stats,
                    )
                    .await
                    {
                        error!("Error processing order {}: {}", order_id, e);
                    }
                }
            }
        });
    }

    /// Process individual order execution
    async fn process_order_execution(
        order_id: ClientOrderId,
        order: &TestOrder,
        orders: &Arc<RwLock<HashMap<ClientOrderId, TestOrder>>>,
        execution_results: &Arc<RwLock<Vec<OrderExecutionResult>>>,
        market_data: &Arc<MarketDataCollector>,
        order_updates: &broadcast::Sender<OrderUpdateEvent>,
        config: &TestExecutionConfig,
        stats: &Arc<RwLock<ExecutionStatistics>>,
    ) -> Result<(), ExecutionError> {
        // Get current market data
        let instrument_key =
            InstrumentKey::new(order.order.key.exchange, order.order.key.instrument.clone());

        let market_price =
            market_data
                .get_market_data(&instrument_key)
                .await
                .map(|data| match order.order.side {
                    Side::Buy => data.ask,
                    Side::Sell => data.bid,
                });

        // Determine if order should fill
        let should_fill = Self::should_order_fill(&order.order, market_price, config).await;

        if should_fill {
            Self::execute_order_fill(
                order_id,
                order,
                market_price,
                orders,
                execution_results,
                order_updates,
                config,
                stats,
            )
            .await?;
        }

        Ok(())
    }

    /// Check if order should fill based on market conditions
    async fn should_order_fill(
        order: &Order<ExchangeId, InstrumentNameExchange, ActiveOrderState>,
        market_price: Option<Decimal>,
        _config: &TestExecutionConfig,
    ) -> bool {
        match order.kind {
            OrderKind::Market => true,
            OrderKind::Limit => {
                if let Some(market_price) = market_price {
                    match order.side {
                        Side::Buy => market_price <= order.price,
                        Side::Sell => market_price >= order.price,
                    }
                } else {
                    false
                }
            }
            _ => false, // Other order types not implemented in this test
        }
    }

    /// Execute order fill
    async fn execute_order_fill(
        order_id: ClientOrderId,
        order: &TestOrder,
        market_price: Option<Decimal>,
        orders: &Arc<RwLock<HashMap<ClientOrderId, TestOrder>>>,
        execution_results: &Arc<RwLock<Vec<OrderExecutionResult>>>,
        order_updates: &broadcast::Sender<OrderUpdateEvent>,
        config: &TestExecutionConfig,
        stats: &Arc<RwLock<ExecutionStatistics>>,
    ) -> Result<(), ExecutionError> {
        let fill_price = Self::calculate_fill_price(&order.order, market_price, config).await;

        // Determine fill quantity (partial or full)
        let remaining_quantity = order.order.quantity - order.filled_quantity;
        let fill_quantity = if config.partial_fill_probability > 0.0
            && fastrand::f64() < config.partial_fill_probability
        {
            // Partial fill
            let min_fill = remaining_quantity
                * Decimal::try_from(config.min_partial_fill_percentage)
                    .unwrap_or(Decimal::new(1, 1));
            let fill_range = remaining_quantity - min_fill;
            min_fill
                + (fill_range * Decimal::try_from(fastrand::f64()).unwrap_or(Decimal::new(5, 1)))
        } else {
            // Full fill
            remaining_quantity
        };

        // Create partial fill
        let partial_fill = PartialFill {
            fill_id: Uuid::new_v4().to_string(),
            quantity: fill_quantity,
            price: fill_price,
            timestamp: Utc::now(),
            side: order.order.side,
        };

        // Update order
        let mut orders_guard = orders.write().await;
        if let Some(order) = orders_guard.get_mut(&order_id) {
            order.partial_fills.push(partial_fill.clone());
            order.filled_quantity += fill_quantity;
            order.execution_latency = Some(
                Utc::now()
                    .signed_duration_since(order.submitted_at)
                    .to_std()
                    .unwrap_or(Duration::ZERO),
            );

            // Calculate average fill price
            let total_value: Decimal = order
                .partial_fills
                .iter()
                .map(|fill| fill.price * fill.quantity)
                .sum();
            order.average_fill_price = Some(total_value / order.filled_quantity);

            let is_fully_filled = order.filled_quantity >= order.order.quantity;

            if is_fully_filled {
                order.execution_status = ExecutionStatus::Filled;

                // Update statistics
                {
                    let mut stats_guard = stats.write().await;
                    stats_guard.orders_filled += 1;
                    stats_guard.total_volume_executed += order.filled_quantity;
                    stats_guard.last_updated = Utc::now();
                }

                // Create final execution result
                let execution_result = Self::create_execution_result_from_order(order).await;

                // Store result
                {
                    let mut results = execution_results.write().await;
                    results.push(execution_result.clone());
                }

                // Broadcast completion event
                let event = OrderUpdateEvent::OrderFilled {
                    order_id: order_id.clone(),
                    final_fill: partial_fill,
                    execution_result,
                };
                let _ = order_updates.send(event);

                // Remove from active orders
                orders_guard.remove(&order_id);

                info!("Order {} fully filled", order_id);
            } else {
                order.execution_status = ExecutionStatus::PartiallyFilled;

                // Update statistics
                {
                    let mut stats_guard = stats.write().await;
                    stats_guard.orders_partially_filled += 1;
                    stats_guard.total_volume_executed += fill_quantity;
                    stats_guard.last_updated = Utc::now();
                }

                // Broadcast partial fill event
                let event = OrderUpdateEvent::PartialFill {
                    order_id: order_id.clone(),
                    fill: partial_fill,
                    remaining_quantity: order.order.quantity - order.filled_quantity,
                };
                let _ = order_updates.send(event);

                info!(
                    "Order {} partially filled: {}/{}",
                    order_id, order.filled_quantity, order.order.quantity
                );
            }
        }

        Ok(())
    }

    /// Calculate fill price with slippage simulation
    async fn calculate_fill_price(
        order: &Order<ExchangeId, InstrumentNameExchange, ActiveOrderState>,
        market_price: Option<Decimal>,
        config: &TestExecutionConfig,
    ) -> Decimal {
        let base_price = match order.kind {
            OrderKind::Market => market_price.unwrap_or(order.price),
            OrderKind::Limit => order.price,
            _ => order.price,
        };

        if config.simulate_slippage && order.kind == OrderKind::Market {
            let slippage_bps = fastrand::u32(0..=config.max_slippage_bps);
            let slippage_factor = Decimal::new(slippage_bps as i64, 4); // Convert basis points to decimal

            match order.side {
                Side::Buy => base_price * (Decimal::ONE + slippage_factor),
                Side::Sell => base_price * (Decimal::ONE - slippage_factor),
            }
        } else {
            base_price
        }
    }

    /// Create execution result from order
    async fn create_execution_result_from_order(order: &TestOrder) -> OrderExecutionResult {
        let first_fill_at = order.partial_fills.first().map(|f| f.timestamp);
        let last_fill_at = order.partial_fills.last().map(|f| f.timestamp);

        OrderExecutionResult {
            order_id: order.order.key.cid.clone(),
            strategy_id: order.order.key.strategy.clone(),
            exchange: order.order.key.exchange,
            instrument: order.order.key.instrument.clone(),
            side: order.order.side,
            quantity: order.order.quantity,
            price: order.order.price,
            order_type: order.order.kind,
            time_in_force: order.order.time_in_force,
            execution_status: order.execution_status.clone(),
            filled_quantity: order.filled_quantity,
            average_execution_price: order.average_fill_price,
            submitted_at: order.submitted_at,
            first_fill_at,
            last_fill_at,
            execution_duration: order.execution_latency,
            fills: order.partial_fills.clone(),
            fees: Some(order.filled_quantity * Decimal::new(1, 4)), // 0.01% fee
            quality_metrics: ExecutionQualityMetrics {
                slippage: None, // Calculate based on expected vs actual price
                time_to_first_fill: first_fill_at.map(|t| {
                    t.signed_duration_since(order.submitted_at)
                        .to_std()
                        .unwrap_or(Duration::ZERO)
                }),
                time_to_complete_fill: last_fill_at.map(|t| {
                    t.signed_duration_since(order.submitted_at)
                        .to_std()
                        .unwrap_or(Duration::ZERO)
                }),
                fill_rate: order.filled_quantity / order.order.quantity * Decimal::from(100),
                price_improvement: None,
                market_impact: None,
            },
        }
    }

    /// Calculate expected execution time
    async fn calculate_expected_execution_time(
        &self,
        submission_time: DateTime<Utc>,
    ) -> Option<DateTime<Utc>> {
        if self.config.simulate_execution_delays {
            let delay_ms = fastrand::u64(
                self.config.base_execution_delay_ms..=self.config.max_execution_delay_ms,
            );
            Some(submission_time + chrono::Duration::milliseconds(delay_ms as i64))
        } else {
            None
        }
    }

    /// Check if order should be rejected
    async fn should_reject_order(&self) -> bool {
        self.config.simulate_rejections && fastrand::f64() < self.config.rejection_probability
    }

    /// Reject an order
    async fn reject_order(
        &self,
        order_id: ClientOrderId,
        reason: String,
    ) -> Result<(), ExecutionError> {
        let mut orders = self.active_orders.write().await;

        if let Some(mut order) = orders.remove(&order_id) {
            order.execution_status = ExecutionStatus::Rejected;

            // Update statistics
            {
                let mut stats = self.stats.write().await;
                stats.orders_rejected += 1;
                stats.last_updated = Utc::now();
            }

            // Create execution result
            let execution_result = self.create_execution_result(&order).await;

            // Store result
            {
                let mut results = self.execution_results.write().await;
                results.push(execution_result);
            }

            // Broadcast rejection event
            let event = OrderUpdateEvent::OrderRejected {
                order_id: order_id.clone(),
                reason: reason.clone(),
            };
            let _ = self.order_updates.send(event);

            warn!("Order {} rejected: {}", order_id, reason);
        }

        Ok(())
    }

    /// Create execution result from test order
    async fn create_execution_result(&self, order: &TestOrder) -> OrderExecutionResult {
        Self::create_execution_result_from_order(order).await
    }

    /// Start statistics updater
    async fn start_statistics_updater(&self) {
        let stats = Arc::clone(&self.stats);
        let results = Arc::clone(&self.execution_results);
        let mut interval = interval(Duration::from_secs(60)); // Update every minute

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                let mut stats_guard = stats.write().await;
                let results_guard = results.read().await;

                // Calculate average metrics
                if !results_guard.is_empty() {
                    let total_execution_time: Duration = results_guard
                        .iter()
                        .filter_map(|r| r.execution_duration)
                        .sum();
                    stats_guard.avg_execution_time_ms =
                        total_execution_time.as_millis() as f64 / results_guard.len() as f64;

                    let total_fill_rate: Decimal = results_guard
                        .iter()
                        .map(|r| r.quality_metrics.fill_rate)
                        .sum();
                    stats_guard.avg_fill_rate = (total_fill_rate
                        / Decimal::from(results_guard.len()))
                    .to_f64()
                    .unwrap_or(0.0);
                }

                stats_guard.last_updated = Utc::now();
            }
        });
    }
}

/// Execution error types
#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("Order not found: {0}")]
    OrderNotFound(ClientOrderId),
    #[error("Invalid order: {0}")]
    InvalidOrder(String),
    #[error("Market data unavailable: {0}")]
    MarketDataUnavailable(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
}

/// Helper function to create test order requests
pub fn create_test_limit_order(
    exchange: ExchangeId,
    instrument: InstrumentNameExchange,
    side: Side,
    quantity: Decimal,
    price: Decimal,
    strategy_id: StrategyId,
) -> OrderRequestOpen<ExchangeId, InstrumentNameExchange> {
    let order_key = OrderKey {
        exchange,
        instrument,
        strategy: strategy_id,
        cid: ClientOrderId::new(
            "limit_order_".to_string() + &uuid::Uuid::new_v4().to_string()[..8],
        ),
    };

    OrderRequestOpen {
        key: order_key,
        state: RequestOpen {
            side,
            price,
            quantity,
            kind: OrderKind::Limit,
            time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
        },
    }
}

/// Helper function to create test market orders
pub fn create_test_market_order(
    exchange: ExchangeId,
    instrument: InstrumentNameExchange,
    side: Side,
    quantity: Decimal,
    strategy_id: StrategyId,
) -> OrderRequestOpen<ExchangeId, InstrumentNameExchange> {
    let order_key = OrderKey {
        exchange,
        instrument,
        strategy: strategy_id,
        cid: ClientOrderId::new(
            "market_order_".to_string() + &uuid::Uuid::new_v4().to_string()[..8],
        ),
    };

    OrderRequestOpen {
        key: order_key,
        state: RequestOpen {
            side,
            price: Decimal::ZERO, // Market order doesn't need price
            quantity,
            kind: OrderKind::Market,
            time_in_force: TimeInForce::ImmediateOrCancel,
        },
    }
}
