use std::time::{Duration, Instant};
use tracing::warn;

/// Simple circuit breaker for repeated connection failures.
#[derive(Debug)]
pub struct CircuitBreaker {
    failures: u8,
    threshold: u8,
    open_since: Option<Instant>,
    open_interval: Duration,
}

impl CircuitBreaker {
    /// Construct a new [`CircuitBreaker`]. When `threshold` consecutive failures
    /// occur, the breaker will open for `open_interval` duration.
    pub fn new(threshold: u8, open_interval: Duration) -> Self {
        Self {
            failures: 0,
            threshold,
            open_since: None,
            open_interval,
        }
    }

    /// Record a failure and open the breaker if the threshold has been reached.
    pub fn record_failure(&mut self) {
        if self.is_open() {
            return;
        }
        self.failures = self.failures.saturating_add(1);
        if self.failures >= self.threshold {
            self.open_since = Some(Instant::now());
            warn!("WebSocket circuit breaker opened after {} failures", self.failures);
        }
    }

    /// Reset the breaker to the closed state.
    pub fn reset(&mut self) {
        self.failures = 0;
        self.open_since = None;
    }

    /// Returns true if the breaker is currently open.
    pub fn is_open(&self) -> bool {
        match self.open_since {
            Some(t) => t.elapsed() < self.open_interval,
            None => false,
        }
    }

    /// Remaining time before the breaker closes.
    pub fn remaining(&self) -> Option<Duration> {
        self.open_since.map(|t| {
            if t.elapsed() >= self.open_interval {
                Duration::from_millis(0)
            } else {
                self.open_interval - t.elapsed()
            }
        })
    }
}
