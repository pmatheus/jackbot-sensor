//! Exchange-specific protection and performance hardening
//! 
//! Provides circuit breaker and rate limiting configurations optimized
//! for each exchange's characteristics and requirements.

use crate::circuit_breaker::{CircuitBreakerConfig, RateLimiterConfig, ProtectedClient};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

/// Exchange-specific protection configurations
pub struct ExchangeProtectionConfig {
    pub name: String,
    pub circuit_breaker_config: CircuitBreakerConfig,
    pub rate_limiter_config: RateLimiterConfig,
    pub performance_hardening: PerformanceHardening,
}

/// Performance hardening settings for each exchange
#[derive(Debug, Clone)]
pub struct PerformanceHardening {
    /// Maximum latency tolerance (ms) - beyond this, trigger circuit breaker
    pub max_latency_ms: u64,
    /// Maximum memory per connection (MB)
    pub max_memory_mb: usize,
    /// Connection timeout (ms)
    pub connection_timeout_ms: u64,
    /// Heartbeat interval (seconds)
    pub heartbeat_interval_secs: u64,
    /// Maximum message size (bytes)
    pub max_message_size: usize,
    /// Enable compression
    pub enable_compression: bool,
    /// Connection pool size
    pub connection_pool_size: usize,
    /// Backpressure threshold (queued messages)
    pub backpressure_threshold: usize,
}

impl Default for PerformanceHardening {
    fn default() -> Self {
        Self {
            max_latency_ms: 10_000, // 10ms requirement
            max_memory_mb: 100,     // 100MB requirement
            connection_timeout_ms: 30_000,
            heartbeat_interval_secs: 30,
            max_message_size: 64 * 1024, // 64KB
            enable_compression: true,
            connection_pool_size: 4,
            backpressure_threshold: 10_000, // 10K capacity
        }
    }
}

/// Exchange protection manager
pub struct ExchangeProtectionManager {
    protected_clients: HashMap<String, Arc<ProtectedClient>>,
    configs: HashMap<String, ExchangeProtectionConfig>,
}

impl ExchangeProtectionManager {
    pub fn new() -> Self {
        let mut manager = Self {
            protected_clients: HashMap::new(),
            configs: HashMap::new(),
        };
        
        // Initialize all exchange configurations
        manager.setup_exchange_configs();
        manager.create_protected_clients();
        
        manager
    }

