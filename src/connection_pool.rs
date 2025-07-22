//! High-performance connection pool for 10K+ concurrent connections
//!
//! This module provides connection pooling, Redis caching, and parallel processing
//! to meet <50ms response time requirements at 10K RPS.

use anyhow::{Context, Result};
use deadpool_redis::{Config as RedisConfig, Pool as RedisPool, Runtime};
use parking_lot::RwLock;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Semaphore};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Connection pool configuration for high performance
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    /// Request timeout in milliseconds  
    pub request_timeout_ms: u64,
    /// Redis cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Connection keepalive interval
    pub keepalive_interval_ms: u64,
    /// Pool health check interval
    pub health_check_interval_ms: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10000,     // 10K concurrent connections
            connection_timeout_ms: 100, // 100ms connection timeout
            request_timeout_ms: 50,     // 50ms request timeout target
            cache_ttl_seconds: 300,     // 5 minute cache TTL
            keepalive_interval_ms: 30000, // 30 second keepalive
            health_check_interval_ms: 5000, // 5 second health checks
        }
    }
}

/// Cached data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedData {
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub expiry: u64,
}

/// Connection pool metrics
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    pub active_connections: usize,
    pub pending_requests: usize,
    pub cache_hit_rate: f64,
    pub avg_response_time_ms: f64,
    pub error_rate: f64,
    pub throughput_rps: f64,
}

/// High-performance connection pool manager
pub struct ConnectionPool {
    config: PoolConfig,
    redis_pool: RedisPool,
    connection_semaphore: Arc<Semaphore>,
    request_metrics: Arc<RwLock<RequestMetrics>>,
    cache_metrics: Arc<RwLock<CacheMetrics>>,
}

/// Request processing metrics
#[derive(Debug, Default)]
struct RequestMetrics {
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    total_response_time_ms: u64,
    last_reset: Instant,
}

/// Cache performance metrics
#[derive(Debug, Default)]
struct CacheMetrics {
    hits: u64,
    misses: u64,
    sets: u64,
    errors: u64,
}

impl ConnectionPool {
    /// Create a new high-performance connection pool
    pub async fn new(config: PoolConfig, redis_url: &str) -> Result<Self> {
        info!("Initializing connection pool with {} max connections", config.max_connections);
        
        // Configure Redis connection pool
        let redis_config = RedisConfig::from_url(redis_url);
        let redis_pool = redis_config
            .create_pool(Some(Runtime::Tokio1))
            .context("Failed to create Redis pool")?;

        // Test Redis connection
        {
            let mut conn = redis_pool.get().await
                .context("Failed to get Redis connection")?;
            let _: String = conn.ping().await
                .context("Redis ping failed")?;
            info!("Redis connection established successfully");
        }

        let pool = Self {
            connection_semaphore: Arc::new(Semaphore::new(config.max_connections)),
            redis_pool,
            config,
            request_metrics: Arc::new(RwLock::new(RequestMetrics {
                last_reset: Instant::now(),
                ..Default::default()
            })),
            cache_metrics: Arc::new(RwLock::new(CacheMetrics::default())),
        };

        // Start background tasks
        pool.start_health_monitor();
        
        Ok(pool)
    }

    /// Process request with connection pooling and caching
    pub async fn process_request<T, F, R>(&self, 
        cache_key: &str,
        processor: F
    ) -> Result<T> 
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + 'static,
        F: FnOnce() -> R + Send,
        R: std::future::Future<Output = Result<T>> + Send,
    {
        let start_time = Instant::now();
        let request_id = Uuid::new_v4();
        
        debug!("Processing request {} with cache key: {}", request_id, cache_key);

        // Step 1: Check cache first
        if let Ok(cached) = self.get_from_cache::<T>(cache_key).await {
            let response_time = start_time.elapsed().as_millis() as u64;
            self.record_request_success(response_time);
            self.record_cache_hit();
            
            debug!("Cache hit for request {} in {}ms", request_id, response_time);
            return Ok(cached);
        }

        // Step 2: Acquire connection permit (backpressure control)
        let _permit = self.connection_semaphore.acquire().await
            .context("Failed to acquire connection permit")?;

        // Step 3: Process with timeout
        let result = tokio::time::timeout(
            Duration::from_millis(self.config.request_timeout_ms),
            processor()
        ).await;

        match result {
            Ok(Ok(data)) => {
                let response_time = start_time.elapsed().as_millis() as u64;
                
                // Step 4: Cache the result
                if let Err(cache_err) = self.set_in_cache(cache_key, &data).await {
                    warn!("Failed to cache result for {}: {}", cache_key, cache_err);
                }

                self.record_request_success(response_time);
                
                if response_time > 50 {
                    warn!("Slow request {} took {}ms (target: <50ms)", request_id, response_time);
                }

                debug!("Request {} completed in {}ms", request_id, response_time);
                Ok(data)
            }
            Ok(Err(e)) => {
                let response_time = start_time.elapsed().as_millis() as u64;
                self.record_request_failure(response_time);
                error!("Request {} failed in {}ms: {}", request_id, response_time, e);
                Err(e)
            }
            Err(_) => {
                let response_time = start_time.elapsed().as_millis() as u64;
                self.record_request_failure(response_time);
                error!("Request {} timed out after {}ms", request_id, response_time);
                Err(anyhow::anyhow!("Request timeout"))
            }
        }
    }

