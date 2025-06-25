//! Latency tracking and performance monitoring for trading operations.

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// High-precision latency tracker for trading operations
#[derive(Debug)]
pub struct LatencyTracker {
    measurements: Arc<RwLock<VecDeque<LatencyMeasurement>>>,
    max_samples: usize,
    total_measurements: AtomicUsize,
    total_latency_ns: AtomicU64,
    min_latency_ns: AtomicU64,
    max_latency_ns: AtomicU64,
    p95_latency_ns: AtomicU64,
    p99_latency_ns: AtomicU64,
    last_update: Arc<RwLock<Option<Instant>>>,
}

#[derive(Debug, Clone)]
pub struct LatencyMeasurement {
    pub timestamp: DateTime<Utc>,
    pub latency_ns: u64,
    pub operation: String,
    pub exchange: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub count: usize,
    pub average_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub total_ns: u64,
    pub last_update: Option<Instant>,
}

impl LatencyTracker {
    /// Create a new latency tracker with specified sample size
    pub fn new(max_samples: usize) -> Self {
        Self {
            measurements: Arc::new(RwLock::new(VecDeque::with_capacity(max_samples))),
            max_samples,
            total_measurements: AtomicUsize::new(0),
            total_latency_ns: AtomicU64::new(0),
            min_latency_ns: AtomicU64::new(u64::MAX),
            max_latency_ns: AtomicU64::new(0),
            p95_latency_ns: AtomicU64::new(0),
            p99_latency_ns: AtomicU64::new(0),
            last_update: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Record a latency measurement
    pub fn record(&self, latency: Duration, operation: String, exchange: Option<String>) {
        let latency_ns = latency.as_nanos() as u64;
        
        let measurement = LatencyMeasurement {
            timestamp: Utc::now(),
            latency_ns,
            operation,
            exchange,
        };
        
        // Update atomic counters
        self.total_measurements.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ns.fetch_add(latency_ns, Ordering::Relaxed);
        
        // Update min/max
        self.update_min_max(latency_ns);
        
        // Store measurement
        {
            let mut measurements = self.measurements.write();
            measurements.push_back(measurement);
            
            // Remove old measurements if we exceed capacity
            while measurements.len() > self.max_samples {
                measurements.pop_front();
            }
        }
        
        // Update percentiles periodically
        if self.total_measurements.load(Ordering::Relaxed) % 100 == 0 {
            self.update_percentiles();
        }
        
        *self.last_update.write() = Some(Instant::now());
    }
    
    /// Record latency from a start time
    pub fn record_since(&self, start: Instant, operation: String, exchange: Option<String>) {
        let latency = start.elapsed();
        self.record(latency, operation, exchange);
    }
    
    /// Get current latency statistics
    pub fn stats(&self) -> LatencyStats {
        let count = self.total_measurements.load(Ordering::Relaxed);
        let total_ns = self.total_latency_ns.load(Ordering::Relaxed);
        
        LatencyStats {
            count,
            average_ns: if count > 0 { total_ns / count as u64 } else { 0 },
            min_ns: self.min_latency_ns.load(Ordering::Relaxed),
            max_ns: self.max_latency_ns.load(Ordering::Relaxed),
            p95_ns: self.p95_latency_ns.load(Ordering::Relaxed),
            p99_ns: self.p99_latency_ns.load(Ordering::Relaxed),
            total_ns,
            last_update: *self.last_update.read(),
        }
    }
    
    /// Check if latency exceeds threshold
    pub fn exceeds_threshold(&self, threshold_ns: u64) -> bool {
        let stats = self.stats();
        stats.average_ns > threshold_ns || stats.p99_ns > threshold_ns
    }
    
    /// Get recent measurements for detailed analysis
    pub fn recent_measurements(&self, count: usize) -> Vec<LatencyMeasurement> {
        let measurements = self.measurements.read();
        measurements
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect()
    }
    
    /// Reset all measurements
    pub fn reset(&self) {
        self.measurements.write().clear();
        self.total_measurements.store(0, Ordering::Relaxed);
        self.total_latency_ns.store(0, Ordering::Relaxed);
        self.min_latency_ns.store(u64::MAX, Ordering::Relaxed);
        self.max_latency_ns.store(0, Ordering::Relaxed);
        self.p95_latency_ns.store(0, Ordering::Relaxed);
        self.p99_latency_ns.store(0, Ordering::Relaxed);
        *self.last_update.write() = None;
    }
    
    fn update_min_max(&self, latency_ns: u64) {
        // Update minimum
        let mut current_min = self.min_latency_ns.load(Ordering::Relaxed);
        while latency_ns < current_min {
            match self.min_latency_ns.compare_exchange_weak(
                current_min,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_min = actual,
            }
        }
        
        // Update maximum
        let mut current_max = self.max_latency_ns.load(Ordering::Relaxed);
        while latency_ns > current_max {
            match self.max_latency_ns.compare_exchange_weak(
                current_max,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
    }
    
    fn update_percentiles(&self) {
        let measurements = self.measurements.read();
        if measurements.len() < 10 {
            return;
        }
        
        let mut latencies: Vec<u64> = measurements.iter().map(|m| m.latency_ns).collect();
        latencies.sort_unstable();
        
        if !latencies.is_empty() {
            let p95_idx = (latencies.len() as f64 * 0.95) as usize;
            let p99_idx = (latencies.len() as f64 * 0.99) as usize;
            
            let p95 = latencies.get(p95_idx).copied().unwrap_or(0);
            let p99 = latencies.get(p99_idx).copied().unwrap_or(0);
            
            self.p95_latency_ns.store(p95, Ordering::Relaxed);
            self.p99_latency_ns.store(p99, Ordering::Relaxed);
        }
    }
}

/// Latency measurement helper for timing operations
pub struct LatencyTimer {
    start: Instant,
    tracker: Arc<LatencyTracker>,
    operation: String,
    exchange: Option<String>,
}

impl LatencyTimer {
    /// Start timing an operation
    pub fn start(
        tracker: Arc<LatencyTracker>,
        operation: String,
        exchange: Option<String>,
    ) -> Self {
        Self {
            start: Instant::now(),
            tracker,
            operation,
            exchange,
        }
    }
    
    /// Stop timing and record the measurement
    pub fn stop(self) {
        let latency = self.start.elapsed();
        self.tracker.record(latency, self.operation, self.exchange);
    }
    
    /// Get elapsed time without stopping the timer
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Global latency trackers for different trading operations
pub struct GlobalLatencyTrackers {
    pub market_data_processing: Arc<LatencyTracker>,
    pub order_book_aggregation: Arc<LatencyTracker>,
    pub order_execution: Arc<LatencyTracker>,
    pub strategy_evaluation: Arc<LatencyTracker>,
    pub database_queries: Arc<LatencyTracker>,
    pub websocket_processing: Arc<LatencyTracker>,
}

impl GlobalLatencyTrackers {
    /// Initialize global latency trackers
    pub fn new() -> Self {
        Self {
            market_data_processing: Arc::new(LatencyTracker::new(10000)),
            order_book_aggregation: Arc::new(LatencyTracker::new(5000)),
            order_execution: Arc::new(LatencyTracker::new(5000)),
            strategy_evaluation: Arc::new(LatencyTracker::new(5000)),
            database_queries: Arc::new(LatencyTracker::new(5000)),
            websocket_processing: Arc::new(LatencyTracker::new(10000)),
        }
    }
    
    /// Get all stats as a summary
    pub fn summary(&self) -> LatencyTrackerSummary {
        LatencyTrackerSummary {
            market_data_processing: self.market_data_processing.stats(),
            order_book_aggregation: self.order_book_aggregation.stats(),
            order_execution: self.order_execution.stats(),
            strategy_evaluation: self.strategy_evaluation.stats(),
            database_queries: self.database_queries.stats(),
            websocket_processing: self.websocket_processing.stats(),
        }
    }
    
    /// Check if any tracker exceeds critical thresholds
    pub fn check_thresholds(&self) -> Vec<LatencyAlert> {
        let mut alerts = Vec::new();
        
        // Market data processing should be < 100ms
        if self.market_data_processing.exceeds_threshold(100_000_000) {
            alerts.push(LatencyAlert {
                operation: "market_data_processing".to_string(),
                threshold_ns: 100_000_000,
                actual_stats: self.market_data_processing.stats(),
            });
        }
        
        // Order book aggregation should be < 50ms
        if self.order_book_aggregation.exceeds_threshold(50_000_000) {
            alerts.push(LatencyAlert {
                operation: "order_book_aggregation".to_string(),
                threshold_ns: 50_000_000,
                actual_stats: self.order_book_aggregation.stats(),
            });
        }
        
        // Order execution should be < 500ms
        if self.order_execution.exceeds_threshold(500_000_000) {
            alerts.push(LatencyAlert {
                operation: "order_execution".to_string(),
                threshold_ns: 500_000_000,
                actual_stats: self.order_execution.stats(),
            });
        }
        
        // Strategy evaluation should be < 50ms
        if self.strategy_evaluation.exceeds_threshold(50_000_000) {
            alerts.push(LatencyAlert {
                operation: "strategy_evaluation".to_string(),
                threshold_ns: 50_000_000,
                actual_stats: self.strategy_evaluation.stats(),
            });
        }
        
        alerts
    }
}

#[derive(Debug, Clone)]
pub struct LatencyTrackerSummary {
    pub market_data_processing: LatencyStats,
    pub order_book_aggregation: LatencyStats,
    pub order_execution: LatencyStats,
    pub strategy_evaluation: LatencyStats,
    pub database_queries: LatencyStats,
    pub websocket_processing: LatencyStats,
}

#[derive(Debug, Clone)]
pub struct LatencyAlert {
    pub operation: String,
    pub threshold_ns: u64,
    pub actual_stats: LatencyStats,
}

use std::sync::OnceLock;

/// Global latency trackers instance
pub static GLOBAL_LATENCY_TRACKERS: OnceLock<GlobalLatencyTrackers> = OnceLock::new();

/// Get or initialize global latency trackers
pub fn get_global_latency_trackers() -> &'static GlobalLatencyTrackers {
    GLOBAL_LATENCY_TRACKERS.get_or_init(|| GlobalLatencyTrackers::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};
    
    #[test]
    fn test_latency_tracker_basic() {
        let tracker = LatencyTracker::new(100);
        
        // Record some measurements
        tracker.record(Duration::from_millis(10), "test_op".to_string(), None);
        tracker.record(Duration::from_millis(20), "test_op".to_string(), None);
        tracker.record(Duration::from_millis(30), "test_op".to_string(), None);
        
        let stats = tracker.stats();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.min_ns, 10_000_000);
        assert_eq!(stats.max_ns, 30_000_000);
        assert_eq!(stats.average_ns, 20_000_000);
    }
    
    #[test]
    fn test_latency_timer() {
        let tracker = Arc::new(LatencyTracker::new(100));
        
        {
            let timer = LatencyTimer::start(
                tracker.clone(),
                "test_operation".to_string(),
                Some("test_exchange".to_string()),
            );
            
            thread::sleep(Duration::from_millis(10));
            timer.stop();
        }
        
        let stats = tracker.stats();
        assert_eq!(stats.count, 1);
        assert!(stats.average_ns >= 10_000_000);
    }
    
    #[test]
    fn test_threshold_checking() {
        let tracker = LatencyTracker::new(100);
        
        // Record measurement below threshold
        tracker.record(Duration::from_millis(5), "test_op".to_string(), None);
        assert!(!tracker.exceeds_threshold(10_000_000));
        
        // Record measurement above threshold
        tracker.record(Duration::from_millis(15), "test_op".to_string(), None);
        assert!(tracker.exceeds_threshold(10_000_000));
    }
}