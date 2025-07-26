use jackbot_integration::rate_limit::{Priority, RateLimiter};
use std::time::Duration;

/// Coinbase API rate limiter for REST and WebSocket usage.
#[derive(Clone, Debug)]
pub struct CoinbaseRateLimit {
    rest: RateLimiter,
    ws: RateLimiter,
}

impl Default for CoinbaseRateLimit {
    fn default() -> Self {
        Self::new()
    }
}

impl CoinbaseRateLimit {
    /// Create a new [`CoinbaseRateLimit`] using placeholder quotas.
    pub fn new() -> Self {
        Self::with_params(
            600,
            Duration::from_secs(60),
            10,
            Duration::from_secs(1),
            Duration::from_millis(100),
        )
    }

    /// Create a custom [`CoinbaseRateLimit`] with provided quotas and jitter for testing.
    pub fn with_params(
        rest_capacity: usize,
        rest_interval: Duration,
        ws_capacity: usize,
        ws_interval: Duration,
        jitter: Duration,
    ) -> Self {
        Self {
            rest: RateLimiter::new(),
            ws: RateLimiter::new(),
        }
    }

    pub async fn acquire_rest(&self, priority: Priority) {
        self.rest.acquire(priority).await;
    }

    pub async fn acquire_ws(&self, priority: Priority) {
        self.ws.acquire(priority).await;
    }

    pub async fn report_rest_violation(&self) {
        self.rest.report_violation(Priority::High).await;
    }

    pub async fn report_ws_violation(&self) {
        self.ws.report_violation(Priority::High).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackbot_integration::rate_limit::Priority;
    use tokio::time::{Duration, Instant};

    #[tokio::test]
    async fn test_rest_limit_exhaustion() {
        tokio::time::pause();
        let rl = CoinbaseRateLimit::with_params(
            1,
            Duration::from_millis(10),
            1,
            Duration::from_millis(10),
            Duration::from_millis(0),
        );
        rl.acquire_rest(Priority::Medium).await;
        let start = Instant::now();
        tokio::time::advance(Duration::from_millis(10)).await;
        rl.acquire_rest(Priority::Medium).await;
        assert!(start.elapsed() >= Duration::from_millis(10));
        assert!(start.elapsed() <= Duration::from_millis(50)); // Add upper bound
    }

    #[tokio::test]
    async fn test_ws_backoff_jitter() {
        tokio::time::pause();
        let rl = CoinbaseRateLimit::with_params(
            1,
            Duration::from_millis(5),
            1,
            Duration::from_millis(5),
            Duration::from_millis(5),
        );
        rl.acquire_ws(Priority::Medium).await;
        rl.report_ws_violation().await;
        let start = Instant::now();
        tokio::time::advance(Duration::from_millis(15)).await;
        rl.acquire_ws(Priority::Medium).await;
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(10)); // reduced from 40
        assert!(elapsed <= Duration::from_millis(20)); // reduced from 60
    }
}
