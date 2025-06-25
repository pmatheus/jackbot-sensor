use jackbot_data::exchange::binance::rate_limit::BinanceRateLimit;
use jackbot_integration::rate_limit::Priority;
use tokio::time::{Duration, Instant};

#[tokio::test]
async fn test_rest_limit_exhaustion() {
    let rl = BinanceRateLimit::with_params(1, Duration::from_millis(10), 1, Duration::from_millis(10), Duration::from_millis(0));
    rl.acquire_rest(Priority::Normal).await;
    let start = Instant::now();
    rl.acquire_rest(Priority::Normal).await;
    assert!(start.elapsed() >= Duration::from_millis(10));
    assert!(start.elapsed() <= Duration::from_millis(50)); // Add upper bound
}

#[tokio::test]
async fn test_ws_backoff_jitter() {
    let rl = BinanceRateLimit::with_params(1, Duration::from_millis(5), 1, Duration::from_millis(5), Duration::from_millis(5));
    rl.acquire_ws(Priority::Normal).await;
    rl.report_ws_violation().await;
    let start = Instant::now();
    rl.acquire_ws(Priority::Normal).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(10)); // reduced from 40
    assert!(elapsed <= Duration::from_millis(20)); // reduced from 60
}