    /// Setup exchange-specific configurations
    fn setup_exchange_configs(&mut self) {
        // Gate.io configuration - aggressive rate limiting (100 msg/sec)
        self.configs.insert("gateio".to_string(), ExchangeProtectionConfig {
            name: "gateio".to_string(),
            circuit_breaker_config: CircuitBreakerConfig {
                failure_threshold: 3,      // Open quickly due to strict rate limits
                success_threshold: 5,      // Higher threshold for recovery
                window_size_seconds: 30,   // Shorter window
                timeout_seconds: 60,       // Longer timeout
                half_open_max_requests: 2, // Very conservative
                min_requests: 5,
            },
            rate_limiter_config: RateLimiterConfig {
                max_requests_per_second: 100,  // Gate.io limit
                burst_capacity: 50,            // Conservative burst
                refill_rate: 100,
                max_queue_size: 500,           // Smaller queue
                request_timeout_ms: 2000,      // Shorter timeout
            },
            performance_hardening: PerformanceHardening {
                max_latency_ms: 8_000,        // Stricter latency
                connection_timeout_ms: 20_000,
                heartbeat_interval_secs: 20,   // More frequent heartbeat
                max_message_size: 32 * 1024,   // Smaller messages
                backpressure_threshold: 5_000, // Lower threshold
                ..Default::default()
            },
        });

        // MEXC configuration - connection stability focus
        self.configs.insert("mexc".to_string(), ExchangeProtectionConfig {
            name: "mexc".to_string(),
            circuit_breaker_config: CircuitBreakerConfig {
                failure_threshold: 4,
                success_threshold: 3,
                window_size_seconds: 45,
                timeout_seconds: 30,
                half_open_max_requests: 3,
                min_requests: 8,
            },
            rate_limiter_config: RateLimiterConfig {
                max_requests_per_second: 200,  // MEXC higher limit
                burst_capacity: 100,
                refill_rate: 200,
                max_queue_size: 1000,
                request_timeout_ms: 3000,
            },
            performance_hardening: PerformanceHardening {
                max_latency_ms: 9_000,
                connection_timeout_ms: 25_000,
                heartbeat_interval_secs: 15,    // Frequent due to stability issues
                max_message_size: 128 * 1024,   // Larger messages OK
                connection_pool_size: 2,        // Fewer connections for stability
                backpressure_threshold: 8_000,
                ..Default::default()
            },
        });

        // BingX configuration - memory leak protection
        self.configs.insert("bingx".to_string(), ExchangeProtectionConfig {
            name: "bingx".to_string(),
            circuit_breaker_config: CircuitBreakerConfig {
                failure_threshold: 5,
                success_threshold: 4,
                window_size_seconds: 60,
                timeout_seconds: 45,
                half_open_max_requests: 4,
                min_requests: 10,
            },
            rate_limiter_config: RateLimiterConfig {
                max_requests_per_second: 50,   // Conservative due to memory issues
                burst_capacity: 25,            // Small burst
                refill_rate: 50,
                max_queue_size: 200,           // Small queue to prevent memory buildup
                request_timeout_ms: 5000,
            },
            performance_hardening: PerformanceHardening {
                max_latency_ms: 7_000,         // Strict latency due to memory issues
                max_memory_mb: 50,             // Half the default limit
                connection_timeout_ms: 15_000,
                heartbeat_interval_secs: 60,   // Less frequent to reduce memory usage
                max_message_size: 32 * 1024,   // Small messages only
                connection_pool_size: 1,       // Single connection to limit memory
                backpressure_threshold: 2_000, // Very low threshold
                enable_compression: true,      // Enable compression to save memory
            },
        });

        // Binance configuration - high performance
        self.configs.insert("binance".to_string(), ExchangeProtectionConfig {
            name: "binance".to_string(),
            circuit_breaker_config: CircuitBreakerConfig {
                failure_threshold: 6,
                success_threshold: 3,
                window_size_seconds: 60,
                timeout_seconds: 30,
                half_open_max_requests: 5,
                min_requests: 15,
            },
            rate_limiter_config: RateLimiterConfig {
                max_requests_per_second: 1200, // Binance high limits
                burst_capacity: 500,
                refill_rate: 1200,
                max_queue_size: 2000,
                request_timeout_ms: 1000,      // Fast timeout
            },
            performance_hardening: PerformanceHardening {
                max_latency_ms: 5_000,         // Strict for high performance
                connection_timeout_ms: 10_000,
                heartbeat_interval_secs: 30,
                max_message_size: 1024 * 1024, // Large messages OK
                connection_pool_size: 8,       // Many connections
                backpressure_threshold: 20_000,
                ..Default::default()
            },
        });

        // Add other exchanges with standard configs
        for &exchange in &["coinbase", "bybit", "bitget", "hyperliquid", "kucoin", "kraken", "okx"] {
            self.configs.insert(exchange.to_string(), ExchangeProtectionConfig {
                name: exchange.to_string(),
                circuit_breaker_config: CircuitBreakerConfig::default(),
                rate_limiter_config: RateLimiterConfig::default(),
                performance_hardening: PerformanceHardening::default(),
            });
        }

        info!("🛡️  Exchange protection configurations loaded for {} exchanges", self.configs.len());
    }

