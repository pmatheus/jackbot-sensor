//! Circuit breaker and rate limiting module
//!
//! This module implements advanced circuit breaker patterns and rate limiting
//! to protect exchange connections and prevent system overload.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio::time::{interval, sleep};
use tracing::{debug, info, warn, error};

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    Closed,    // Normal operation
    Open,      // Failing - reject all requests
    HalfOpen,  // Testing - allow limited requests
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    pub failure_threshold: u32,
    /// Success threshold to close circuit from half-open
    pub success_threshold: u32,
    /// Time window for failure counting (seconds)
    pub window_size_seconds: u64,
    /// Timeout before transitioning to half-open (seconds)
    pub timeout_seconds: u64,
    /// Maximum requests in half-open state
    pub half_open_max_requests: u32,
    /// Minimum requests before considering failure rate
    pub min_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            window_size_seconds: 60,
            timeout_seconds: 30,
            half_open_max_requests: 3,
            min_requests: 10,
        }
    }
}

/// Rate limiter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiterConfig {
    /// Maximum requests per second
    pub max_requests_per_second: u32,
    /// Burst capacity
    pub burst_capacity: u32,
    /// Token bucket refill rate
    pub refill_rate: u32,
    /// Maximum queue size for waiting requests
    pub max_queue_size: usize,
    /// Request timeout (milliseconds)
    pub request_timeout_ms: u64,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            max_requests_per_second: 100,
            burst_capacity: 200,
            refill_rate: 100,
            max_queue_size: 1000,
            request_timeout_ms: 5000,
        }
    }
}

/// Request execution result
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub duration: Duration,
    pub error_message: Option<String>,
    pub timestamp: Instant,
}

/// Circuit breaker statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub rejected_requests: u64,
    pub failure_rate_pct: f64,
    pub average_response_time_ms: u64,
    pub last_failure_time: Option<u64>,
    pub state_transition_count: u64,
}

/// Rate limiter statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiterStats {
    pub current_tokens: u32,
    pub total_requests: u64,
    pub allowed_requests: u64,
    pub rejected_requests: u64,
    pub queued_requests: u64,
    pub average_wait_time_ms: u64,
    pub tokens_per_second: f64,
}

