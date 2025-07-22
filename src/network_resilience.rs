//! Network Resilience Patterns for Real Exchange Connectivity
//!
//! Implements robust network handling patterns for production exchange connections:
//! - Exponential backoff with jitter for reconnection
//! - Circuit breaker pattern to prevent cascade failures
//! - Automatic failover to backup endpoints
//! - Connection health monitoring and recovery

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    Closed,     // Normal operation
    Open,       // Failing, reject requests
    HalfOpen,   // Testing if service recovered
}

/// Circuit breaker implementation
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    
    /// Number of successes in half-open state before closing
    pub success_threshold: u32,
    
    /// Time to wait before transitioning from open to half-open
    pub timeout: Duration,
    
    /// Time window for counting failures
    pub failure_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            failure_window: Duration::from_secs(60),
        }
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure_time: Arc::new(RwLock::new(None)),
            config,
        }
    }
    
    /// Check if circuit allows request
    pub async fn allow_request(&self) -> bool {
        let state = self.state.read().await;
        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                drop(state);
                self.check_recovery().await;
                false
            }
            CircuitState::HalfOpen => true,
        }
    }
    
    /// Record successful operation
    pub async fn record_success(&self) {
        let state = self.state.read().await;
        match *state {
            CircuitState::HalfOpen => {
                let count = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                if count >= self.config.success_threshold {
                    drop(state);
                    self.close_circuit().await;
                }
            }
            _ => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Relaxed);
            }
        }
    }
    
    /// Record failed operation
    pub async fn record_failure(&self) {
        let mut last_failure = self.last_failure_time.write().await;
        *last_failure = Some(Instant::now());
        drop(last_failure);
        
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        
        let state = self.state.read().await;
        match *state {
            CircuitState::Closed => {
                if count >= self.config.failure_threshold {
                    drop(state);
                    self.open_circuit().await;
                }
            }
            CircuitState::HalfOpen => {
                drop(state);
                self.open_circuit().await;
            }
            _ => {}
        }
    }
    
    async fn open_circuit(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Open;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        warn!("⚡ Circuit breaker OPENED due to failures");
    }
    
    async fn close_circuit(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        info!("✅ Circuit breaker CLOSED - service recovered");
    }
    
    async fn check_recovery(&self) {
        let last_failure = self.last_failure_time.read().await;
        if let Some(failure_time) = *last_failure {
            if failure_time.elapsed() >= self.config.timeout {
                drop(last_failure);
                let mut state = self.state.write().await;
                if *state == CircuitState::Open {
                    *state = CircuitState::HalfOpen;
                    self.success_count.store(0, Ordering::Relaxed);
                    info!("🔄 Circuit breaker transitioned to HALF-OPEN for testing");
                }
            }
        }
    }
    
    pub async fn get_state(&self) -> CircuitState {
        self.state.read().await.clone()
    }
}

/// Exponential backoff with jitter
pub struct ExponentialBackoff {
    base_delay: Duration,
    max_delay: Duration,
    current_delay: Arc<Mutex<Duration>>,
    attempt: AtomicU32,
    jitter_factor: f64,
}

impl ExponentialBackoff {
    pub fn new(base_delay: Duration, max_delay: Duration, jitter_factor: f64) -> Self {
        Self {
            base_delay,
            max_delay,
            current_delay: Arc::new(Mutex::new(base_delay)),
            attempt: AtomicU32::new(0),
            jitter_factor,
        }
    }
    
    /// Get next backoff duration
    pub async fn next_backoff(&self) -> Duration {
        let attempt = self.attempt.fetch_add(1, Ordering::Relaxed);
        
        // Calculate exponential delay: base * 2^attempt
        let exponential_delay = self.base_delay
            .saturating_mul(2u32.saturating_pow(attempt))
            .min(self.max_delay);
        
        // Add jitter to prevent thundering herd
        let jitter = rand::random::<f64>() * self.jitter_factor;
        let jittered_delay = exponential_delay.mul_f64(1.0 + jitter);
        
        let mut current = self.current_delay.lock().await;
        *current = jittered_delay;
        
        debug!(
            "Backoff attempt {} - waiting {:?} (with jitter)",
            attempt + 1, jittered_delay
        );
        
        jittered_delay
    }
    