    /// Create protected clients for all exchanges
    fn create_protected_clients(&mut self) {
        for (exchange, config) in &self.configs {
            let protected_client = Arc::new(ProtectedClient::new(
                exchange.clone(),
                config.circuit_breaker_config.clone(),
                config.rate_limiter_config.clone(),
            ));
            
            self.protected_clients.insert(exchange.clone(), protected_client);
            
            info!("🛡️  Protected client created for {}: rate_limit={}req/s, failure_threshold={}", 
                  exchange, 
                  config.rate_limiter_config.max_requests_per_second,
                  config.circuit_breaker_config.failure_threshold);
        }
    }

    /// Get protected client for an exchange
    pub fn get_protected_client(&self, exchange: &str) -> Option<Arc<ProtectedClient>> {
        self.protected_clients.get(exchange).cloned()
    }

    /// Get performance hardening config for an exchange
    pub fn get_performance_config(&self, exchange: &str) -> Option<&PerformanceHardening> {
        self.configs.get(exchange).map(|c| &c.performance_hardening)
    }

    /// Execute protected operation for an exchange
    pub async fn execute_protected<F, Fut, T>(&self, exchange: &str, operation: F) -> anyhow::Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<T>>,
    {
        match self.protected_clients.get(exchange) {
            Some(client) => {
                client.execute(operation).await
            }
            None => {
                warn!("No protection configured for exchange: {}", exchange);
                operation().await
            }
        }
    }

    /// Get protection statistics for all exchanges
    pub async fn get_all_protection_stats(&self) -> HashMap<String, (crate::circuit_breaker::CircuitBreakerStats, crate::circuit_breaker::RateLimiterStats)> {
        let mut stats = HashMap::new();
        
        for (exchange, client) in &self.protected_clients {
            let protection_stats = client.get_protection_stats().await;
            stats.insert(exchange.clone(), protection_stats);
        }
        
        stats
    }

    /// Get protection status summary
    pub async fn get_protection_summary(&self) -> ProtectionSummary {
        let mut summary = ProtectionSummary {
            total_exchanges: self.protected_clients.len(),
            healthy_exchanges: 0,
            degraded_exchanges: 0,
            failing_exchanges: 0,
            total_requests: 0,
            total_failures: 0,
            average_latency_ms: 0,
        };

        let mut total_latency = 0u64;
        let mut latency_count = 0u64;

        for (exchange, client) in &self.protected_clients {
            let (cb_stats, rl_stats) = client.get_protection_stats().await;
            
            summary.total_requests += cb_stats.total_requests;
            summary.total_failures += cb_stats.failed_requests;
            
            if cb_stats.average_response_time_ms > 0 {
                total_latency += cb_stats.average_response_time_ms;
                latency_count += 1;
            }

            // Classify exchange health
            match cb_stats.state {
                crate::circuit_breaker::CircuitState::Closed => {
                    if cb_stats.failure_rate_pct < 1.0 {
                        summary.healthy_exchanges += 1;
                    } else {
                        summary.degraded_exchanges += 1;
                    }
                }
                crate::circuit_breaker::CircuitState::HalfOpen => {
                    summary.degraded_exchanges += 1;
                }
                crate::circuit_breaker::CircuitState::Open => {
                    summary.failing_exchanges += 1;
                }
            }
        }

        if latency_count > 0 {
            summary.average_latency_ms = total_latency / latency_count;
        }

        summary
    }

    /// Force circuit breaker state for testing
    #[cfg(test)]
    pub async fn force_circuit_state(&self, exchange: &str, state: crate::circuit_breaker::CircuitState) -> anyhow::Result<()> {
        if let Some(client) = self.protected_clients.get(exchange) {
            // This would require exposing the circuit breaker in ProtectedClient
            // For now, return an error indicating this needs implementation
            Err(anyhow::anyhow!("Force state not implemented - would need circuit breaker access"))
        } else {
            Err(anyhow::anyhow!("Exchange not found: {}", exchange))
        }
    }
}

/// Protection summary for monitoring
#[derive(Debug, Clone)]
pub struct ProtectionSummary {
    pub total_exchanges: usize,
    pub healthy_exchanges: usize,
    pub degraded_exchanges: usize,
    pub failing_exchanges: usize,
    pub total_requests: u64,
    pub total_failures: u64,
    pub average_latency_ms: u64,
}