/// Circuit breaker implementation
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitState>>,
    stats: Arc<RwLock<CircuitBreakerStats>>,
    request_history: Arc<Mutex<VecDeque<ExecutionResult>>>,
    half_open_requests: Arc<AtomicUsize>,
    last_state_change: Arc<Mutex<Instant>>,
    state_transitions: Arc<AtomicU64>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        let stats = CircuitBreakerStats {
            state: CircuitState::Closed,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            rejected_requests: 0,
            failure_rate_pct: 0.0,
            average_response_time_ms: 0,
            last_failure_time: None,
            state_transition_count: 0,
        };

        let circuit_breaker = Self {
            config,
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            stats: Arc::new(RwLock::new(stats)),
            request_history: Arc::new(Mutex::new(VecDeque::new())),
            half_open_requests: Arc::new(AtomicUsize::new(0)),
            last_state_change: Arc::new(Mutex::new(Instant::now())),
            state_transitions: Arc::new(AtomicU64::new(0)),
        };

        // Start monitoring task
        circuit_breaker.start_monitoring();
        circuit_breaker
    }

    /// Execute a request through the circuit breaker
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        // Check if request should be allowed
        if !self.should_allow_request().await {
            self.record_rejection().await;
            return Err(anyhow::anyhow!("Circuit breaker is open - request rejected"));
        }

        let start_time = Instant::now();
        
        // Execute the operation
        let result = operation().await;
        
        let duration = start_time.elapsed();
        
        // Record the result
        match &result {
            Ok(_) => self.record_success(duration).await,
            Err(e) => self.record_failure(duration, e.to_string()).await,
        }

        result
    }

    /// Check current circuit breaker state
    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }

    /// Get circuit breaker statistics
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        self.stats.read().await.clone()
    }

    /// Force circuit breaker to specific state (for testing)
    pub async fn force_state(&self, new_state: CircuitState) {
        let mut state = self.state.write().await;
        if *state != new_state {
            info!("Circuit breaker manually forced to state: {:?}", new_state);
            *state = new_state;
            self.state_transitions.fetch_add(1, Ordering::Relaxed);
            
            let mut last_change = self.last_state_change.lock().await;
            *last_change = Instant::now();
        }
    }

    /// Check if request should be allowed
    async fn should_allow_request(&self) -> bool {
        let state = *self.state.read().await;
        
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                let last_change = self.last_state_change.lock().await;
                if last_change.elapsed() >= Duration::from_secs(self.config.timeout_seconds) {
                    drop(last_change);
                    self.transition_to_half_open().await;
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                let current_requests = self.half_open_requests.load(Ordering::Relaxed);
                current_requests < self.config.half_open_max_requests as usize
            }
        }
    }

    /// Record successful request
    async fn record_success(&self, duration: Duration) {
        let mut stats = self.stats.write().await;
        stats.total_requests += 1;
        stats.successful_requests += 1;
        
        // Update average response time
        let total_time = stats.average_response_time_ms * (stats.total_requests - 1) + duration.as_millis() as u64;
        stats.average_response_time_ms = total_time / stats.total_requests;

        // Add to history
        let result = ExecutionResult {
            success: true,
            duration,
            error_message: None,
            timestamp: Instant::now(),
        };
        
        let mut history = self.request_history.lock().await;
        history.push_back(result);
        self.cleanup_old_history(&mut history).await;
        
        drop(history);
        drop(stats);

        // Check for state transitions in half-open state
        let state = *self.state.read().await;
        if state == CircuitState::HalfOpen {
            let consecutive_successes = self.count_consecutive_successes().await;
            if consecutive_successes >= self.config.success_threshold {
                self.transition_to_closed().await;
            }
        }
    }

    /// Record failed request
    async fn record_failure(&self, duration: Duration, error: String) {
        let mut stats = self.stats.write().await;
        stats.total_requests += 1;
        stats.failed_requests += 1;
        stats.last_failure_time = Some(Instant::now().elapsed().as_secs());
        
        // Update average response time
        let total_time = stats.average_response_time_ms * (stats.total_requests - 1) + duration.as_millis() as u64;
        stats.average_response_time_ms = total_time / stats.total_requests;

        // Add to history
        let result = ExecutionResult {
            success: false,
            duration,
            error_message: Some(error),
            timestamp: Instant::now(),
        };
        
        let mut history = self.request_history.lock().await;
        history.push_back(result);
        self.cleanup_old_history(&mut history).await;
        
        drop(history);
        drop(stats);

        // Check if we should open the circuit
        self.check_failure_threshold().await;
    }

    /// Record rejected request
    async fn record_rejection(&self) {
        let mut stats = self.stats.write().await;
        stats.rejected_requests += 1;
    }

    /// Check if failure threshold is exceeded
    async fn check_failure_threshold(&self) {
        let history = self.request_history.lock().await;
        let recent_requests: Vec<&ExecutionResult> = history
            .iter()
            .rev()
            .take_while(|r| r.timestamp.elapsed() <= Duration::from_secs(self.config.window_size_seconds))
            .collect();

        if recent_requests.len() >= self.config.min_requests as usize {
            let failures = recent_requests.iter().filter(|r| !r.success).count();
            let failure_rate = failures as f64 / recent_requests.len() as f64;
            
            // Update failure rate in stats
            {
                let mut stats = self.stats.write().await;
                stats.failure_rate_pct = failure_rate * 100.0;
            }

            if failures >= self.config.failure_threshold as usize {
                drop(history);
                self.transition_to_open().await;
            }
        }
    }

    /// Transition to open state
    async fn transition_to_open(&self) {
        let mut state = self.state.write().await;
        if *state != CircuitState::Open {
            warn!("Circuit breaker opening due to failure threshold exceeded");
            *state = CircuitState::Open;
            self.state_transitions.fetch_add(1, Ordering::Relaxed);
            
            let mut last_change = self.last_state_change.lock().await;
            *last_change = Instant::now();
            
            let mut stats = self.stats.write().await;
            stats.state = CircuitState::Open;
            stats.state_transition_count += 1;
        }
    }

    /// Transition to half-open state
    async fn transition_to_half_open(&self) {
        let mut state = self.state.write().await;
        if *state != CircuitState::HalfOpen {
            info!("Circuit breaker transitioning to half-open state");
            *state = CircuitState::HalfOpen;
            self.half_open_requests.store(0, Ordering::Relaxed);
            self.state_transitions.fetch_add(1, Ordering::Relaxed);
            
            let mut last_change = self.last_state_change.lock().await;
            *last_change = Instant::now();
            
            let mut stats = self.stats.write().await;
            stats.state = CircuitState::HalfOpen;
            stats.state_transition_count += 1;
        }
    }

    /// Transition to closed state
    async fn transition_to_closed(&self) {
        let mut state = self.state.write().await;
        if *state != CircuitState::Closed {
            info!("Circuit breaker closing - service recovered");
            *state = CircuitState::Closed;
            self.state_transitions.fetch_add(1, Ordering::Relaxed);
            
            let mut last_change = self.last_state_change.lock().await;
            *last_change = Instant::now();
            
            let mut stats = self.stats.write().await;
            stats.state = CircuitState::Closed;
            stats.state_transition_count += 1;
        }
    }

    /// Count consecutive successes in half-open state
    async fn count_consecutive_successes(&self) -> u32 {
        let history = self.request_history.lock().await;
        let mut count = 0;
        
        for result in history.iter().rev() {
            if result.success {
                count += 1;
            } else {
                break;
            }
        }
        
        count
    }

    /// Clean up old history entries
    async fn cleanup_old_history(&self, history: &mut VecDeque<ExecutionResult>) {
        let cutoff = Instant::now() - Duration::from_secs(self.config.window_size_seconds * 2);
        while let Some(front) = history.front() {
            if front.timestamp < cutoff {
                history.pop_front();
            } else {
                break;
            }
        }
    }

    /// Start monitoring task
    fn start_monitoring(&self) {
        let state = Arc::clone(&self.state);
        let stats = Arc::clone(&self.stats);
        let request_history = Arc::clone(&self.request_history);
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                // Update statistics
                let history = request_history.lock().await;
                let current_state = *state.read().await;
                
                let recent_requests: Vec<&ExecutionResult> = history
                    .iter()
                    .rev()
                    .take_while(|r| r.timestamp.elapsed() <= Duration::from_secs(config.window_size_seconds))
                    .collect();

                if !recent_requests.is_empty() {
                    let failures = recent_requests.iter().filter(|r| !r.success).count();
                    let failure_rate = failures as f64 / recent_requests.len() as f64;
                    
                    let mut stats = stats.write().await;
                    stats.state = current_state;
                    stats.failure_rate_pct = failure_rate * 100.0;
                }

                drop(history);
            }
        });
    }
}