    /// Reset backoff to initial state
    pub fn reset(&self) {
        self.attempt.store(0, Ordering::Relaxed);
    }
    
    /// Get current attempt count
    pub fn attempts(&self) -> u32 {
        self.attempt.load(Ordering::Relaxed)
    }
}

/// Connection failover manager
pub struct FailoverManager {
    endpoints: Vec<String>,
    current_index: AtomicU32,
    endpoint_health: Arc<RwLock<Vec<EndpointHealth>>>,
    health_check_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct EndpointHealth {
    pub url: String,
    pub is_healthy: bool,
    pub last_check: Instant,
    pub consecutive_failures: u32,
    pub average_latency_ms: f64,
}

impl FailoverManager {
    pub fn new(endpoints: Vec<String>, health_check_interval: Duration) -> Self {
        let endpoint_health = endpoints.iter().map(|url| EndpointHealth {
            url: url.clone(),
            is_healthy: true,
            last_check: Instant::now(),
            consecutive_failures: 0,
            average_latency_ms: 0.0,
        }).collect();
        
        Self {
            endpoints,
            current_index: AtomicU32::new(0),
            endpoint_health: Arc::new(RwLock::new(endpoint_health)),
            health_check_interval,
        }
    }
    
    /// Get next available endpoint
    pub async fn get_next_endpoint(&self) -> Option<String> {
        let health = self.endpoint_health.read().await;
        let current = self.current_index.load(Ordering::Relaxed) as usize;
        
        // Try to find a healthy endpoint starting from current
        for i in 0..self.endpoints.len() {
            let index = (current + i) % self.endpoints.len();
            if health[index].is_healthy {
                self.current_index.store(index as u32, Ordering::Relaxed);
                return Some(self.endpoints[index].clone());
            }
        }
        
        // No healthy endpoints, return primary as last resort
        warn!("⚠️ No healthy endpoints available, using primary");
        self.endpoints.first().cloned()
    }
    
    /// Mark endpoint as failed
    pub async fn mark_endpoint_failed(&self, url: &str) {
        let mut health = self.endpoint_health.write().await;
        if let Some(endpoint) = health.iter_mut().find(|e| e.url == url) {
            endpoint.consecutive_failures += 1;
            if endpoint.consecutive_failures >= 3 {
                endpoint.is_healthy = false;
                error!("❌ Endpoint {} marked as unhealthy after {} failures", 
                    url, endpoint.consecutive_failures);
            }
        }
    }
    
    /// Mark endpoint as healthy
    pub async fn mark_endpoint_healthy(&self, url: &str, latency_ms: f64) {
        let mut health = self.endpoint_health.write().await;
        if let Some(endpoint) = health.iter_mut().find(|e| e.url == url) {
            endpoint.is_healthy = true;
            endpoint.consecutive_failures = 0;
            endpoint.last_check = Instant::now();
            
            // Update average latency (exponential moving average)
            if endpoint.average_latency_ms == 0.0 {
                endpoint.average_latency_ms = latency_ms;
            } else {
                endpoint.average_latency_ms = 
                    endpoint.average_latency_ms * 0.9 + latency_ms * 0.1;
            }
            
            debug!("✅ Endpoint {} healthy (latency: {:.2}ms avg)", 
                url, endpoint.average_latency_ms);
        }
    }
    
