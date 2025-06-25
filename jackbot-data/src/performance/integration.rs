//! Integration module for high-performance trading operations.

use super::{
    memory_pool::{MemoryPool, get_global_pools},
    safe_ring_buffer::{SafeRingBuffer, ChannelRingBuffer},
    latency_tracker::{LatencyTracker, get_global_latency_trackers, LatencyTimer},
};
use crate::books::aggregator::{OrderBookAggregator, AggregatorMetrics};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{info, warn, error};

/// High-performance market data processor with monitoring
pub struct PerformanceOptimizedProcessor {
    /// Buffer for incoming market data
    pub data_buffer: SafeRingBuffer<Vec<u8>>,
    /// Buffer for processed events  
    pub event_buffer: ChannelRingBuffer<ProcessedEvent>,
    /// Latency tracker for this processor
    pub latency_tracker: Arc<LatencyTracker>,
    /// Performance configuration
    pub config: ProcessorConfig,
    /// Last performance check timestamp
    last_perf_check: Arc<parking_lot::Mutex<Option<Instant>>>,
}

#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// Maximum buffer size for incoming data
    pub max_buffer_size: usize,
    /// Target latency threshold in microseconds
    pub target_latency_us: u64,
    /// Performance check interval
    pub perf_check_interval: Duration,
    /// Whether to use memory pools
    pub use_memory_pools: bool,
    /// Number of worker threads for parallel processing
    pub num_workers: usize,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            max_buffer_size: 8192,
            target_latency_us: 100_000, // 100ms
            perf_check_interval: Duration::from_secs(60),
            use_memory_pools: true,
            num_workers: num_cpus::get(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessedEvent {
    pub data: Vec<u8>,
    pub processing_time_us: u64,
    pub timestamp: Instant,
    pub exchange: Option<String>,
}

impl PerformanceOptimizedProcessor {
    /// Create a new performance-optimized processor
    pub fn new(config: ProcessorConfig) -> Self {
        Self {
            data_buffer: SafeRingBuffer::new(config.max_buffer_size),
            event_buffer: ChannelRingBuffer::new(config.max_buffer_size),
            latency_tracker: Arc::new(LatencyTracker::new(10000)),
            config,
            last_perf_check: Arc::new(parking_lot::Mutex::new(None)),
        }
    }
    
    /// Process incoming market data with performance monitoring
    pub fn process_market_data(
        &self,
        data: Vec<u8>,
        exchange: Option<String>,
    ) -> Result<(), ProcessingError> {
        let timer = LatencyTimer::start(
            self.latency_tracker.clone(),
            "market_data_processing".to_string(),
            exchange.clone(),
        );
        
        // Use memory pool if configured
        let processed_data = if self.config.use_memory_pools {
            let pools = get_global_pools();
            let mut buffer = pools.message_buffers.acquire();
            buffer.clear();
            buffer.extend_from_slice(&data);
            buffer.to_vec()
        } else {
            data
        };
        
        // Store in ring buffer for processing
        self.data_buffer.push(processed_data.clone())
            .map_err(|_| ProcessingError::BufferFull)?;
        
        let processing_time_us = timer.elapsed().as_micros() as u64;
        
        // Create processed event
        let event = ProcessedEvent {
            data: processed_data,
            processing_time_us,
            timestamp: Instant::now(),
            exchange,
        };
        
        // Send to event buffer
        self.event_buffer.send(event)
            .map_err(|_| ProcessingError::EventBufferFull)?;
        
        timer.stop();
        
        // Check performance periodically
        self.maybe_check_performance();
        
        Ok(())
    }
    
    /// Get the next processed event
    pub fn get_next_event(&self) -> Option<ProcessedEvent> {
        self.event_buffer.recv()
    }
    
    /// Get performance metrics
    pub fn get_performance_metrics(&self) -> ProcessorMetrics {
        let latency_stats = self.latency_tracker.stats();
        let data_buffer_stats = self.data_buffer.stats();
        let event_buffer_metrics = self.event_buffer.metrics();
        
        ProcessorMetrics {
            latency_stats,
            data_buffer_utilization: data_buffer_stats.utilization,
            data_buffer_dropped: data_buffer_stats.dropped_count,
            event_buffer_messages_sent: event_buffer_metrics.messages_sent,
            event_buffer_messages_received: event_buffer_metrics.messages_received,
            event_buffer_messages_dropped: event_buffer_metrics.messages_dropped,
            target_latency_us: self.config.target_latency_us,
            is_meeting_targets: latency_stats.average_ns <= (self.config.target_latency_us * 1000),
        }
    }
    
    /// Check if performance targets are being met
    pub fn is_meeting_performance_targets(&self) -> bool {
        let metrics = self.get_performance_metrics();
        metrics.is_meeting_targets && metrics.data_buffer_utilization < 90.0
    }
    
    fn maybe_check_performance(&self) {
        let mut last_check = self.last_perf_check.lock();
        let now = Instant::now();
        
        let should_check = match *last_check {
            Some(last) => now.duration_since(last) >= self.config.perf_check_interval,
            None => true,
        };
        
        if should_check {
            *last_check = Some(now);
            drop(last_check);
            
            let metrics = self.get_performance_metrics();
            
            if !metrics.is_meeting_targets {
                warn!(
                    average_latency_us = metrics.latency_stats.average_ns / 1000,
                    target_latency_us = self.config.target_latency_us,
                    buffer_utilization = metrics.data_buffer_utilization,
                    "Performance targets not being met"
                );
                
                // Log global tracker state
                let global_trackers = get_global_latency_trackers();
                let alerts = global_trackers.check_thresholds();
                
                for alert in alerts {
                    error!(
                        operation = %alert.operation,
                        threshold_ms = alert.threshold_ns / 1_000_000,
                        actual_avg_ms = alert.actual_stats.average_ns / 1_000_000,
                        actual_p99_ms = alert.actual_stats.p99_ns / 1_000_000,
                        "Latency threshold exceeded"
                    );
                }
            } else {
                info!(
                    average_latency_us = metrics.latency_stats.average_ns / 1000,
                    buffer_utilization = metrics.data_buffer_utilization,
                    "Performance targets being met"
                );
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessorMetrics {
    pub latency_stats: super::latency_tracker::LatencyStats,
    pub data_buffer_utilization: f64,
    pub data_buffer_dropped: usize,
    pub event_buffer_messages_sent: u64,
    pub event_buffer_messages_received: u64,
    pub event_buffer_messages_dropped: u64,
    pub target_latency_us: u64,
    pub is_meeting_targets: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessingError {
    #[error("Data buffer is full")]
    BufferFull,
    #[error("Event buffer is full")]
    EventBufferFull,
    #[error("Processing timeout")]
    Timeout,
}

/// Enhanced order book aggregator with performance monitoring
pub struct MonitoredOrderBookAggregator {
    inner: OrderBookAggregator,
    latency_tracker: Arc<LatencyTracker>,
}

impl MonitoredOrderBookAggregator {
    /// Create a new monitored aggregator
    pub fn new(aggregator: OrderBookAggregator) -> Self {
        Self {
            inner: aggregator,
            latency_tracker: Arc::new(LatencyTracker::new(5000)),
        }
    }
    
    /// Aggregate with performance monitoring
    pub fn aggregate(&self, depth: usize) -> crate::books::OrderBook {
        let timer = LatencyTimer::start(
            self.latency_tracker.clone(),
            "order_book_aggregation".to_string(),
            None,
        );
        
        let result = self.inner.aggregate(depth);
        timer.stop();
        
        // Record in global tracker as well
        let global_trackers = get_global_latency_trackers();
        global_trackers.order_book_aggregation.record(
            timer.elapsed(),
            "order_book_aggregation".to_string(),
            None,
        );
        
        result
    }
    
    /// Get aggregator performance metrics
    pub fn get_aggregator_metrics(&self) -> AggregatorMetrics {
        self.inner.get_performance_metrics()
    }
    
    /// Get latency metrics
    pub fn get_latency_metrics(&self) -> super::latency_tracker::LatencyStats {
        self.latency_tracker.stats()
    }
}

/// Global performance monitoring and alerting
pub struct GlobalPerformanceMonitor;

impl GlobalPerformanceMonitor {
    /// Check all global performance metrics and return alerts
    pub fn check_performance() -> Vec<PerformanceAlert> {
        let mut alerts = Vec::new();
        let global_trackers = get_global_latency_trackers();
        
        // Check latency thresholds
        let latency_alerts = global_trackers.check_thresholds();
        for alert in latency_alerts {
            alerts.push(PerformanceAlert::LatencyThresholdExceeded {
                operation: alert.operation,
                threshold_ns: alert.threshold_ns,
                actual_avg_ns: alert.actual_stats.average_ns,
                actual_p99_ns: alert.actual_stats.p99_ns,
            });
        }
        
        // Check memory pool utilization
        let pools = get_global_pools();
        if pools.order_book_levels.size() == 0 && pools.order_book_levels.total_allocated() > 100 {
            alerts.push(PerformanceAlert::MemoryPoolExhausted {
                pool_type: "order_book_levels".to_string(),
                allocated: pools.order_book_levels.total_allocated(),
            });
        }
        
        alerts
    }
    
    /// Get summary of all performance metrics
    pub fn get_performance_summary() -> PerformanceSummary {
        let global_trackers = get_global_latency_trackers();
        let pools = get_global_pools();
        
        PerformanceSummary {
            latency_summary: global_trackers.summary(),
            memory_pool_summary: MemoryPoolSummary {
                order_book_levels_size: pools.order_book_levels.size(),
                order_book_levels_allocated: pools.order_book_levels.total_allocated(),
                message_buffers_size: pools.message_buffers.size(),
                message_buffers_allocated: pools.message_buffers.total_allocated(),
                price_vectors_size: pools.price_vectors.size(),
                price_vectors_allocated: pools.price_vectors.total_allocated(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum PerformanceAlert {
    LatencyThresholdExceeded {
        operation: String,
        threshold_ns: u64,
        actual_avg_ns: u64,
        actual_p99_ns: u64,
    },
    MemoryPoolExhausted {
        pool_type: String,
        allocated: usize,
    },
}

#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    pub latency_summary: super::latency_tracker::LatencyTrackerSummary,
    pub memory_pool_summary: MemoryPoolSummary,
}

#[derive(Debug, Clone)]
pub struct MemoryPoolSummary {
    pub order_book_levels_size: usize,
    pub order_book_levels_allocated: usize,
    pub message_buffers_size: usize,
    pub message_buffers_allocated: usize,
    pub price_vectors_size: usize,
    pub price_vectors_allocated: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_performance_optimized_processor() {
        let config = ProcessorConfig::default();
        let processor = PerformanceOptimizedProcessor::new(config);
        
        // Process some test data
        let test_data = b"test market data".to_vec();
        assert!(processor.process_market_data(test_data, Some("test_exchange".to_string())).is_ok());
        
        // Check that we can retrieve the event
        let event = processor.get_next_event();
        assert!(event.is_some());
        
        let metrics = processor.get_performance_metrics();
        assert!(metrics.latency_stats.count > 0);
    }
    
    #[test]
    fn test_global_performance_monitor() {
        let alerts = GlobalPerformanceMonitor::check_performance();
        // Should not have alerts in test environment
        assert!(alerts.is_empty());
        
        let summary = GlobalPerformanceMonitor::get_performance_summary();
        // Should have valid summary
        assert!(summary.memory_pool_summary.order_book_levels_size >= 0);
    }
}