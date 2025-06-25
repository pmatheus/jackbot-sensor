use crate::order::{
    executor::{ExecutionResult, PendingOrdersStats},
    sensor::OrderExecutionMetrics,
    OrderKind,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::{sync::RwLock, time::Duration};
use tracing::debug;

/// Real-time analytics and performance tracking for sensor orders
///
/// Provides comprehensive metrics, latency analysis, and performance insights
/// for high-frequency sensor-specific trading operations.
#[derive(Debug)]
pub struct OrderAnalytics {
    /// Real-time performance metrics
    metrics: Arc<RwLock<OrderExecutionMetrics>>,
    /// Historical performance data
    historical_data: Arc<RwLock<HistoricalData>>,
    /// Exchange-specific analytics
    exchange_analytics: Arc<RwLock<HashMap<ExchangeId, ExchangeAnalytics>>>,
    /// Order type performance breakdown
    order_type_metrics: Arc<RwLock<HashMap<OrderKind, OrderTypeMetrics>>>,
    /// Real-time event stream for monitoring
    event_stream: Arc<RwLock<VecDeque<AnalyticsEvent>>>,
    /// Configuration for analytics collection
    config: AnalyticsConfig,
}

/// Configuration for analytics collection and retention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsConfig {
    /// Maximum events to keep in memory
    pub max_events: usize,
    /// Historical data retention period
    pub retention_period: ChronoDuration,
    /// Sampling rate for detailed metrics (0.0 to 1.0)
    pub sampling_rate: f64,
    /// Enable real-time alerting
    pub enable_alerts: bool,
    /// Performance thresholds for alerting
    pub alert_thresholds: AlertThresholds,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            max_events: 10000,
            retention_period: ChronoDuration::hours(24),
            sampling_rate: 1.0, // 100% sampling by default
            enable_alerts: true,
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

/// Thresholds for performance alerting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Maximum acceptable execution time (ms)
    pub max_execution_time_ms: u64,
    /// Minimum acceptable success rate (0.0 to 1.0)
    pub min_success_rate: f64,
    /// Maximum acceptable latency per exchange (ms)
    pub max_exchange_latency_ms: u64,
    /// Minimum required liquidity score
    pub min_liquidity_score: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_execution_time_ms: 500,   // 500ms max execution
            min_success_rate: 0.95,       // 95% success rate
            max_exchange_latency_ms: 200, // 200ms max exchange latency
            min_liquidity_score: 0.7,     // 70% liquidity score
        }
    }
}

/// Historical data storage for trend analysis
#[derive(Debug, Clone, Default)]
pub struct HistoricalData {
    /// Execution times over time (timestamp, execution_time_ms)
    pub execution_times: VecDeque<(DateTime<Utc>, u64)>,
    /// Success rates over time (timestamp, success_rate)
    pub success_rates: VecDeque<(DateTime<Utc>, f64)>,
    /// Order volumes over time (timestamp, volume)
    pub volumes: VecDeque<(DateTime<Utc>, Decimal)>,
    /// Performance snapshots at regular intervals
    pub snapshots: VecDeque<PerformanceSnapshot>,
}

/// Performance snapshot at a specific point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    pub timestamp: DateTime<Utc>,
    pub total_orders: u64,
    pub successful_orders: u64,
    pub average_execution_time_ms: u64,
    pub total_volume: Decimal,
    pub active_exchanges: usize,
    pub sensor_order_breakdown: HashMap<String, u64>, // order type -> count
}

/// Exchange-specific analytics
#[derive(Debug, Clone, Default)]
pub struct ExchangeAnalytics {
    /// Orders executed on this exchange
    pub orders_executed: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Average latency (milliseconds)
    pub average_latency_ms: f64,
    /// Total volume traded
    pub total_volume: Decimal,
    /// Recent latency samples
    pub latency_samples: VecDeque<(DateTime<Utc>, u64)>,
    /// Health status history
    pub health_history: VecDeque<(DateTime<Utc>, String)>,
}

