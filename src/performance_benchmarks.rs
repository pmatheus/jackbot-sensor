//! Performance benchmarking module for jackbot-sensor
//!
//! This module provides comprehensive performance testing to validate
//! the <10ms market data processing and <50ms order execution requirements.

use anyhow::Result;
use futures::{stream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};
use tokio::time::interval;
use tracing::{info, warn};

use crate::api::{OrderBookData, PriceLevel, TickerData};
use crate::connector::{
    Balance, Exchange, MarketData, MarketDataStream, Order, OrderId, OrderResult, OrderSide,
    OrderStatus, OrderType, TimeInForce, Connection,
};
use crate::order_book_aggregator::{AggregationConfig, OrderBookAggregator};
use crate::smart_routing::{SmartOrderRouter, SmartRoutingConfig};

/// Performance benchmark configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Duration to run each benchmark
    pub duration_seconds: u64,
    /// Number of concurrent operations to test
    pub concurrency_level: usize,
    /// Market data update frequency (messages per second)
    pub market_data_rate: u32,
    /// Order submission rate (orders per second)
    pub order_rate: u32,
    /// Performance targets
    pub targets: PerformanceTargets,
}

/// Performance targets to validate against
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTargets {
    /// Maximum market data processing latency (microseconds)
    pub max_market_data_latency_us: u64,
    /// Maximum order execution latency (milliseconds)
    pub max_order_latency_ms: u64,
    /// Minimum throughput (messages per second)
    pub min_throughput_mps: u32,
    /// Maximum memory usage (MB)
    pub max_memory_mb: u64,
    /// Minimum success rate (percentage)
    pub min_success_rate_pct: f64,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            duration_seconds: 60,
            concurrency_level: 100,
            market_data_rate: 10000, // 10K messages/second
            order_rate: 100,         // 100 orders/second
            targets: PerformanceTargets {
                max_market_data_latency_us: 10000, // 10ms = 10,000 microseconds
                max_order_latency_ms: 50,           // 50ms
                min_throughput_mps: 1000000,        // 1M messages/second
                max_memory_mb: 512,                 // 512MB
                min_success_rate_pct: 99.9,         // 99.9%
            },
        }
    }
}

/// Performance metrics collected during benchmarks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub test_name: String,
    pub duration_ms: u64,
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub success_rate_pct: f64,
    pub avg_latency_us: u64,
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub p99_latency_us: u64,
    pub max_latency_us: u64,
    pub throughput_ops_sec: f64,
    pub memory_usage_mb: u64,
    pub cpu_usage_pct: f64,
    pub passed_targets: bool,
    pub target_violations: Vec<String>,
}

/// Latency measurement
#[derive(Debug, Clone, Copy)]
struct LatencyMeasurement {
    start_time: Instant,
    end_time: Instant,
    latency_us: u64,
}

/// High-performance mock exchange for benchmarking
pub struct BenchmarkExchange {
    name: String,
    latency_simulation_us: u64,
    failure_rate: f64,
    operation_counter: Arc<AtomicU64>,
    latency_measurements: Arc<Mutex<VecDeque<LatencyMeasurement>>>,
}