/// Token bucket rate limiter
pub struct TokenBucketRateLimiter {
    config: RateLimiterConfig,
    tokens: Arc<AtomicU64>,
    last_refill: Arc<Mutex<Instant>>,
    stats: Arc<RwLock<RateLimiterStats>>,
    request_semaphore: Arc<Semaphore>,
}

impl TokenBucketRateLimiter {
    /// Create a new token bucket rate limiter
    pub fn new(config: RateLimiterConfig) -> Self {
        let stats = RateLimiterStats {
            current_tokens: config.burst_capacity,
            total_requests: 0,
            allowed_requests: 0,
            rejected_requests: 0,
            queued_requests: 0,
            average_wait_time_ms: 0,
            tokens_per_second: config.max_requests_per_second as f64,
        };

        let limiter = Self {
            tokens: Arc::new(AtomicU64::new(config.burst_capacity as u64)),
            config,
            last_refill: Arc::new(Mutex::new(Instant::now())),
            stats: Arc::new(RwLock::new(stats)),
            request_semaphore: Arc::new(Semaphore::new(1000)),
        };

        // Start token refill task
        limiter.start_refill_task();
        limiter
    }

    /// Acquire permission to make a request
    pub async fn acquire(&self) -> Result<RateLimitPermit> {
        let start_time = Instant::now();
        
        // Check queue size
        {
            let mut stats = self.stats.write().await;
            stats.total_requests += 1;
            
            if stats.queued_requests >= self.config.max_queue_size as u64 {
                stats.rejected_requests += 1;
                return Err(anyhow::anyhow!("Rate limiter queue full"));
            }
            
            stats.queued_requests += 1;
        }

        // Wait for semaphore (queue management)
        let _permit = self.request_semaphore.clone().acquire_owned().await
            .context("Failed to acquire semaphore")?;

        // Wait for token with timeout
        let timeout = Duration::from_millis(self.config.request_timeout_ms);
        let acquire_result = tokio::time::timeout(timeout, self.wait_for_token()).await;

        let wait_time = start_time.elapsed();
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.queued_requests = stats.queued_requests.saturating_sub(1);
            
            match acquire_result {
                Ok(Ok(())) => {
                    stats.allowed_requests += 1;
                    // Update average wait time
                    let total_wait = stats.average_wait_time_ms * (stats.allowed_requests - 1) + wait_time.as_millis() as u64;
                    stats.average_wait_time_ms = total_wait / stats.allowed_requests;
                    
                    Ok(RateLimitPermit {
                        _permit,
                        acquired_at: Instant::now(),
                    })
                }
                Ok(Err(e)) => {
                    stats.rejected_requests += 1;
                    Err(e)
                }
                Err(_) => {
                    stats.rejected_requests += 1;
                    Err(anyhow::anyhow!("Rate limit acquisition timeout"))
                }
            }
        }
    }

    /// Get current rate limiter statistics
    pub async fn get_stats(&self) -> RateLimiterStats {
        let mut stats = self.stats.write().await;
        stats.current_tokens = self.tokens.load(Ordering::Relaxed) as u32;
        stats.clone()
    }

    /// Wait for token availability
    async fn wait_for_token(&self) -> Result<()> {
        loop {
            self.refill_tokens().await;
            
            let current_tokens = self.tokens.load(Ordering::Relaxed);
            if current_tokens > 0 {
                // Try to consume a token
                let new_tokens = current_tokens - 1;
                if self.tokens.compare_exchange(current_tokens, new_tokens, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                    debug!("Token acquired, {} tokens remaining", new_tokens);
                    return Ok(());
                }
            }
            
            // Wait a bit before trying again
            sleep(Duration::from_millis(1)).await;
        }
    }

    /// Refill tokens based on elapsed time
    async fn refill_tokens(&self) {
        let mut last_refill = self.last_refill.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(*last_refill);
        
        if elapsed >= Duration::from_millis(10) { // Refill every 10ms minimum
            let tokens_to_add = (elapsed.as_secs_f64() * self.config.refill_rate as f64) as u64;
            
            if tokens_to_add > 0 {
                let current_tokens = self.tokens.load(Ordering::Relaxed);
                let new_tokens = (current_tokens + tokens_to_add).min(self.config.burst_capacity as u64);
                self.tokens.store(new_tokens, Ordering::Relaxed);
                *last_refill = now;
                
                debug!("Refilled {} tokens, total: {}", tokens_to_add, new_tokens);
            }
        }
    }

    /// Start token refill background task
    fn start_refill_task(&self) {
        let tokens = Arc::clone(&self.tokens);
        let last_refill = Arc::clone(&self.last_refill);
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100)); // Refill every 100ms
            
            loop {
                interval.tick().await;
                
                let mut last_refill_guard = last_refill.lock().await;
                let now = Instant::now();
                let elapsed = now.duration_since(*last_refill_guard);
                
                let tokens_to_add = (elapsed.as_secs_f64() * config.refill_rate as f64) as u64;
                
                if tokens_to_add > 0 {
                    let current_tokens = tokens.load(Ordering::Relaxed);
                    let new_tokens = (current_tokens + tokens_to_add).min(config.burst_capacity as u64);
                    tokens.store(new_tokens, Ordering::Relaxed);
                    *last_refill_guard = now;
                }
            }
        });
    }
}