/// Order type specific metrics
#[derive(Debug, Clone, Default)]
pub struct OrderTypeMetrics {
    /// Total orders of this type
    pub total_orders: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Average execution time
    pub average_execution_time: Duration,
    /// Average confidence score (for sensor orders)
    pub average_confidence_score: Option<f64>,
    /// Hit rate (for Jackpot orders)
    pub hit_rate: Option<f64>,
    /// Prediction accuracy (for Prophetic orders)
    pub prediction_accuracy: Option<f64>,
}

/// Real-time analytics events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalyticsEvent {
    OrderExecuted {
        timestamp: DateTime<Utc>,
        order_id: String,
        order_type: OrderKind,
        exchange: ExchangeId,
        execution_time_ms: u64,
        success: bool,
        confidence_score: Option<f64>,
    },
    PerformanceAlert {
        timestamp: DateTime<Utc>,
        alert_type: AlertType,
        message: String,
        severity: AlertSeverity,
    },
    SystemHealth {
        timestamp: DateTime<Utc>,
        metric: String,
        value: f64,
        threshold: f64,
        status: HealthStatus,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    ExecutionTimeExceeded,
    SuccessRateDropped,
    ExchangeLatencyHigh,
    LiquidityInsufficient,
    SystemOverload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
}

/// Comprehensive analytics dashboard data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsDashboard {
    /// Current timestamp
    pub timestamp: DateTime<Utc>,
    /// Overall system performance
    pub system_performance: SystemPerformance,
    /// Exchange breakdown
    pub exchange_breakdown: HashMap<ExchangeId, ExchangePerformance>,
    /// Order type breakdown
    pub order_type_breakdown: HashMap<String, OrderTypePerformance>,
    /// Recent performance trends
    pub performance_trends: PerformanceTrends,
    /// Active alerts
    pub active_alerts: Vec<AnalyticsEvent>,
    /// Real-time metrics
    pub real_time_metrics: RealTimeMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPerformance {
    pub total_orders: u64,
    pub success_rate: f64,
    pub average_execution_time_ms: u64,
    pub orders_per_second: f64,
    pub total_volume: Decimal,
    pub system_health: HealthStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangePerformance {
    pub exchange_id: ExchangeId,
    pub orders_executed: u64,
    pub success_rate: f64,
    pub average_latency_ms: f64,
    pub volume_share: f64,
    pub health_status: HealthStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderTypePerformance {
    pub order_type: String,
    pub count: u64,
    pub success_rate: f64,
    pub average_execution_time_ms: u64,
    pub performance_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTrends {
    pub execution_time_trend: TrendDirection,
    pub success_rate_trend: TrendDirection,
    pub volume_trend: TrendDirection,
    pub trend_period_hours: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrendDirection {
    Improving,
    Stable,
    Degrading,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealTimeMetrics {
    pub pending_orders: PendingOrdersStats,
    pub recent_execution_times: Vec<u64>, // Last 20 execution times
    pub current_throughput: f64,          // Orders per second
    pub active_connections: usize,
}

impl OrderAnalytics {
    /// Create a new OrderAnalytics instance
    pub fn new(config: AnalyticsConfig) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(OrderExecutionMetrics::default())),
            historical_data: Arc::new(RwLock::new(HistoricalData::default())),
            exchange_analytics: Arc::new(RwLock::new(HashMap::new())),
            order_type_metrics: Arc::new(RwLock::new(HashMap::new())),
            event_stream: Arc::new(RwLock::new(VecDeque::new())),
            config,
        }
    }

    /// Record order execution result
    pub async fn record_execution(&self, result: ExecutionResult) {
        let timestamp = Utc::now();

        // Update main metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.update_execution(
                Duration::from_millis(result.execution_time_ms),
                result.success,
            );
        }

        // Update exchange-specific analytics
        if let Some(exchange_id) = result.exchange_used {
            let mut exchange_analytics = self.exchange_analytics.write().await;
            let exchange_data = exchange_analytics.entry(exchange_id).or_default();

            exchange_data.orders_executed += 1;
            if result.success {
                exchange_data.successful_executions += 1;
            }

            // Update average latency
            let new_latency = result.execution_time_ms as f64;
            if exchange_data.orders_executed == 1 {
                exchange_data.average_latency_ms = new_latency;
            } else {
                exchange_data.average_latency_ms = (exchange_data.average_latency_ms
                    * (exchange_data.orders_executed - 1) as f64
                    + new_latency)
                    / exchange_data.orders_executed as f64;
            }

            // Add latency sample
            exchange_data
                .latency_samples
                .push_back((timestamp, result.execution_time_ms));
            if exchange_data.latency_samples.len() > 1000 {
                exchange_data.latency_samples.pop_front();
            }
        }

        // Update order type metrics if sensor type is available
        if let Some(sensor_type) = &result.sensor_type {
            if let Ok(order_kind) = sensor_type.parse::<OrderKind>() {
                let mut order_type_metrics = self.order_type_metrics.write().await;
                let type_data = order_type_metrics.entry(order_kind).or_default();

                type_data.total_orders += 1;
                if result.success {
                    type_data.successful_executions += 1;
                }

                // Update average execution time
                let new_time = Duration::from_millis(result.execution_time_ms);
                if type_data.total_orders == 1 {
                    type_data.average_execution_time = new_time;
                } else {
                    let total_time = type_data.average_execution_time.as_millis() as u64
                        * (type_data.total_orders - 1)
                        + result.execution_time_ms;
                    type_data.average_execution_time =
                        Duration::from_millis(total_time / type_data.total_orders);
                }

                // Update confidence score for sensor orders
                if let Some(confidence) = result.confidence_score {
                    type_data.average_confidence_score = Some(
                        type_data
                            .average_confidence_score
                            .map(|avg| {
                                (avg * (type_data.total_orders - 1) as f64 + confidence)
                                    / type_data.total_orders as f64
                            })
                            .unwrap_or(confidence),
                    );
                }
            }
        }

        // Add to event stream
        let event = AnalyticsEvent::OrderExecuted {
            timestamp,
            order_id: result.order_id.to_string(),
            order_type: OrderKind::Market, // Default if not parsed
            exchange: result.exchange_used.unwrap_or(ExchangeId::Mock),
            execution_time_ms: result.execution_time_ms,
            success: result.success,
            confidence_score: result.confidence_score,
        };

        self.add_event(event).await;

        // Check for performance alerts
        self.check_performance_alerts(&result).await;

        debug!(
            "Recorded execution: id={}, success={}, time={}ms",
            result.order_id, result.success, result.execution_time_ms
        );
    }

    /// Add analytics event to the stream
    pub async fn add_event(&self, event: AnalyticsEvent) {
        let mut events = self.event_stream.write().await;
        events.push_back(event);

        // Maintain event limit
        while events.len() > self.config.max_events {
            events.pop_front();
        }
    }

    /// Check for performance alerts based on execution result
    async fn check_performance_alerts(&self, result: &ExecutionResult) {
        if !self.config.enable_alerts {
            return;
        }

        let timestamp = Utc::now();
        let thresholds = &self.config.alert_thresholds;

        // Check execution time threshold
        if result.execution_time_ms > thresholds.max_execution_time_ms {
            let alert = AnalyticsEvent::PerformanceAlert {
                timestamp,
                alert_type: AlertType::ExecutionTimeExceeded,
                message: format!(
                    "Order execution time {}ms exceeded threshold {}ms",
                    result.execution_time_ms, thresholds.max_execution_time_ms
                ),
                severity: if result.execution_time_ms > thresholds.max_execution_time_ms * 2 {
                    AlertSeverity::Critical
                } else {
                    AlertSeverity::Warning
                },
            };

            self.add_event(alert).await;
        }

        // Check success rate (based on recent history)
        let metrics = self.metrics.read().await;
        if metrics.success_rate() < thresholds.min_success_rate {
            let alert = AnalyticsEvent::PerformanceAlert {
                timestamp,
                alert_type: AlertType::SuccessRateDropped,
                message: format!(
                    "Success rate {:.2}% below threshold {:.2}%",
                    metrics.success_rate() * 100.0,
                    thresholds.min_success_rate * 100.0
                ),
                severity: AlertSeverity::Warning,
            };

            self.add_event(alert).await;
        }
    }

    /// Generate comprehensive analytics dashboard
    pub async fn generate_dashboard(&self) -> AnalyticsDashboard {
        let timestamp = Utc::now();

        // Get current metrics
        let metrics = self.metrics.read().await.clone();
        let exchange_analytics = self.exchange_analytics.read().await.clone();
        let order_type_metrics = self.order_type_metrics.read().await.clone();
        let events = self.event_stream.read().await.clone();

        // Calculate system performance
        let system_performance = SystemPerformance {
            total_orders: metrics.total_orders,
            success_rate: metrics.success_rate(),
            average_execution_time_ms: metrics.average_execution_time.as_millis() as u64,
            orders_per_second: self.calculate_throughput(&metrics).await,
            total_volume: Decimal::ZERO, // Would be calculated from historical data
            system_health: self.determine_system_health(&metrics),
        };

        // Calculate exchange breakdown
        let total_volume = Decimal::ONE; // Placeholder
        let exchange_breakdown: HashMap<ExchangeId, ExchangePerformance> = exchange_analytics
            .iter()
            .map(|(&exchange_id, analytics)| {
                let performance = ExchangePerformance {
                    exchange_id,
                    orders_executed: analytics.orders_executed,
                    success_rate: if analytics.orders_executed > 0 {
                        analytics.successful_executions as f64 / analytics.orders_executed as f64
                    } else {
                        0.0
                    },
                    average_latency_ms: analytics.average_latency_ms,
                    volume_share: (analytics.total_volume / total_volume)
                        .to_f64()
                        .unwrap_or(0.0),
                    health_status: if analytics.average_latency_ms < 200.0 {
                        HealthStatus::Healthy
                    } else if analytics.average_latency_ms < 500.0 {
                        HealthStatus::Warning
                    } else {
                        HealthStatus::Critical
                    },
                };
                (exchange_id, performance)
            })
            .collect();

        // Calculate order type breakdown
        let order_type_breakdown: HashMap<String, OrderTypePerformance> = order_type_metrics
            .iter()
            .map(|(order_type, metrics)| {
                let performance = OrderTypePerformance {
                    order_type: format!("{:?}", order_type),
                    count: metrics.total_orders,
                    success_rate: if metrics.total_orders > 0 {
                        metrics.successful_executions as f64 / metrics.total_orders as f64
                    } else {
                        0.0
                    },
                    average_execution_time_ms: metrics.average_execution_time.as_millis() as u64,
                    performance_score: metrics.average_confidence_score.unwrap_or(0.8),
                };
                (format!("{:?}", order_type), performance)
            })
            .collect();

        // Get recent alerts
        let active_alerts: Vec<AnalyticsEvent> = events
            .iter()
            .filter(|event| matches!(event, AnalyticsEvent::PerformanceAlert { .. }))
            .rev()
            .take(10)
            .cloned()
            .collect();

        // Calculate performance trends (simplified)
        let performance_trends = PerformanceTrends {
            execution_time_trend: TrendDirection::Stable,
            success_rate_trend: TrendDirection::Stable,
            volume_trend: TrendDirection::Stable,
            trend_period_hours: 1,
        };

        // Real-time metrics
        let recent_execution_times: Vec<u64> = events
            .iter()
            .filter_map(|event| {
                if let AnalyticsEvent::OrderExecuted {
                    execution_time_ms, ..
                } = event
                {
                    Some(*execution_time_ms)
                } else {
                    None
                }
            })
            .rev()
            .take(20)
            .collect();

        let real_time_metrics = RealTimeMetrics {
            pending_orders: PendingOrdersStats::default(), // Would come from executor
            recent_execution_times,
            current_throughput: self.calculate_throughput(&metrics).await,
            active_connections: exchange_analytics.len(),
        };

        AnalyticsDashboard {
            timestamp,
            system_performance,
            exchange_breakdown,
            order_type_breakdown,
            performance_trends,
            active_alerts,
            real_time_metrics,
        }
    }

    /// Calculate current throughput (orders per second)
    async fn calculate_throughput(&self, metrics: &OrderExecutionMetrics) -> f64 {
        // Simplified calculation based on recent activity
        // In practice, would use time-windowed calculations
        if metrics.average_execution_time.as_secs_f64() > 0.0 {
            1.0 / metrics.average_execution_time.as_secs_f64()
        } else {
            0.0
        }
    }

    /// Determine overall system health status
    fn determine_system_health(&self, metrics: &OrderExecutionMetrics) -> HealthStatus {
        let success_rate = metrics.success_rate();
        let avg_execution_ms = metrics.average_execution_time.as_millis() as u64;

        if success_rate >= 0.95 && avg_execution_ms <= 200 {
            HealthStatus::Healthy
        } else if success_rate >= 0.90 && avg_execution_ms <= 400 {
            HealthStatus::Warning
        } else {
            HealthStatus::Critical
        }
    }

    /// Get current system metrics
    pub async fn get_current_metrics(&self) -> OrderExecutionMetrics {
        self.metrics.read().await.clone()
    }

    /// Get historical performance data
    pub async fn get_historical_data(&self) -> HistoricalData {
        self.historical_data.read().await.clone()
    }

    /// Get recent events from the stream
    pub async fn get_recent_events(&self, count: usize) -> Vec<AnalyticsEvent> {
        let events = self.event_stream.read().await;
        events.iter().rev().take(count).cloned().collect()
    }

    /// Clear old historical data based on retention policy
    pub async fn cleanup_historical_data(&self) {
        let cutoff = Utc::now() - self.config.retention_period;
        let mut historical = self.historical_data.write().await;

        // Clean execution times
        while let Some(&(timestamp, _)) = historical.execution_times.front() {
            if timestamp < cutoff {
                historical.execution_times.pop_front();
            } else {
                break;
            }
        }

        // Clean success rates
        while let Some(&(timestamp, _)) = historical.success_rates.front() {
            if timestamp < cutoff {
                historical.success_rates.pop_front();
            } else {
                break;
            }
        }

        // Clean volumes
        while let Some(&(timestamp, _)) = historical.volumes.front() {
            if timestamp < cutoff {
                historical.volumes.pop_front();
            } else {
                break;
            }
        }

        // Clean snapshots
        while let Some(snapshot) = historical.snapshots.front() {
            if snapshot.timestamp < cutoff {
                historical.snapshots.pop_front();
            } else {
                break;
            }
        }

        debug!("Cleaned historical data older than {}", cutoff);
    }
}

// Helper trait to parse OrderKind from string (simplified)
impl std::str::FromStr for OrderKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "market" => Ok(OrderKind::Market),
            "limit" => Ok(OrderKind::Limit),
            "stop" => Ok(OrderKind::Stop),
            "stoplimit" => Ok(OrderKind::StopLimit),
            "jackpot" => Ok(OrderKind::Jackpot),
            "prophetic" => Ok(OrderKind::Prophetic),
            "eventtriggered" => Ok(OrderKind::EventTriggered),
            _ => Err(format!("Unknown order kind: {}", s)),
        }
    }
}
