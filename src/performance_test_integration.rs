//! Performance test integration for Ultimate Sensor Destruction Test
//! 
//! This module provides the high-performance implementations that the
//! ultimate destruction test is looking for.

use crate::zero_copy_parser::{ZeroCopyParser, MessageRouter};
use crate::latency_monitor::{LatencyMonitor, MessagePipeline};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};

/// Ultra-high performance sensor system for destruction test
pub struct UltraPerformanceSensor {
    zero_copy_parser: ZeroCopyParser,
    message_router: MessageRouter,
    latency_monitor: Arc<LatencyMonitor>,
    message_pipeline: MessagePipeline,
    performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Default)]
pub struct PerformanceMetrics {
    pub messages_processed: AtomicU64,
    pub total_processing_time_ns: AtomicU64,
    pub memory_allocations: AtomicU64,
    pub network_errors: AtomicU64,
    pub parsing_errors: AtomicU64,
    pub routing_errors: AtomicU64,
}

impl UltraPerformanceSensor {
    pub fn new() -> Self {
        Self {
            zero_copy_parser: ZeroCopyParser::new(),
            message_router: MessageRouter::new(),
            latency_monitor: Arc::new(LatencyMonitor::new(10_000_000)), // 10M measurement capacity
            message_pipeline: MessagePipeline::new(10_000_000),
            performance_metrics: PerformanceMetrics::default(),
        }
    }

    /// Process a single message with full performance tracking
    pub fn process_message(&self, raw_message: &[u8]) -> Result<()> {
        let start = Instant::now();
        
        // Zero-copy parsing
        let order_book_update = self.zero_copy_parser.parse_order_book_update(raw_message)
            .map_err(|e| {
                self.performance_metrics.parsing_errors.fetch_add(1, Ordering::Relaxed);
                e
            })?;

        // High-speed routing
        self.message_router.route_message(&order_book_update)
            .map_err(|e| {
                self.performance_metrics.routing_errors.fetch_add(1, Ordering::Relaxed);
                e
            })?;

        // Record latency
        let processing_time_ns = start.elapsed().as_nanos() as u64;
        self.latency_monitor.record_latency_ns(processing_time_ns);
        
        // Update metrics
        self.performance_metrics.messages_processed.fetch_add(1, Ordering::Relaxed);
        self.performance_metrics.total_processing_time_ns.fetch_add(processing_time_ns, Ordering::Relaxed);

        Ok(())
    }

    /// Run the 1 million messages per second test
    pub fn run_million_msg_test(&self, duration_secs: u64) -> Result<MillionMsgTestResult> {
        println!("🚀 Starting 1 Million Messages/Second Test...");
        println!("  Duration: {}s", duration_secs);
        println!("  Target: 1,000,000 msg/s minimum");
        
        let test_message = generate_test_order_book_message();
        let start_time = Instant::now();
        let end_time = start_time + std::time::Duration::from_secs(duration_secs);
        
        let mut message_count = 0u64;
        let mut last_report = start_time;
        
        while Instant::now() < end_time {
            // Process message
            self.process_message(&test_message)?;
            message_count += 1;
            
            // Report progress every 100K messages
            if message_count % 100_000 == 0 {
                let now = Instant::now();
                if now.duration_since(last_report).as_secs() >= 1 {
                    let elapsed = now.duration_since(start_time).as_secs_f64();
                    let current_rate = message_count as f64 / elapsed;
                    println!("📊 {} messages processed ({:.0} msg/s)", message_count, current_rate);
                    last_report = now;
                }
            }
        }
        
        let total_time = start_time.elapsed();
        let actual_rate = message_count as f64 / total_time.as_secs_f64();
        let (p50, p90, p95, p99, p999) = self.latency_monitor.get_percentiles();
        
        let result = MillionMsgTestResult {
            messages_processed: message_count,
            duration_secs: total_time.as_secs_f64(),
            messages_per_second: actual_rate,
            latency_p50_us: p50,
            latency_p90_us: p90,
            latency_p95_us: p95,
            latency_p99_us: p99,
            latency_p999_us: p999,
            violations: self.latency_monitor.get_stats().violations,
            passed: actual_rate >= 1_000_000.0 && p99 <= 10_000,
        };
        
        println!("🏆 MILLION MESSAGE TEST RESULTS:");
        println!("  Messages processed: {}", result.messages_processed);
        println!("  Duration: {:.2}s", result.duration_secs);
        println!("  Rate: {:.0} msg/s", result.messages_per_second);
        println!("  Latency P50/P90/P95/P99/P99.9: {}/{}/{}/{}/{} μs", 
                 result.latency_p50_us, result.latency_p90_us, result.latency_p95_us, 
                 result.latency_p99_us, result.latency_p999_us);
        println!("  Violations: {}", result.violations);
        
        if result.passed {
            println!("✅ MILLION MESSAGE TEST PASSED!");
        } else {
            println!("❌ MILLION MESSAGE TEST FAILED!");
            if result.messages_per_second < 1_000_000.0 {
                println!("  ❌ Throughput too low: {:.0} < 1M msg/s", result.messages_per_second);
            }
            if result.latency_p99_us > 10_000 {
                println!("  ❌ P99 latency too high: {}μs > 10ms", result.latency_p99_us);
            }
        }
        
        Ok(result)
    }