/// Rate limit permit - allows one request
pub struct RateLimitPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    acquired_at: Instant,
}

impl RateLimitPermit {
    /// Get the time this permit was acquired
    pub fn acquired_at(&self) -> Instant {
        self.acquired_at
    }
}

/// Combined circuit breaker and rate limiter
pub struct ProtectedClient {
    circuit_breaker: Arc<CircuitBreaker>,
    rate_limiter: Arc<TokenBucketRateLimiter>,
    name: String,
}

impl ProtectedClient {
    /// Create a new protected client
    pub fn new(
        name: String,
        circuit_config: CircuitBreakerConfig,
        rate_config: RateLimiterConfig,
    ) -> Self {
        Self {
            circuit_breaker: Arc::new(CircuitBreaker::new(circuit_config)),
            rate_limiter: Arc::new(TokenBucketRateLimiter::new(rate_config)),
            name,
        }
    }

    /// Execute a protected operation
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        // Acquire rate limit permission
        let _permit = self.rate_limiter.acquire().await
            .context("Rate limit exceeded")?;

        // Execute through circuit breaker
        self.circuit_breaker.execute(operation).await
    }

    /// Get protection statistics
    pub async fn get_protection_stats(&self) -> (CircuitBreakerStats, RateLimiterStats) {
        let cb_stats = self.circuit_breaker.get_stats().await;
        let rl_stats = self.rate_limiter.get_stats().await;
        (cb_stats, rl_stats)
    }

    /// Get client name
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    async fn failing_operation() -> Result<String> {
        Err(anyhow::anyhow!("Simulated failure"))
    }

    async fn successful_operation() -> Result<String> {
        Ok("Success".to_string())
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            min_requests: 3,
            ..Default::default()
        };
        
        let cb = CircuitBreaker::new(config);
        
        // Trigger failures to open circuit
        for _ in 0..5 {
            let _ = cb.execute(failing_operation).await;
        }
        
        // Circuit should be open now
        assert_eq!(cb.get_state().await, CircuitState::Open);
        
        // New requests should be rejected
        let result = cb.execute(successful_operation).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_limits_requests() {
        let config = RateLimiterConfig {
            max_requests_per_second: 5,
            burst_capacity: 5,
            refill_rate: 1,
            ..Default::default()
        };
        
        let limiter = TokenBucketRateLimiter::new(config);
        
        // Should be able to get 5 permits immediately (burst)
        for _ in 0..5 {
            let permit = limiter.acquire().await;
            assert!(permit.is_ok());
        }
        
        // 6th request should be rate limited
        let start = Instant::now();
        let permit = limiter.acquire().await;
        let elapsed = start.elapsed();
        
        assert!(permit.is_ok());
        assert!(elapsed >= Duration::from_millis(100)); // Should wait for refill
    }

    #[tokio::test]
    async fn test_protected_client_integration() {
        let cb_config = CircuitBreakerConfig {
            failure_threshold: 2,
            min_requests: 2,
            ..Default::default()
        };
        
        let rl_config = RateLimiterConfig {
            max_requests_per_second: 10,
            burst_capacity: 5,
            ..Default::default()
        };
        
        let client = ProtectedClient::new("test".to_string(), cb_config, rl_config);
        
        // Should allow successful operations
        let result = client.execute(successful_operation).await;
        assert!(result.is_ok());
        
        // Trigger circuit breaker
        for _ in 0..3 {
            let _ = client.execute(failing_operation).await;
        }
        
        // Should be blocked by circuit breaker
        let result = client.execute(successful_operation).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Circuit breaker"));
    }
}