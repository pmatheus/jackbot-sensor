use jackbot_integration::rate_limit::{Priority, RateLimiter};
use std::time::Duration;

/// Binance API rate limiter for REST and WebSocket usage.
#[derive(Clone, Debug)]
pub struct BinanceRateLimit {
    rest: RateLimiter,
    ws: RateLimiter,
}

impl Default for BinanceRateLimit {
    fn default() -> Self {
        Self::new()
    }
}

impl BinanceRateLimit {
    /// Create a new [`BinanceRateLimit`] using placeholder quotas.
    ///
    /// REST: 1200 requests per minute.
    /// WebSocket: 5 messages per second.
    pub fn new() -> Self {
        Self::with_params(
            1200,
            Duration::from_secs(60),
            5,
            Duration::from_secs(1),
            Duration::from_millis(100),
        )
    }

    /// Create a custom [`BinanceRateLimit`] with provided quotas and jitter for testing.
    pub fn with_params(
        _rest_capacity: usize,
        _rest_interval: Duration,
        _ws_capacity: usize,
        _ws_interval: Duration,
        _jitter: Duration,
    ) -> Self {
        // Using default rate limiter for now
        // TODO: Implement custom rate limits when RateLimiter supports custom parameters
        Self {
            rest: RateLimiter::new(),
            ws: RateLimiter::new(),
        }
    }

    /// Acquire a REST permit with the specified [`Priority`].
    pub async fn acquire_rest(&self, priority: Priority) {
        self.rest.acquire(priority).await;
    }

    /// Acquire a WebSocket permit with the specified [`Priority`].
    pub async fn acquire_ws(&self, priority: Priority) {
        self.ws.acquire(priority).await;
    }

    /// Report a REST rate limit violation.
    pub async fn report_rest_violation(&self) {
        self.rest.report_violation(Priority::High).await;
    }

    /// Report a WebSocket rate limit violation.
    pub async fn report_ws_violation(&self) {
        self.ws.report_violation(Priority::High).await;
    }
}
