use jackbot_data::exchange::{
    cryptocom::rate_limit::CryptocomRateLimit,
    mexc::rate_limit::MexcRateLimit,
};
use jackbot_integration::rate_limit::Priority;
use tokio::time::{Duration, Instant};

#[tokio::test]
async fn test_mexc_rest_backoff_jitter() {
    let rl = MexcRateLimit::with_params(
        1,
        Duration::from_millis(20),
        1,
        Duration::from_millis(20),
        Duration::from_millis(20),
    );
    rl.acquire_rest(Priority::Normal).await;
    rl.report_rest_violation().await;
    let start = Instant::now();
    rl.acquire_rest(Priority::Normal).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(40));
    assert!(elapsed <= Duration::from_millis(60));
}

#[tokio::test]
async fn test_cryptocom_ws_backoff_jitter() {
    let rl = CryptocomRateLimit::with_params(
        1,
        Duration::from_millis(20),
        1,
        Duration::from_millis(20),
        Duration::from_millis(20),
    );
    rl.acquire_ws(Priority::Normal).await;
    rl.report_ws_violation().await;
    let start = Instant::now();
    rl.acquire_ws(Priority::Normal).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(40));
    assert!(elapsed <= Duration::from_millis(60));
}
