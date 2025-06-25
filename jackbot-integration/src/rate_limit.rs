use rand::Rng;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, oneshot};
use tracing::{debug, warn};

/// Priority levels for rate limited operations.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Priority {
    High,
    Normal,
    Low,
}

struct Waiter {
    tx: oneshot::Sender<()>,
}

struct Inner {
    capacity: usize,
    tokens: usize,
    interval: Duration,
    last_refill: Instant,
    base_interval: Duration,
    max_interval: Duration,
    jitter: Duration,
    high: VecDeque<Waiter>,
    normal: VecDeque<Waiter>,
    low: VecDeque<Waiter>,
}

impl Inner {
    fn refill(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_refill) >= self.interval {
            let periods =
                now.duration_since(self.last_refill).as_millis() / self.interval.as_millis();
            let add_tokens = (periods as usize + 1) * self.capacity;
            self.last_refill = now;
            self.tokens = usize::min(self.tokens + add_tokens, self.capacity);
            while self.tokens > 0 {
                if let Some(waiter) = self
                    .high
                    .pop_front()
                    .or_else(|| self.normal.pop_front())
                    .or_else(|| self.low.pop_front())
                {
                    self.tokens -= 1;
                    let _ = waiter.tx.send(());
                } else {
                    break;
                }
            }
            if self.tokens > self.capacity {
                self.tokens = self.capacity;
            }
        }
    }
}

/// Simple token bucket rate limiter with priority queues and adaptive backoff.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<Inner>>,
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("inner", &"<mutex>")
            .finish()
    }
}

impl RateLimiter {
    /// Construct a new [`RateLimiter`] allowing `capacity` operations every `interval`.
    pub fn new(capacity: usize, interval: Duration) -> Self {
        Self::new_with_jitter(capacity, interval, Duration::from_millis(0))
    }

    /// Construct a new [`RateLimiter`] with configurable jitter applied on backoff.
    pub fn new_with_jitter(capacity: usize, interval: Duration, jitter: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                capacity,
                tokens: capacity,
                interval,
                last_refill: Instant::now(),
                base_interval: interval,
                max_interval: interval * 16,
                jitter,
                high: VecDeque::new(),
                normal: VecDeque::new(),
                low: VecDeque::new(),
            })),
        }
    }

    /// Acquire a permit according to the provided priority.
    pub async fn acquire(&self, priority: Priority) {
        loop {
            let rx = {
                let mut inner = self.inner.lock().await;
                inner.refill();
                if inner.tokens > 0 {
                    inner.tokens -= 1;
                    None
                } else {
                    debug!("rate limit reached - waiting for permit");
                    let (tx, rx) = oneshot::channel();
                    let waiter = Waiter { tx };
                    match priority {
                        Priority::High => inner.high.push_back(waiter),
                        Priority::Normal => inner.normal.push_back(waiter),
                        Priority::Low => inner.low.push_back(waiter),
                    }
                    Some(rx)
                }
            };
            match rx {
                None => return,
                Some(rx) => {
                    let _ = rx.await;
                }
            }
        }
    }

    /// Report a rate limit violation to trigger backoff.
    pub async fn report_violation(&self) {
        let mut inner = self.inner.lock().await;
        let mut rng = rand::rng();
        let jitter_ms = if inner.jitter.is_zero() {
            0
        } else {
            rng.random_range(0..=inner.jitter.as_millis() as u64)
        };
        let next = inner.interval * 2 + Duration::from_millis(jitter_ms);
        inner.interval = std::cmp::min(next, inner.max_interval + inner.jitter);
        warn!(
            "Rate limit violation - increasing backoff to {} ms",
            inner.interval.as_millis()
        );
    }

    /// Reset the current backoff to the base interval.
    pub async fn reset_backoff(&self) {
        let mut inner = self.inner.lock().await;
        inner.interval = inner.base_interval;
        debug!("Resetting rate limit backoff to base interval");
    }
}