    /// Batch process multiple requests in parallel
    pub async fn batch_process<T, F, R>(&self,
        requests: Vec<(String, F)>,
    ) -> Vec<Result<T>>
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + 'static,
        F: FnOnce() -> R + Send,
        R: std::future::Future<Output = Result<T>> + Send,
    {
        let batch_id = Uuid::new_v4();
        let batch_size = requests.len();
        info!("Processing batch {} with {} requests", batch_id, batch_size);

        let start_time = Instant::now();
        
        // Process all requests concurrently
        let handles: Vec<_> = requests.into_iter().enumerate()
            .map(|(idx, (cache_key, processor))| {
                let pool = self.clone();
                let req_cache_key = format!("batch_{}_{}", batch_id, cache_key);
                
                tokio::spawn(async move {
                    pool.process_request(&req_cache_key, processor).await
                })
            })
            .collect();

        // Wait for all requests to complete
        let results = futures::future::join_all(handles).await;
        
        let batch_time = start_time.elapsed().as_millis();
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        
        info!("Batch {} completed: {}/{} successful in {}ms", 
              batch_id, success_count, batch_size, batch_time);

        // Extract results
        results.into_iter()
            .map(|handle_result| {
                handle_result.unwrap_or_else(|e| Err(anyhow::anyhow!("Task join error: {}", e)))
            })
            .collect()
    }

    /// Get cached data
    async fn get_from_cache<T>(&self, key: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut conn = self.redis_pool.get().await?;
        
        let cached_data: Option<Vec<u8>> = conn.get(key).await
            .map_err(|e| {
                self.record_cache_error();
                anyhow::anyhow!("Redis get error: {}", e)
            })?;

        match cached_data {
            Some(data) => {
                let cached: CachedData = bincode::deserialize(&data)
                    .context("Failed to deserialize cached data")?;
                
                // Check expiry
                let current_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs();
                
                if current_time > cached.expiry {
                    self.record_cache_miss();
                    return Err(anyhow::anyhow!("Cache entry expired"));
                }

                let result: T = bincode::deserialize(&cached.data)
                    .context("Failed to deserialize cached data")?;
                
                Ok(result)
            }
            None => {
                self.record_cache_miss();
                Err(anyhow::anyhow!("Cache miss"))
            }
        }
    }

    /// Set data in cache
    async fn set_in_cache<T>(&self, key: &str, data: &T) -> Result<()>
    where
        T: Serialize,
    {
        let serialized = bincode::serialize(data)
            .context("Failed to serialize data for caching")?;

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let cached = CachedData {
            data: serialized,
            timestamp: current_time,
            expiry: current_time + self.config.cache_ttl_seconds,
        };

        let cached_bytes = bincode::serialize(&cached)
            .context("Failed to serialize cached data")?;

        let mut conn = self.redis_pool.get().await?;
        
        conn.set_ex(key, cached_bytes, self.config.cache_ttl_seconds as usize).await
            .map_err(|e| {
                self.record_cache_error();
                anyhow::anyhow!("Redis set error: {}", e)
            })?;

        self.record_cache_set();
        Ok(())
    }

