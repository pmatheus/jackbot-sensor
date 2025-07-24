//! Nanosecond-precision latency monitoring system
//! 
//! Tracks message processing latency with sub-microsecond precision
//! to ensure <10ms P99 latency requirements are met.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use parking_lot::RwLock;

/// Nanosecond-precision latency tracker
pub struct LatencyMonitor {
    measurements: Arc<RwLock<Vec<u64>>>,
    violations: AtomicU64,
    total_measurements: AtomicU64,
    max_latency_ns: AtomicU64,
    min_latency_ns: AtomicU64,
    sum_latency_ns: AtomicU64,
}

impl LatencyMonitor {
    const MAX_LATENCY_NS: u64 = 10_000_000; // 10ms in nanoseconds
    
    pub fn new(capacity: usize) -> Self {
        Self {
            measurements: Arc::new(RwLock::new(Vec::with_capacity(capacity))),
            violations: AtomicU64::new(0),
            total_measurements: AtomicU64::new(0),
            max_latency_ns: AtomicU64::new(0),
            min_latency_ns: AtomicU64::new(u64::MAX),
            sum_latency_ns: AtomicU64::new(0),
        }
    }

    /// Record a latency measurement in nanoseconds
    pub fn record_latency_ns(&self, latency_ns: u64) {
        // Update atomic counters first (fastest path)
        self.total_measurements.fetch_add(1, Ordering::Relaxed);
        self.sum_latency_ns.fetch_add(latency_ns, Ordering::Relaxed);
        
        // Update min/max
        self.max_latency_ns.fetch_max(latency_ns, Ordering::Relaxed);
        self.min_latency_ns.fetch_min(latency_ns, Ordering::Relaxed);
        
        // Check for violations
        if latency_ns > Self::MAX_LATENCY_NS {
            self.violations.fetch_add(1, Ordering::Relaxed);
            eprintln!("❌ LATENCY VIOLATION: {}μs > 10ms MAX", latency_ns / 1000);
        }
        
        // Store measurement for percentile calculations (slower path)
        if let Some(mut measurements) = self.measurements.try_write() {
            if measurements.len() < measurements.capacity() {
                measurements.push(latency_ns);
            } else {
                // Circular buffer behavior - overwrite oldest
                let index = self.total_measurements.load(Ordering::Relaxed) as usize % measurements.capacity();
                measurements[index] = latency_ns;
            }
        }
    }

    /// Record latency using RAII timing
    pub fn time_operation<F, R>(&self, operation: F) -> R 
    where 
        F: FnOnce() -> R 
    {
        let start = Instant::now();
        let result = operation();
        let latency_ns = start.elapsed().as_nanos() as u64;
        self.record_latency_ns(latency_ns);
        result
    }

    /// Get latency percentiles (P50, P90, P95, P99, P99.9) in microseconds
    pub fn get_percentiles(&self) -> (u64, u64, u64, u64, u64) {
        let measurements = self.measurements.read();
        if measurements.is_empty() {
            return (0, 0, 0, 0, 0);
        }

        let mut sorted = measurements.clone();
        sorted.sort_unstable();
        
        let len = sorted.len();
        let p50 = sorted[len * 50 / 100] / 1000;   // Convert to μs
        let p90 = sorted[len * 90 / 100] / 1000;
        let p95 = sorted[len * 95 / 100] / 1000;
        let p99 = sorted[len * 99 / 100] / 1000;
        let p999 = sorted[len * 999 / 1000] / 1000;
        
        (p50, p90, p95, p99, p999)
    }

    /// Get basic statistics
    pub fn get_stats(&self) -> LatencyStats {
        let total = self.total_measurements.load(Ordering::Relaxed);
        let violations = self.violations.load(Ordering::Relaxed);
        let sum = self.sum_latency_ns.load(Ordering::Relaxed);
        let max_ns = self.max_latency_ns.load(Ordering::Relaxed);
        let min_ns = self.min_latency_ns.load(Ordering::Relaxed);
        
        LatencyStats {
            total_measurements: total,
            violations,
            avg_latency_us: if total > 0 { (sum / total) / 1000 } else { 0 },
            max_latency_us: max_ns / 1000,
            min_latency_us: if min_ns == u64::MAX { 0 } else { min_ns / 1000 },
            violation_rate: if total > 0 { violations as f64 / total as f64 * 100.0 } else { 0.0 },
        }
    }

    /// Check if system is meeting latency requirements
    pub fn is_meeting_requirements(&self) -> bool {
        self.violations.load(Ordering::Relaxed) == 0
    }

    /// Reset all measurements
    pub fn reset(&self) {
        self.measurements.write().clear();
        self.violations.store(0, Ordering::Relaxed);
        self.total_measurements.store(0, Ordering::Relaxed);
        self.max_latency_ns.store(0, Ordering::Relaxed);
        self.min_latency_ns.store(u64::MAX, Ordering::Relaxed);
        self.sum_latency_ns.store(0, Ordering::Relaxed);
    }
}

/// Latency statistics summary
#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub total_measurements: u64,
    pub violations: u64,
    pub avg_latency_us: u64,
    pub max_latency_us: u64,
    pub min_latency_us: u64,
    pub violation_rate: f64,
}

