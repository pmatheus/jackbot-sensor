use jackbot_integration::rate_limit::{Priority, RateLimiter};
use tokio::time::{Duration, Instant, sleep};

#[tokio::test]
async fn test_rate_limit_basic() {
    let rl = RateLimiter::new(2, Duration::from_millis(50));
    rl.acquire(Priority::Normal).await;
    rl.acquire(Priority::Normal).await;
    let start = Instant::now();
    rl.acquire(Priority::Normal).await;
    assert!(start.elapsed() >= Duration::from_millis(50));
}

#[tokio::test]
async fn test_priority_queue() {
    let rl = RateLimiter::new(1, Duration::from_millis(40));
    // consume initial token
    rl.acquire(Priority::Normal).await;
    let rl1 = rl.clone();
    let t1 = tokio::spawn(async move {
        rl1.acquire(Priority::Low).await;
        Instant::now()
    });
    sleep(Duration::from_millis(10)).await;
    let rl2 = rl.clone();
    let t2 = tokio::spawn(async move {
        rl2.acquire(Priority::High).await;
        Instant::now()
    });
    let time_high = t2.await.unwrap();
    let time_low = t1.await.unwrap();
    assert!(time_high <= time_low);
}

#[tokio::test]
async fn test_adaptive_backoff() {
    let rl = RateLimiter::new(1, Duration::from_millis(30));
    rl.acquire(Priority::Normal).await;
    rl.report_violation().await; // double interval
    let start = Instant::now();
    rl.acquire(Priority::Normal).await;
    assert!(start.elapsed() >= Duration::from_millis(60));
}

#[tokio::test]
async fn test_backoff_jitter() {
    let rl = RateLimiter::new_with_jitter(1, Duration::from_millis(20), Duration::from_millis(20));
    rl.acquire(Priority::Normal).await;
    rl.report_violation().await; // interval 40-60ms
    let start = Instant::now();
    rl.acquire(Priority::Normal).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(40));
    assert!(elapsed <= Duration::from_millis(60));
}