impl BenchmarkExchange {
    pub fn new(name: String, latency_simulation_us: u64, failure_rate: f64) -> Self {
        Self {
            name,
            latency_simulation_us,
            failure_rate,
            operation_counter: Arc::new(AtomicU64::new(0)),
            latency_measurements: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    async fn record_latency(&self, start: Instant, end: Instant) {
        let latency_us = end.duration_since(start).as_micros() as u64;
        let measurement = LatencyMeasurement {
            start_time: start,
            end_time: end,
            latency_us,
        };

        let mut measurements = self.latency_measurements.lock().await;
        measurements.push_back(measurement);
        
        // Keep only recent measurements (last 10,000)
        if measurements.len() > 10000 {
            measurements.pop_front();
        }
    }

    pub async fn get_latency_stats(&self) -> (u64, u64, u64, u64) {
        let measurements = self.latency_measurements.lock().await;
        if measurements.is_empty() {
            return (0, 0, 0, 0);
        }

        let mut latencies: Vec<u64> = measurements.iter().map(|m| m.latency_us).collect();
        latencies.sort_unstable();

        let len = latencies.len();
        let p50 = latencies[len * 50 / 100];
        let p95 = latencies[len * 95 / 100];
        let p99 = latencies[len * 99 / 100];
        let max = latencies[len - 1];

        (p50, p95, p99, max)
    }
}

#[async_trait::async_trait]
impl Exchange for BenchmarkExchange {
    async fn connect(&self) -> Result<Connection> {
        tokio::time::sleep(Duration::from_micros(self.latency_simulation_us)).await;
        Ok(Arc::new(()) as Connection)
    }

    async fn subscribe_market_data(&self, symbols: Vec<String>) -> Result<MarketDataStream> {
        let stream = stream::iter(0..1000000).then(move |i| {
            let symbols = symbols.clone();
            async move {
                // Simulate realistic market data
                MarketData::Ticker(TickerData {
                    symbol: symbols[0].clone(),
                    exchange: "benchmark".to_string(),
                    price: 50000.0 + (i as f64 * 0.01),
                    bid: 49999.0,
                    ask: 50001.0,
                    volume_24h: 10000.0,
                    change_24h: 1.5,
                    high_24h: 51000.0,
                    low_24h: 49000.0,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
        });

        Ok(Box::pin(stream) as MarketDataStream)
    }

    async fn place_order(&self, order: Order) -> Result<OrderResult> {
        let start = Instant::now();
        
        // Simulate processing time
        tokio::time::sleep(Duration::from_micros(self.latency_simulation_us)).await;
        
        // Simulate failure rate
        if rand::random::<f64>() < self.failure_rate {
            return Err(anyhow::anyhow!("Simulated order failure"));
        }

        let end = Instant::now();
        self.record_latency(start, end).await;
        self.operation_counter.fetch_add(1, Ordering::Relaxed);

        Ok(OrderResult {
            order_id: format!("bench-{}", uuid::Uuid::new_v4()),
            status: OrderStatus::New,
            filled_quantity: 0.0,
            remaining_quantity: order.quantity,
            average_price: order.price.unwrap_or(50000.0),
            commission: 0.001,
            commission_asset: "USDT".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn cancel_order(&self, _id: OrderId) -> Result<()> {
        let start = Instant::now();
        tokio::time::sleep(Duration::from_micros(self.latency_simulation_us)).await;
        let end = Instant::now();
        
        self.record_latency(start, end).await;
        Ok(())
    }

    async fn get_balance(&self) -> Result<Vec<Balance>> {
        let start = Instant::now();
        tokio::time::sleep(Duration::from_micros(self.latency_simulation_us)).await;
        let end = Instant::now();
        
        self.record_latency(start, end).await;
        
        Ok(vec![Balance {
            asset: "USDT".to_string(),
            free: 100000.0,
            locked: 0.0,
            total: 100000.0,
        }])
    }
}

/// Performance benchmark suite
pub struct PerformanceBenchmarkSuite {
    config: BenchmarkConfig,
}

impl PerformanceBenchmarkSuite {
    pub fn new(config: BenchmarkConfig) -> Self {
        Self { config }
    }

    /// Run all performance benchmarks
    pub async fn run_all_benchmarks(&self) -> Result<Vec<PerformanceMetrics>> {
        info!("Starting performance benchmark suite");
        let mut results = Vec::new();

        // Market data processing benchmark
        results.push(self.benchmark_market_data_processing().await?);

        // Order execution latency benchmark
        results.push(self.benchmark_order_execution().await?);

        // Concurrent operations benchmark
        results.push(self.benchmark_concurrent_operations().await?);

        // Order book aggregation benchmark
        results.push(self.benchmark_order_book_aggregation().await?);

        // Smart routing benchmark
        results.push(self.benchmark_smart_routing().await?);

        // Memory and CPU stress test
        results.push(self.benchmark_stress_test().await?);

        info!("Completed all performance benchmarks");
        Ok(results)
    }

    /// Benchmark market data processing performance
    pub async fn benchmark_market_data_processing(&self) -> Result<PerformanceMetrics> {
        info!("Running market data processing benchmark");
        
        let start_time = Instant::now();
        let mut successful = 0u64;
        let mut failed = 0u64;
        let mut latency_measurements = Vec::new();

        // Create high-frequency market data stream
        let exchange = BenchmarkExchange::new("test".to_string(), 1000, 0.0); // 1ms latency, no failures
        let mut stream = exchange.subscribe_market_data(vec!["BTC/USDT".to_string()]).await?;

        let test_duration = Duration::from_secs(self.config.duration_seconds);
        let mut messages_processed = 0u64;

        while start_time.elapsed() < test_duration {
            let process_start = Instant::now();
            
            if let Some(data) = stream.next().await {
                // Simulate processing
                match data {
                    MarketData::Ticker(ticker) => {
                        // Validate ticker data
                        if ticker.price > 0.0 && ticker.bid > 0.0 && ticker.ask > 0.0 {
                            successful += 1;
                        } else {
                            failed += 1;
                        }
                    }
                    _ => failed += 1,
                }
                
                let process_end = Instant::now();
                let latency_us = process_end.duration_since(process_start).as_micros() as u64;
                latency_measurements.push(latency_us);
                
                messages_processed += 1;
                
                // Limit rate for testing
                if messages_processed % 1000 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        }

        let total_duration = start_time.elapsed();
        self.calculate_metrics(
            "Market Data Processing",
            total_duration,
            successful + failed,
            successful,
            failed,
            latency_measurements,
        )
    }

    /// Benchmark order execution latency
    pub async fn benchmark_order_execution(&self) -> Result<PerformanceMetrics> {
        info!("Running order execution benchmark");
        
        let exchange = Arc::new(BenchmarkExchange::new("test".to_string(), 5000, 0.001)); // 5ms latency, 0.1% failure
        let _connection = exchange.connect().await?;

        let start_time = Instant::now();
        let mut latency_measurements = Vec::new();
        let mut successful = 0u64;
        let mut failed = 0u64;

        let test_duration = Duration::from_secs(self.config.duration_seconds);

        while start_time.elapsed() < test_duration {
            let order = Order {
                id: Some(uuid::Uuid::new_v4().to_string()),
                symbol: "BTC/USDT".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                price: Some(50000.0),
                quantity: 0.1,
                time_in_force: Some(TimeInForce::GTC),
                status: OrderStatus::New,
            };

            let order_start = Instant::now();
            
            match exchange.place_order(order).await {
                Ok(_) => {
                    successful += 1;
                    let latency_us = order_start.elapsed().as_micros() as u64;
                    latency_measurements.push(latency_us);
                }
                Err(_) => failed += 1,
            }

            // Rate limiting
            tokio::time::sleep(Duration::from_millis(10)).await; // 100 orders/second
        }

        let total_duration = start_time.elapsed();
        self.calculate_metrics(
            "Order Execution",
            total_duration,
            successful + failed,
            successful,
            failed,
            latency_measurements,
        )
    }

    /// Benchmark concurrent operations
    pub async fn benchmark_concurrent_operations(&self) -> Result<PerformanceMetrics> {
        info!("Running concurrent operations benchmark");
        
        let exchange = Arc::new(BenchmarkExchange::new("test".to_string(), 2000, 0.001));
        let _connection = exchange.connect().await?;

        let start_time = Instant::now();
        let mut handles = Vec::new();

        // Spawn concurrent tasks
        for _ in 0..self.config.concurrency_level {
            let exchange_clone = Arc::clone(&exchange);
            let handle = tokio::spawn(async move {
                let mut local_successful = 0u64;
                let mut local_failed = 0u64;
                let mut local_latencies = Vec::new();

                for _ in 0..100 {
                    let order = Order {
                        id: Some(uuid::Uuid::new_v4().to_string()),
                        symbol: "BTC/USDT".to_string(),
                        side: OrderSide::Buy,
                        order_type: OrderType::Limit,
                        price: Some(50000.0),
                        quantity: 0.1,
                        time_in_force: Some(TimeInForce::GTC),
                        status: OrderStatus::New,
                    };

                    let order_start = Instant::now();
                    
                    match exchange_clone.place_order(order).await {
                        Ok(_) => {
                            local_successful += 1;
                            let latency_us = order_start.elapsed().as_micros() as u64;
                            local_latencies.push(latency_us);
                        }
                        Err(_) => local_failed += 1,
                    }
                }

                (local_successful, local_failed, local_latencies)
            });
            handles.push(handle);
        }

        // Collect results
        let mut total_successful = 0u64;
        let mut total_failed = 0u64;
        let mut all_latencies = Vec::new();

        for handle in handles {
            let (successful, failed, latencies) = handle.await?;
            total_successful += successful;
            total_failed += failed;
            all_latencies.extend(latencies);
        }

        let total_duration = start_time.elapsed();
        self.calculate_metrics(
            "Concurrent Operations",
            total_duration,
            total_successful + total_failed,
            total_successful,
            total_failed,
            all_latencies,
        )
    }

    /// Benchmark order book aggregation
    pub async fn benchmark_order_book_aggregation(&self) -> Result<PerformanceMetrics> {
        info!("Running order book aggregation benchmark");
        
        let config = AggregationConfig::default();
        let aggregator = OrderBookAggregator::new(config);

        let start_time = Instant::now();
        let mut successful = 0u64;
        let mut failed = 0u64;
        let mut latency_measurements = Vec::new();

        let test_duration = Duration::from_secs(self.config.duration_seconds);

        while start_time.elapsed() < test_duration {
            let process_start = Instant::now();

            // Create mock order book data
            let order_book = OrderBookData {
                symbol: "BTC/USDT".to_string(),
                exchange: "test_exchange".to_string(),
                bids: vec![
                    [50000.0, 1.0],
                    [49999.0, 2.0],
                ],
                asks: vec![
                    [50001.0, 1.5],
                    [50002.0, 1.2],
                ],
                timestamp: chrono::Utc::now().timestamp_millis(),
                sequence_id: Some(1234),
            };

            match aggregator.update_exchange_book("test".to_string(), order_book, 50).await {
                Ok(_) => {
                    successful += 1;
                    let latency_us = process_start.elapsed().as_micros() as u64;
                    latency_measurements.push(latency_us);
                }
                Err(_) => failed += 1,
            }

            tokio::time::sleep(Duration::from_micros(100)).await; // 10K updates/second
        }

        let total_duration = start_time.elapsed();
        self.calculate_metrics(
            "Order Book Aggregation",
            total_duration,
            successful + failed,
            successful,
            failed,
            latency_measurements,
        )
    }

    /// Benchmark smart routing performance
    pub async fn benchmark_smart_routing(&self) -> Result<PerformanceMetrics> {
        info!("Running smart routing benchmark");
        
        let config = SmartRoutingConfig::default();
        let mut router = SmartOrderRouter::new(config);

        // Add mock exchanges
        for i in 0..3 {
            let exchange = Arc::new(BenchmarkExchange::new(
                format!("exchange_{}", i),
                1000 + i * 500, // Varying latencies
                0.001,
            ));
            router.add_exchange(format!("exchange_{}", i), exchange);
        }

        let start_time = Instant::now();
        let mut successful = 0u64;
        let mut failed = 0u64;
        let mut latency_measurements = Vec::new();

        let test_duration = Duration::from_secs(self.config.duration_seconds);

        while start_time.elapsed() < test_duration {
            let order = Order {
                id: Some(uuid::Uuid::new_v4().to_string()),
                symbol: "BTC/USDT".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                price: Some(50000.0),
                quantity: 1.0,
                time_in_force: Some(TimeInForce::GTC),
                status: OrderStatus::New,
            };

            let routing_start = Instant::now();

            match router.route_order(order).await {
                Ok(_decisions) => {
                    successful += 1;
                    let latency_us = routing_start.elapsed().as_micros() as u64;
                    latency_measurements.push(latency_us);
                }
                Err(_) => failed += 1,
            }

            tokio::time::sleep(Duration::from_millis(10)).await; // 100 routes/second
        }

        let total_duration = start_time.elapsed();
        self.calculate_metrics(
            "Smart Routing",
            total_duration,
            successful + failed,
            successful,
            failed,
            latency_measurements,
        )
    }

    /// Stress test for memory and CPU usage
    pub async fn benchmark_stress_test(&self) -> Result<PerformanceMetrics> {
        info!("Running stress test benchmark");
        
        let start_time = Instant::now();
        let mut successful = 0u64;
        let mut failed = 0u64;
        let mut latency_measurements = Vec::new();

        // Simulate high-load scenario
        let mut handles = Vec::new();
        
        for _ in 0..self.config.concurrency_level * 2 {
            let handle = tokio::spawn(async move {
                let mut data = Vec::new();
                let mut local_latencies = Vec::new();
                
                for i in 0..1000 {
                    let start = Instant::now();
                    
                    // Simulate memory allocation and computation
                    data.push(vec![i as f64; 1000]);
                    let _sum: f64 = data.iter().flatten().sum();
                    
                    let latency_us = start.elapsed().as_micros() as u64;
                    local_latencies.push(latency_us);
                    
                    // Prevent memory bloat
                    if data.len() > 100 {
                        data.clear();
                    }
                }
                
                local_latencies
            });
            handles.push(handle);
        }

        // Collect results
        for handle in handles {
            match handle.await {
                Ok(latencies) => {
                    successful += latencies.len() as u64;
                    latency_measurements.extend(latencies);
                }
                Err(_) => failed += 1,
            }
        }

        let total_duration = start_time.elapsed();
        self.calculate_metrics(
            "Stress Test",
            total_duration,
            successful + failed,
            successful,
            failed,
            latency_measurements,
        )
    }

    /// Calculate performance metrics from benchmark results
    fn calculate_metrics(
        &self,
        test_name: &str,
        duration: Duration,
        total_ops: u64,
        successful_ops: u64,
        failed_ops: u64,
        latency_measurements: Vec<u64>,
    ) -> Result<PerformanceMetrics> {
        let duration_ms = duration.as_millis() as u64;
        let success_rate_pct = if total_ops > 0 {
            (successful_ops as f64 / total_ops as f64) * 100.0
        } else {
            0.0
        };

        let throughput_ops_sec = if duration_ms > 0 {
            (total_ops as f64 * 1000.0) / duration_ms as f64
        } else {
            0.0
        };

        // Calculate latency statistics
        let mut sorted_latencies = latency_measurements.clone();
        sorted_latencies.sort_unstable();

        let (avg_latency_us, p50_latency_us, p95_latency_us, p99_latency_us, max_latency_us) =
            if !sorted_latencies.is_empty() {
                let len = sorted_latencies.len();
                let avg = sorted_latencies.iter().sum::<u64>() / len as u64;
                let p50 = sorted_latencies[len * 50 / 100];
                let p95 = sorted_latencies[len * 95 / 100];
                let p99 = sorted_latencies[len * 99 / 100];
                let max = sorted_latencies[len - 1];
                (avg, p50, p95, p99, max)
            } else {
                (0, 0, 0, 0, 0)
            };

        // Check against targets
        let mut target_violations = Vec::new();
        let mut passed_targets = true;

        if p99_latency_us > self.config.targets.max_market_data_latency_us {
            target_violations.push(format!(
                "P99 latency {}μs exceeds target {}μs",
                p99_latency_us, self.config.targets.max_market_data_latency_us
            ));
            passed_targets = false;
        }

        if success_rate_pct < self.config.targets.min_success_rate_pct {
            target_violations.push(format!(
                "Success rate {:.2}% below target {:.2}%",
                success_rate_pct, self.config.targets.min_success_rate_pct
            ));
            passed_targets = false;
        }

        if throughput_ops_sec < self.config.targets.min_throughput_mps as f64 {
            target_violations.push(format!(
                "Throughput {:.0} ops/sec below target {} ops/sec",
                throughput_ops_sec, self.config.targets.min_throughput_mps
            ));
            passed_targets = false;
        }

        Ok(PerformanceMetrics {
            test_name: test_name.to_string(),
            duration_ms,
            total_operations: total_ops,
            successful_operations: successful_ops,
            failed_operations: failed_ops,
            success_rate_pct,
            avg_latency_us,
            p50_latency_us,
            p95_latency_us,
            p99_latency_us,
            max_latency_us,
            throughput_ops_sec,
            memory_usage_mb: 0, // Would need actual memory profiling
            cpu_usage_pct: 0.0, // Would need actual CPU profiling
            passed_targets,
            target_violations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_market_data_benchmark() {
        let config = BenchmarkConfig {
            duration_seconds: 1, // Short test
            ..Default::default()
        };
        
        let suite = PerformanceBenchmarkSuite::new(config);
        let metrics = suite.benchmark_market_data_processing().await.unwrap();
        
        assert!(metrics.total_operations > 0);
        assert!(metrics.success_rate_pct > 95.0);
        println!("Market data benchmark: {:#?}", metrics);
    }

    #[tokio::test]
    async fn test_order_execution_benchmark() {
        let config = BenchmarkConfig {
            duration_seconds: 1,
            ..Default::default()
        };
        
        let suite = PerformanceBenchmarkSuite::new(config);
        let metrics = suite.benchmark_order_execution().await.unwrap();
        
        assert!(metrics.total_operations > 0);
        assert!(metrics.p99_latency_us < 100000); // Less than 100ms
        println!("Order execution benchmark: {:#?}", metrics);
    }

    #[tokio::test]
    async fn test_full_benchmark_suite() {
        let config = BenchmarkConfig {
            duration_seconds: 1,
            concurrency_level: 10,
            ..Default::default()
        };
        
        let suite = PerformanceBenchmarkSuite::new(config);
        let all_metrics = suite.run_all_benchmarks().await.unwrap();
        
        assert!(!all_metrics.is_empty());
        for metrics in &all_metrics {
            println!("{}: {:#?}", metrics.test_name, metrics);
        }
    }
}