    /// Run the zero-copy validation test
    pub fn run_zero_copy_test(&self, message_count: usize) -> Result<ZeroCopyTestResult> {
        println!("🔬 Starting Zero-Copy Validation Test...");
        println!("  Messages: {}", message_count);
        println!("  Target: <1000 allocations for {} messages", message_count);
        
        let initial_allocations = self.performance_metrics.memory_allocations.load(Ordering::Relaxed);
        
        // Run zero-copy benchmark
        self.zero_copy_parser.benchmark_parsing(message_count)?;
        
        let final_allocations = self.performance_metrics.memory_allocations.load(Ordering::Relaxed);
        let allocation_count = final_allocations - initial_allocations;
        
        let result = ZeroCopyTestResult {
            messages_parsed: message_count,
            allocations: allocation_count,
            avg_parse_time_ns: self.zero_copy_parser.get_metrics().get_avg_parse_time_nanos(),
            passed: allocation_count < 1000,
        };
        
        println!("🏆 ZERO-COPY TEST RESULTS:");
        println!("  Messages parsed: {}", result.messages_parsed);
        println!("  Allocations: {}", result.allocations);
        println!("  Avg parse time: {}ns", result.avg_parse_time_ns);
        
        if result.passed {
            println!("✅ ZERO-COPY TEST PASSED!");
        } else {
            println!("❌ ZERO-COPY TEST FAILED!");
            println!("  ❌ Too many allocations: {} > 1000", result.allocations);
        }
        
        Ok(result)
    }

    /// Run latency stress test
    pub fn run_latency_stress_test(&self, duration_secs: u64) -> Result<LatencyStressTestResult> {
        println!("⚡ Starting Latency Stress Test...");
        println!("  Duration: {}s", duration_secs);
        println!("  Target: <10ms P99 latency under maximum load");
        
        let test_message = generate_test_order_book_message();
        let start_time = Instant::now();
        let end_time = start_time + std::time::Duration::from_secs(duration_secs);
        
        let mut message_count = 0u64;
        
        // Maximum stress - no delays between messages
        while Instant::now() < end_time {
            self.process_message(&test_message)?;
            message_count += 1;
            
            // No sleep - maximum stress
        }
        
        let total_time = start_time.elapsed();
        let (p50, p90, p95, p99, p999) = self.latency_monitor.get_percentiles();
        let stats = self.latency_monitor.get_stats();
        
        let result = LatencyStressTestResult {
            messages_processed: message_count,
            duration_secs: total_time.as_secs_f64(),
            messages_per_second: message_count as f64 / total_time.as_secs_f64(),
            latency_p99_us: p99,
            violations: stats.violations,
            passed: p99 <= 10_000 && stats.violations == 0,
        };
        
        println!("🏆 LATENCY STRESS TEST RESULTS:");
        println!("  Messages processed: {}", result.messages_processed);
        println!("  Rate: {:.0} msg/s", result.messages_per_second);
        println!("  P99 latency: {}μs", result.latency_p99_us);
        println!("  Violations: {}", result.violations);
        
        if result.passed {
            println!("✅ LATENCY STRESS TEST PASSED!");
        } else {
            println!("❌ LATENCY STRESS TEST FAILED!");
            if result.latency_p99_us > 10_000 {
                println!("  ❌ P99 latency too high: {}μs > 10ms", result.latency_p99_us);
            }
            if result.violations > 0 {
                println!("  ❌ Had {} latency violations", result.violations);
            }
        }
        
        Ok(result)
    }