    /// Start background health monitoring
    pub async fn start_health_monitoring(&self) {
        let health = self.endpoint_health.clone();
        let endpoints = self.endpoints.clone();
        let interval = self.health_check_interval;
        
        tokio::spawn(async move {
            let mut check_interval = tokio::time::interval(interval);
            
            loop {
                check_interval.tick().await;
                
                for endpoint in &endpoints {
                    let health_clone = health.clone();
                    let url = endpoint.clone();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::check_endpoint_health(&url, &health_clone).await {
                            warn!("Health check failed for {}: {}", url, e);
                        }
                    });
                }
            }
        });
    }
    
    async fn check_endpoint_health(
        url: &str,
        health: &Arc<RwLock<Vec<EndpointHealth>>>
    ) -> Result<()> {
        // Simulate health check - in production, this would ping the endpoint
        let start = Instant::now();
        
        // Try to connect with timeout
        let check_result = timeout(
            Duration::from_secs(5),
            Self::perform_health_check(url)
        ).await;
        
        let latency_ms = start.elapsed().as_millis() as f64;
        
        match check_result {
            Ok(Ok(())) => {
                // Health check passed
                let mut health_guard = health.write().await;
                if let Some(endpoint) = health_guard.iter_mut().find(|e| e.url == url) {
                    endpoint.is_healthy = true;
                    endpoint.consecutive_failures = 0;
                    endpoint.last_check = Instant::now();
                    endpoint.average_latency_ms = 
                        endpoint.average_latency_ms * 0.9 + latency_ms * 0.1;
                }
            }
            Ok(Err(_)) | Err(_) => {
                // Health check failed
                let mut health_guard = health.write().await;
                if let Some(endpoint) = health_guard.iter_mut().find(|e| e.url == url) {
                    endpoint.consecutive_failures += 1;
                    if endpoint.consecutive_failures >= 3 {
                        endpoint.is_healthy = false;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    async fn perform_health_check(url: &str) -> Result<()> {
        // In production, this would actually connect to the WebSocket
        // For now, simulate with a simple check
        if url.starts_with("wss://") || url.starts_with("ws://") {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Invalid WebSocket URL"))
        }
    }
    
    /// Get health status of all endpoints
    pub async fn get_health_status(&self) -> Vec<EndpointHealth> {
        self.endpoint_health.read().await.clone()
    }
}

/// Resilient WebSocket connection wrapper
pub struct ResilientWebSocketConnection {
    pub exchange: String,
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub backoff: Arc<ExponentialBackoff>,
    pub failover: Arc<FailoverManager>,
    pub metrics: Arc<ConnectionMetrics>,
}

#[derive(Debug, Default)]
pub struct ConnectionMetrics {
    pub connection_attempts: AtomicU64,
    pub successful_connections: AtomicU64,
    pub failed_connections: AtomicU64,
    pub messages_sent: AtomicU64,
    pub messages_received: AtomicU64,
    pub total_downtime_ms: AtomicU64,
}

impl ResilientWebSocketConnection {
    pub fn new(
        exchange: String,
        endpoints: Vec<String>,
    ) -> Self {
        let circuit_breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default()));
        let backoff = Arc::new(ExponentialBackoff::new(
            Duration::from_millis(100),  // Base delay: 100ms
            Duration::from_secs(30),      // Max delay: 30s
            0.3,                          // 30% jitter
        ));
        let failover = Arc::new(FailoverManager::new(
            endpoints,
            Duration::from_secs(30),
        ));
        
        Self {
            exchange,
            circuit_breaker,
            backoff,
            failover,
            metrics: Arc::new(ConnectionMetrics::default()),
        }
    }
    
    /// Connect with full resilience patterns
    pub async fn connect(&self) -> Result<()> {
        loop {
            // Check circuit breaker
            if !self.circuit_breaker.allow_request().await {
                warn!("Circuit breaker is OPEN for {} - waiting...", self.exchange);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
            
            // Get next endpoint from failover manager
            let endpoint = match self.failover.get_next_endpoint().await {
                Some(url) => url,
                None => {
                    error!("No available endpoints for {}", self.exchange);
                    return Err(anyhow::anyhow!("No available endpoints"));
                }
            };
            
            info!("🔗 Attempting connection to {} ({})", self.exchange, endpoint);
            self.metrics.connection_attempts.fetch_add(1, Ordering::Relaxed);
            
            let start = Instant::now();
            match self.try_connect(&endpoint).await {
                Ok(()) => {
                    info!("✅ Successfully connected to {} in {:?}", 
                        self.exchange, start.elapsed());
                    
                    self.metrics.successful_connections.fetch_add(1, Ordering::Relaxed);
                    self.circuit_breaker.record_success().await;
                    self.backoff.reset();
                    self.failover.mark_endpoint_healthy(
                        &endpoint, 
                        start.elapsed().as_millis() as f64
                    ).await;
                    
                    return Ok(());
                }
                Err(e) => {
                    error!("❌ Failed to connect to {}: {}", endpoint, e);
                    
                    self.metrics.failed_connections.fetch_add(1, Ordering::Relaxed);
                    self.circuit_breaker.record_failure().await;
                    self.failover.mark_endpoint_failed(&endpoint).await;
                    
                    // Apply exponential backoff
                    let backoff_duration = self.backoff.next_backoff().await;
                    warn!("⏳ Waiting {:?} before retry (attempt {})", 
                        backoff_duration, self.backoff.attempts());
                    
                    sleep(backoff_duration).await;
                }
            }
        }
    }
    
    async fn try_connect(&self, endpoint: &str) -> Result<()> {
        // In production, this would establish actual WebSocket connection
        // For now, simulate connection attempt
        
        // Simulate network latency
        let latency = Duration::from_millis(rand::random::<u64>() % 50);
        sleep(latency).await;
        
        // Simulate 90% success rate for testing
        if rand::random::<f32>() > 0.1 {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Simulated connection failure"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_circuit_breaker() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            failure_window: Duration::from_secs(60),
        });
        
        // Initial state should be closed
        assert_eq!(cb.get_state().await, CircuitState::Closed);
        assert!(cb.allow_request().await);
        
        // Record failures to open circuit
        for _ in 0..3 {
            cb.record_failure().await;
        }
        
        assert_eq!(cb.get_state().await, CircuitState::Open);
        assert!(!cb.allow_request().await);
        
        // Wait for timeout to transition to half-open
        sleep(Duration::from_millis(150)).await;
        cb.check_recovery().await;
        
        // Circuit should now be half-open
        assert!(cb.allow_request().await);
        
        // Record successes to close circuit
        for _ in 0..2 {
            cb.record_success().await;
        }
        
        assert_eq!(cb.get_state().await, CircuitState::Closed);
    }
    
    #[tokio::test]
    async fn test_exponential_backoff() {
        let backoff = ExponentialBackoff::new(
            Duration::from_millis(10),
            Duration::from_millis(1000),
            0.0, // No jitter for predictable testing
        );
        
        // First attempt: 10ms
        let delay1 = backoff.next_backoff().await;
        assert_eq!(delay1, Duration::from_millis(10));
        
        // Second attempt: 20ms
        let delay2 = backoff.next_backoff().await;
        assert_eq!(delay2, Duration::from_millis(20));
        
        // Third attempt: 40ms
        let delay3 = backoff.next_backoff().await;
        assert_eq!(delay3, Duration::from_millis(40));
        
        // Reset should go back to base delay
        backoff.reset();
        let delay4 = backoff.next_backoff().await;
        assert_eq!(delay4, Duration::from_millis(10));
    }
    
    #[tokio::test]
    async fn test_failover_manager() {
        let endpoints = vec![
            "wss://primary.example.com".to_string(),
            "wss://backup1.example.com".to_string(),
            "wss://backup2.example.com".to_string(),
        ];
        
        let failover = FailoverManager::new(endpoints.clone(), Duration::from_secs(30));
        
        // Should start with primary
        let endpoint = failover.get_next_endpoint().await.unwrap();
        assert_eq!(endpoint, "wss://primary.example.com");
        
        // Mark primary as failed
        for _ in 0..3 {
            failover.mark_endpoint_failed(&endpoints[0]).await;
        }
        
        // Should switch to backup1
        let endpoint = failover.get_next_endpoint().await.unwrap();
        assert_eq!(endpoint, "wss://backup1.example.com");
        
        // Check health status
        let health = failover.get_health_status().await;
        assert!(!health[0].is_healthy);
        assert!(health[1].is_healthy);
        assert!(health[2].is_healthy);
    }
}