/// High-performance message processing pipeline with latency tracking
pub struct MessagePipeline {
    latency_monitor: Arc<LatencyMonitor>,
    processed_count: AtomicU64,
    error_count: AtomicU64,
}

impl MessagePipeline {
    pub fn new(latency_capacity: usize) -> Self {
        Self {
            latency_monitor: Arc::new(LatencyMonitor::new(latency_capacity)),
            processed_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        }
    }

    /// Process a message with latency tracking
    pub fn process_message<T>(&self, message: T, processor: impl FnOnce(T) -> Result<(), Box<dyn std::error::Error>>) {
        let result = self.latency_monitor.time_operation(|| {
            processor(message)
        });

        match result {
            Ok(_) => {
                self.processed_count.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                self.error_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Get processing statistics
    pub fn get_processing_stats(&self) -> ProcessingStats {
        ProcessingStats {
            processed_count: self.processed_count.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            latency_stats: self.latency_monitor.get_stats(),
        }
    }

    /// Get latency monitor reference
    pub fn latency_monitor(&self) -> &Arc<LatencyMonitor> {
        &self.latency_monitor
    }

    /// Run performance benchmark
    pub fn benchmark(&self, message_count: usize, operation_us: u64) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔥 Starting message pipeline benchmark...");
        println!("  Target: {} messages", message_count);
        println!("  Simulated operation: {}μs", operation_us);
        
        let start_time = Instant::now();
        
        for i in 0..message_count {
            let message = format!("test_message_{}", i);
            
            self.process_message(message, |_msg| {
                // Simulate processing time
                if operation_us > 0 {
                    std::thread::sleep(std::time::Duration::from_micros(operation_us));
                }
                Ok(())
            });

            // Log progress every 100K messages
            if i % 100_000 == 0 && i > 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rate = i as f64 / elapsed;
                println!("📊 Processed {} messages in {:.2}s ({:.0} msg/s)", i, elapsed, rate);
            }
        }

        let total_time = start_time.elapsed();
        let rate = message_count as f64 / total_time.as_secs_f64();
        let (p50, p90, p95, p99, p999) = self.latency_monitor.get_percentiles();
        let stats = self.get_processing_stats();

        println!("🏆 MESSAGE PIPELINE BENCHMARK RESULTS:");
        println!("  Messages processed: {}", stats.processed_count);
        println!("  Error count: {}", stats.error_count);
        println!("  Total time: {:.2}s", total_time.as_secs_f64());
        println!("  Messages/sec: {:.0}", rate);
        println!("  Latency P50/P90/P95/P99/P99.9: {}/{}/{}/{}/{} μs", p50, p90, p95, p99, p999);
        println!("  Latency violations: {}", stats.latency_stats.violations);
        println!("  Violation rate: {:.3}%", stats.latency_stats.violation_rate);

        // Verify requirements
        if rate < 1_000_000.0 {
            return Err(format!("❌ Failed throughput requirement: {:.0} msg/s < 1M msg/s", rate).into());
        }
        
        if p99 > 10_000 {
            return Err(format!("❌ Failed latency requirement: P99 {}μs > 10ms", p99).into());
        }
        
        if stats.latency_stats.violations > 0 {
            return Err(format!("❌ Had {} latency violations", stats.latency_stats.violations).into());
        }

        println!("✅ MESSAGE PIPELINE BENCHMARK PASSED!");
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ProcessingStats {
    pub processed_count: u64,
    pub error_count: u64,
    pub latency_stats: LatencyStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_latency_monitor() {
        let monitor = LatencyMonitor::new(1000);
        
        // Record some test latencies
        monitor.record_latency_ns(1_000_000); // 1ms
        monitor.record_latency_ns(5_000_000); // 5ms
        monitor.record_latency_ns(2_000_000); // 2ms
        
        let stats = monitor.get_stats();
        assert_eq!(stats.total_measurements, 3);
        assert_eq!(stats.violations, 0);
        assert!(stats.avg_latency_us > 0);
    }

    #[test]
    fn test_latency_violation() {
        let monitor = LatencyMonitor::new(1000);
        
        // Record a violation (15ms > 10ms max)
        monitor.record_latency_ns(15_000_000);
        
        let stats = monitor.get_stats();
        assert_eq!(stats.violations, 1);
        assert!(!monitor.is_meeting_requirements());
    }

    #[test]
    fn test_message_pipeline() {
        let pipeline = MessagePipeline::new(1000);
        
        // Process some test messages
        for i in 0..10 {
            let message = format!("test_{}", i);
            pipeline.process_message(message, |_| Ok(()));
        }
        
        let stats = pipeline.get_processing_stats();
        assert_eq!(stats.processed_count, 10);
        assert_eq!(stats.error_count, 0);
    }

    #[test]
    fn test_timing_operation() {
        let monitor = LatencyMonitor::new(1000);
        
        let result = monitor.time_operation(|| {
            thread::sleep(Duration::from_millis(1));
            42
        });
        
        assert_eq!(result, 42);
        
        let stats = monitor.get_stats();
        assert_eq!(stats.total_measurements, 1);
        assert!(stats.avg_latency_us >= 1000); // At least 1ms
    }
}