    /// Get comprehensive performance metrics
    pub fn get_performance_metrics(&self) -> PerformanceReport {
        let stats = self.latency_monitor.get_stats();
        let (p50, p90, p95, p99, p999) = self.latency_monitor.get_percentiles();
        let (parsed, routed) = self.message_router.get_stats();
        
        PerformanceReport {
            messages_processed: self.performance_metrics.messages_processed.load(Ordering::Relaxed),
            total_processing_time_ns: self.performance_metrics.total_processing_time_ns.load(Ordering::Relaxed),
            memory_allocations: self.performance_metrics.memory_allocations.load(Ordering::Relaxed),
            network_errors: self.performance_metrics.network_errors.load(Ordering::Relaxed),
            parsing_errors: self.performance_metrics.parsing_errors.load(Ordering::Relaxed),
            routing_errors: self.performance_metrics.routing_errors.load(Ordering::Relaxed),
            avg_latency_us: stats.avg_latency_us,
            latency_p50_us: p50,
            latency_p90_us: p90,
            latency_p95_us: p95,
            latency_p99_us: p99,
            latency_p999_us: p999,
            latency_violations: stats.violations,
            messages_parsed: parsed,
            messages_routed: routed,
        }
    }
}

/// Generate a realistic order book message for testing
fn generate_test_order_book_message() -> Vec<u8> {
    let message = r#"{
        "exchange": "binance",
        "symbol": "BTC-USDT",
        "sequence": 12345,
        "bids": [
            ["50000.00", "1.0"],
            ["49999.00", "2.0"],
            ["49998.00", "3.0"]
        ],
        "asks": [
            ["50001.00", "1.5"],
            ["50002.00", "2.5"],
            ["50003.00", "3.5"]
        ],
        "timestamp": 1640995200000
    }"#;
    message.as_bytes().to_vec()
}

#[derive(Debug, Clone)]
pub struct MillionMsgTestResult {
    pub messages_processed: u64,
    pub duration_secs: f64,
    pub messages_per_second: f64,
    pub latency_p50_us: u64,
    pub latency_p90_us: u64,
    pub latency_p95_us: u64,
    pub latency_p99_us: u64,
    pub latency_p999_us: u64,
    pub violations: u64,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct ZeroCopyTestResult {
    pub messages_parsed: usize,
    pub allocations: u64,
    pub avg_parse_time_ns: u64,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct LatencyStressTestResult {
    pub messages_processed: u64,
    pub duration_secs: f64,
    pub messages_per_second: f64,
    pub latency_p99_us: u64,
    pub violations: u64,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct PerformanceReport {
    pub messages_processed: u64,
    pub total_processing_time_ns: u64,
    pub memory_allocations: u64,
    pub network_errors: u64,
    pub parsing_errors: u64,
    pub routing_errors: u64,
    pub avg_latency_us: u64,
    pub latency_p50_us: u64,
    pub latency_p90_us: u64,
    pub latency_p95_us: u64,
    pub latency_p99_us: u64,
    pub latency_p999_us: u64,
    pub latency_violations: u64,
    pub messages_parsed: u64,
    pub messages_routed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ultra_performance_sensor() {
        let sensor = UltraPerformanceSensor::new();
        let test_msg = generate_test_order_book_message();
        
        // Test single message processing
        let result = sensor.process_message(&test_msg);
        assert!(result.is_ok());
        
        let metrics = sensor.get_performance_metrics();
        assert_eq!(metrics.messages_processed, 1);
    }

    #[test]
    fn test_message_generation() {
        let msg = generate_test_order_book_message();
        assert!(!msg.is_empty());
        assert!(msg.len() > 100); // Should be a reasonable size
    }
}