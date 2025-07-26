use chrono::{DateTime, Utc};
use jackbot_instrument::exchange::ExchangeId;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;

/// Core performance metrics for the high-frequency trading system
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CorePerformanceMetrics {
    pub system_metrics: SystemPerformanceMetrics,
    pub trading_metrics: TradingPerformanceMetrics,
    pub execution_metrics: ExecutionPerformanceMetrics,
    pub market_data_metrics: MarketDataPerformanceMetrics,
    pub risk_metrics: RiskPerformanceMetrics,
    pub network_metrics: NetworkPerformanceMetrics,
    pub resource_metrics: ResourceUtilizationMetrics,
    pub database_metrics: DatabaseConnectionMetrics,
    pub qos_metrics: QualityOfServiceMetrics,
}

/// System-level performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SystemPerformanceMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_io: f64,
    pub network_io: f64,
    pub thread_count: u32,
    pub gc_metrics: GarbageCollectionMetrics,
    pub thread_pool_metrics: ThreadPoolMetrics,
    pub uptime: Duration,
    pub last_updated: DateTime<Utc>,
    pub process_id: u32,
    pub hostname: String,
    pub system_load: f64,
}

/// Garbage collection performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GarbageCollectionMetrics {
    pub total_collections: u64,
    pub total_gc_time: Duration,
    pub average_gc_time: Duration,
    pub max_gc_pause: Duration,
    pub gc_frequency: f64,
    pub memory_freed: u64,
}

/// Thread pool performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ThreadPoolMetrics {
    pub active_threads: u32,
    pub idle_threads: u32,
    pub max_threads: u32,
    pub queued_tasks: u32,
    pub completed_tasks: u64,
    pub rejected_tasks: u64,
    pub average_task_duration: Duration,
    pub thread_utilization: f64,
}

/// Trading-specific performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TradingPerformanceMetrics {
    pub orders_per_second: f64,
    pub fills_per_second: f64,
    pub position_updates_per_second: f64,
    pub pnl_calculation_time: Duration,
    pub portfolio_update_time: Duration,
}

/// Order execution performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExecutionPerformanceMetrics {
    pub order_latency: LatencyMetrics,
    pub fill_latency: LatencyMetrics,
    pub cancel_latency: LatencyMetrics,
    pub modification_latency: LatencyMetrics,
    pub slippage_metrics: SlippageMetrics,
    pub execution_rate: f64,
    pub success_rate: f64,
    pub error_rate: f64,
    pub timeout_rate: f64,
    pub strategy_metrics: HashMap<String, StrategyPerformanceMetrics>,
}

/// Slippage analysis metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SlippageMetrics {
    pub average_slippage: f64,
    pub max_slippage: f64,
    pub min_slippage: f64,
    pub slippage_variance: f64,
    pub positive_slippage_rate: f64,
    pub negative_slippage_rate: f64,
    pub slippage_distribution: Vec<f64>,
}

/// Strategy-specific performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct StrategyPerformanceMetrics {
    pub strategy_id: String,
    pub total_orders: u64,
    pub successful_orders: u64,
    pub average_execution_time: Duration,
    pub pnl: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
}

/// Market data performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MarketDataPerformanceMetrics {
    pub feed_latency: LatencyMetrics,
    pub processing_latency: LatencyMetrics,
    pub messages_per_second: f64,
    pub missed_messages: u64,
    pub out_of_order_messages: u64,
    pub duplicate_messages: u64,
    pub data_quality_score: f64,
    pub exchange_metrics: HashMap<ExchangeId, ExchangePerformanceMetrics>,
}

/// Exchange-specific performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExchangePerformanceMetrics {
    pub exchange_id: ExchangeId,
    pub connection_status: String,
    pub last_heartbeat: DateTime<Utc>,
    pub message_rate: f64,
    pub error_rate: f64,
    pub reconnection_count: u32,
    pub data_latency: LatencyMetrics,
}