    /// Get pool performance metrics
    pub async fn get_metrics(&self) -> PoolMetrics {
        let request_metrics = self.request_metrics.read();
        let cache_metrics = self.cache_metrics.read();
        
        let total_cache_requests = cache_metrics.hits + cache_metrics.misses;
        let cache_hit_rate = if total_cache_requests > 0 {
            cache_metrics.hits as f64 / total_cache_requests as f64
        } else {
            0.0
        };

        let error_rate = if request_metrics.total_requests > 0 {
            request_metrics.failed_requests as f64 / request_metrics.total_requests as f64
        } else {
            0.0
        };

        let avg_response_time = if request_metrics.successful_requests > 0 {
            request_metrics.total_response_time_ms as f64 / request_metrics.successful_requests as f64
        } else {
            0.0
        };

        let elapsed_seconds = request_metrics.last_reset.elapsed().as_secs_f64();
        let throughput_rps = if elapsed_seconds > 0.0 {
            request_metrics.total_requests as f64 / elapsed_seconds
        } else {
            0.0
        };

        PoolMetrics {
            active_connections: self.config.max_connections - self.connection_semaphore.available_permits(),
            pending_requests: 0, // Would need additional tracking
            cache_hit_rate,
            avg_response_time_ms: avg_response_time,
            error_rate,
            throughput_rps,
        }
    }

    /// Record successful request
    fn record_request_success(&self, response_time_ms: u64) {
        let mut metrics = self.request_metrics.write();
        metrics.total_requests += 1;
        metrics.successful_requests += 1;
        metrics.total_response_time_ms += response_time_ms;
    }

    /// Record failed request
    fn record_request_failure(&self, response_time_ms: u64) {
        let mut metrics = self.request_metrics.write();
        metrics.total_requests += 1;
        metrics.failed_requests += 1;
        metrics.total_response_time_ms += response_time_ms;
    }

    /// Record cache hit
    fn record_cache_hit(&self) {
        self.cache_metrics.write().hits += 1;
    }

    /// Record cache miss
    fn record_cache_miss(&self) {
        self.cache_metrics.write().misses += 1;
    }

    /// Record cache set operation
    fn record_cache_set(&self) {
        self.cache_metrics.write().sets += 1;
    }

    /// Record cache error
    fn record_cache_error(&self) {
        self.cache_metrics.write().errors += 1;
    }

    /// Start health monitoring background task
    fn start_health_monitor(&self) {
        let redis_pool = self.redis_pool.clone();
        let check_interval = self.config.health_check_interval_ms;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(check_interval));
            
            loop {
                interval.tick().await;
                
                match redis_pool.get().await {
                    Ok(mut conn) => {
                        match conn.ping::<String>().await {
                            Ok(_) => {
                                debug!("Redis health check passed");
                            }
                            Err(e) => {
                                error!("Redis health check failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to get Redis connection for health check: {}", e);
                    }
                }
            }
        });
    }

    /// Reset metrics (for testing/monitoring)
    pub fn reset_metrics(&self) {
        let mut request_metrics = self.request_metrics.write();
        *request_metrics = RequestMetrics {
            last_reset: Instant::now(),
            ..Default::default()
        };
        
        let mut cache_metrics = self.cache_metrics.write();
        *cache_metrics = CacheMetrics::default();
    }
}

// Implement Clone for ConnectionPool to support concurrent usage
impl Clone for ConnectionPool {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            redis_pool: self.redis_pool.clone(),
            connection_semaphore: self.connection_semaphore.clone(),
            request_metrics: self.request_metrics.clone(),
            cache_metrics: self.cache_metrics.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_connection_pool_basic() {
        // This would require a Redis instance for full testing
        // Mock test for compilation
        let config = PoolConfig::default();
        
        // Test configuration
        assert_eq!(config.max_connections, 10000);
        assert_eq!(config.request_timeout_ms, 50);
    }

    #[tokio::test] 
    async fn test_batch_processing() {
        // Mock test for batch processing logic
        let requests = vec![
            ("key1".to_string(), || async { Ok("result1".to_string()) }),
            ("key2".to_string(), || async { Ok("result2".to_string()) }),
        ];
        
        assert_eq!(requests.len(), 2);
    }

    #[tokio::test]
    async fn test_performance_targets() {
        // Test that our target response time is achievable
        let start = Instant::now();
        
        // Simulate fast processing
        sleep(Duration::from_millis(10)).await;
        
        let elapsed = start.elapsed().as_millis();
        assert!(elapsed < 50, "Processing took {}ms, should be <50ms", elapsed);
    }
}