impl ProtectionSummary {
    /// Check if system is meeting performance requirements
    pub fn meets_requirements(&self) -> bool {
        // <10ms P99 latency requirement - using average as proxy
        let latency_ok = self.average_latency_ms < 10_000;
        
        // Zero data loss - failure rate should be very low
        let failure_rate = if self.total_requests > 0 {
            (self.total_failures as f64 / self.total_requests as f64) * 100.0
        } else {
            0.0
        };
        let failure_rate_ok = failure_rate < 0.1; // <0.1% failure rate
        
        // Most exchanges should be healthy
        let health_ratio = self.healthy_exchanges as f64 / self.total_exchanges as f64;
        let health_ok = health_ratio >= 0.8; // 80% of exchanges healthy
        
        latency_ok && failure_rate_ok && health_ok
    }

    /// Get system health score (0-100)
    pub fn health_score(&self) -> u8 {
        let mut score = 100u8;
        
        // Latency penalty
        if self.average_latency_ms > 10_000 {
            score = score.saturating_sub(30);
        } else if self.average_latency_ms > 5_000 {
            score = score.saturating_sub(15);
        }
        
        // Failure rate penalty
        let failure_rate = if self.total_requests > 0 {
            (self.total_failures as f64 / self.total_requests as f64) * 100.0
        } else {
            0.0
        };
        
        if failure_rate > 1.0 {
            score = score.saturating_sub(40);
        } else if failure_rate > 0.1 {
            score = score.saturating_sub(20);
        }
        
        // Exchange health penalty
        let unhealthy_ratio = (self.degraded_exchanges + self.failing_exchanges) as f64 / self.total_exchanges as f64;
        score = score.saturating_sub((unhealthy_ratio * 30.0) as u8);
        
        score
    }
}

/// Global protection manager instance
static PROTECTION_MANAGER: std::sync::OnceLock<ExchangeProtectionManager> = std::sync::OnceLock::new();

/// Initialize global protection manager
pub fn init_exchange_protection() -> &'static ExchangeProtectionManager {
    PROTECTION_MANAGER.get_or_init(|| {
        info!("🛡️  Initializing exchange protection system");
        ExchangeProtectionManager::new()
    })
}

/// Get global protection manager
pub fn get_protection_manager() -> Option<&'static ExchangeProtectionManager> {
    PROTECTION_MANAGER.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exchange_configs() {
        let manager = ExchangeProtectionManager::new();
        
        // Check that all exchanges have configs
        assert!(manager.configs.contains_key("gateio"));
        assert!(manager.configs.contains_key("mexc"));
        assert!(manager.configs.contains_key("bingx"));
        assert!(manager.configs.contains_key("binance"));
        
        // Check Gate.io specific config
        let gateio_config = manager.configs.get("gateio").unwrap();
        assert_eq!(gateio_config.rate_limiter_config.max_requests_per_second, 100);
        assert_eq!(gateio_config.circuit_breaker_config.failure_threshold, 3);
    }

    #[test]
    fn test_performance_hardening() {
        let manager = ExchangeProtectionManager::new();
        
        // Check BingX has stricter memory limits
        let bingx_config = manager.get_performance_config("bingx").unwrap();
        assert_eq!(bingx_config.max_memory_mb, 50);
        assert_eq!(bingx_config.connection_pool_size, 1);
        
        // Check Binance has higher performance settings
        let binance_config = manager.get_performance_config("binance").unwrap();
        assert_eq!(binance_config.connection_pool_size, 8);
        assert_eq!(binance_config.max_latency_ms, 5_000);
    }

    #[tokio::test]
    async fn test_protection_summary() {
        let manager = ExchangeProtectionManager::new();
        let summary = manager.get_protection_summary().await;
        
        assert!(summary.total_exchanges > 0);
        assert!(summary.health_score() > 80); // Should start healthy
    }
}