//! Circuit breaker implementation for fault tolerance

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CircuitBreakerError {
    #[error("Circuit breaker is open")]
    Open,
    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
    pub half_open_max_calls: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
        }
    }
}

#[derive(Debug)]
struct CircuitBreakerStats {
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<DateTime<Utc>>,
    half_open_calls: u32,
}

/// Circuit breaker for protecting against cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitBreakerState>>,
    stats: Arc<RwLock<CircuitBreakerStats>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
            stats: Arc::new(RwLock::new(CircuitBreakerStats {
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
                half_open_calls: 0,
            })),
            config,
        }
    }

    pub fn state(&self) -> CircuitBreakerState {
        *self.state.read()
    }

    pub async fn call<F, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError>
    where
        F: futures::Future<Output = Result<T, E>>,
        E: std::error::Error + Send + Sync + 'static,
    {
        match self.check_state() {
            CircuitBreakerState::Open => return Err(CircuitBreakerError::Open),
            CircuitBreakerState::HalfOpen => {
                let mut stats = self.stats.write();
                if stats.half_open_calls >= self.config.half_open_max_calls {
                    return Err(CircuitBreakerError::Open);
                }
                stats.half_open_calls += 1;
            }
            CircuitBreakerState::Closed => {}
        }

        match operation.await {
            Ok(result) => {
                self.on_success();
                Ok(result)
            }
            Err(error) => {
                self.on_failure();
                Err(CircuitBreakerError::OperationFailed(error.to_string()))
            }
        }
    }

    fn check_state(&self) -> CircuitBreakerState {
        let state = *self.state.read();
        
        if state == CircuitBreakerState::Open {
            let stats = self.stats.read();
            if let Some(last_failure) = stats.last_failure_time {
                if Utc::now().signed_duration_since(last_failure)
                    > chrono::Duration::from_std(self.config.recovery_timeout).unwrap_or_default()
                {
                    drop(stats);
                    *self.state.write() = CircuitBreakerState::HalfOpen;
                    let mut stats = self.stats.write();
                    stats.half_open_calls = 0;
                    return CircuitBreakerState::HalfOpen;
                }
            }
        }
        
        state
    }

    fn on_success(&self) {
        let mut stats = self.stats.write();
        stats.success_count += 1;
        stats.failure_count = 0;
        
        if *self.state.read() == CircuitBreakerState::HalfOpen {
            *self.state.write() = CircuitBreakerState::Closed;
            stats.half_open_calls = 0;
        }
    }

    fn on_failure(&self) {
        let mut stats = self.stats.write();
        stats.failure_count += 1;
        stats.last_failure_time = Some(Utc::now());
        
        if stats.failure_count >= self.config.failure_threshold {
            *self.state.write() = CircuitBreakerState::Open;
        }
    }

    /// Check if the circuit breaker is in open state
    pub fn is_open(&self) -> bool {
        matches!(*self.state.read(), CircuitBreakerState::Open)
    }

    /// Get remaining time until circuit breaker can transition to half-open
    pub fn remaining(&self) -> Option<std::time::Duration> {
        if !self.is_open() {
            return None;
        }
        
        let stats = self.stats.read();
        if let Some(last_failure) = stats.last_failure_time {
            let elapsed = Utc::now().signed_duration_since(last_failure);
            let elapsed_std = std::time::Duration::from_millis(elapsed.num_milliseconds() as u64);
            
            if elapsed_std < self.config.recovery_timeout {
                Some(self.config.recovery_timeout - elapsed_std)
            } else {
                Some(std::time::Duration::ZERO)
            }
        } else {
            None
        }
    }

    /// Reset the circuit breaker to closed state
    pub fn reset(&self) {
        *self.state.write() = CircuitBreakerState::Closed;
        let mut stats = self.stats.write();
        stats.failure_count = 0;
        stats.success_count = 0;
        stats.half_open_calls = 0;
        stats.last_failure_time = None;
    }

    /// Record a failure (public interface)
    pub fn record_failure(&self) {
        self.on_failure();
    }
}