/// Risk management performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RiskPerformanceMetrics {
    pub risk_check_latency: LatencyMetrics,
    pub position_limit_checks: u64,
    pub exposure_calculations: u64,
    pub var_calculations: u64,
    pub stress_test_calculations: u64,
    pub risk_alerts: u64,
    pub compliance_checks: u64,
}

/// Network performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NetworkPerformanceMetrics {
    pub bandwidth_utilization: f64,
    pub packet_loss: f64,
    pub jitter: Duration,
    pub round_trip_time: LatencyMetrics,
    pub connection_pool_size: u32,
    pub active_connections: u32,
    pub failed_connections: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

/// Latency measurement metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LatencyMetrics {
    pub average: Duration,
    pub median: Duration,
    pub percentile_95: Duration,
    pub percentile_99: Duration,
    pub percentile_99_9: Duration,
    pub min: Duration,
    pub max: Duration,
    pub count: u64,
    pub standard_deviation: Duration,
}

/// Resource utilization metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ResourceUtilizationMetrics {
    pub cpu_cores: Vec<f64>,
    pub memory_breakdown: MemoryBreakdown,
    pub disk_usage: DiskUsageMetrics,
    pub file_descriptors_used: u32,
    pub file_descriptors_limit: u32,
    pub swap_usage: f64,
    pub cache_hit_rate: f64,
}

/// Memory breakdown metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MemoryBreakdown {
    pub heap_used: u64,
    pub heap_total: u64,
    pub stack_used: u64,
    pub cache_used: u64,
    pub buffer_used: u64,
    pub shared_memory: u64,
}

/// Disk usage metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DiskUsageMetrics {
    pub total_space: u64,
    pub used_space: u64,
    pub available_space: u64,
    pub io_operations_per_second: f64,
    pub read_throughput: f64,
    pub write_throughput: f64,
}

/// Database connection performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DatabaseConnectionMetrics {
    pub active_connections: u32,
    pub idle_connections: u32,
    pub max_connections: u32,
    pub query_latency: LatencyMetrics,
    pub transaction_latency: LatencyMetrics,
    pub connection_pool_wait_time: LatencyMetrics,
    pub failed_queries: u64,
    pub slow_queries: u64,
}

/// Quality of service metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct QualityOfServiceMetrics {
    pub availability: f64,
    pub reliability: f64,
    pub performance_score: f64,
    pub error_budget_remaining: f64,
    pub sla_compliance: f64,
    pub service_level_indicators: HashMap<String, f64>,
    pub incident_count: u32,
    pub mean_time_to_recovery: Duration,
    pub uptime_percentage: f64,
}

impl CorePerformanceMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_system_metrics(&mut self, cpu: f64, memory: f64, disk_io: f64, network_io: f64) {
        self.system_metrics.cpu_usage = cpu;
        self.system_metrics.memory_usage = memory;
        self.system_metrics.disk_io = disk_io;
        self.system_metrics.network_io = network_io;
        self.system_metrics.last_updated = Utc::now();
    }

    pub fn get_overall_health_score(&self) -> f64 {
        let cpu_score = (100.0 - self.system_metrics.cpu_usage) / 100.0;
        let memory_score = (100.0 - self.system_metrics.memory_usage) / 100.0;
        let execution_score = self.execution_metrics.success_rate;
        let market_data_score = self.market_data_metrics.data_quality_score;
        
        (cpu_score + memory_score + execution_score + market_data_score) / 4.0
    }
}

impl LatencyMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, latency: Duration) {
        self.count += 1;
        
        if self.count == 1 {
            self.min = latency;
            self.max = latency;
            self.average = latency;
            self.median = latency;
        } else {
            self.min = self.min.min(latency);
            self.max = self.max.max(latency);
            
            // Simple running average - would need proper percentile calculation in production
            let total_nanos = self.average.as_nanos() * (self.count - 1) as u128 + latency.as_nanos();
            self.average = Duration::from_nanos((total_nanos / self.count as u128) as u64);
        }
    }
}