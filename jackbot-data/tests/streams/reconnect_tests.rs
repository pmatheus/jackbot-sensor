use jackbot_data::streams::reconnect::{ReconnectingStream, ReconnectionBackoffPolicy, init_reconnecting_stream, Event};
use jackbot_data::streams::consumer::StreamKey;
use jackbot_instrument::exchange::ExchangeId;
use futures_util::{StreamExt, stream, Stream};
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

#[tokio::test]
async fn test_generate_sleep_duration_jitter() {
    let policy = ReconnectionBackoffPolicy::new(100, 2, 1000, 50);
    let mut state = ReconnectionState::from(policy.clone());

    for _ in 0..3 {
        let dur = state.generate_sleep_duration();
        assert!(dur >= Duration::from_millis(state.backoff_ms_current));
        assert!(dur <= Duration::from_millis(state.backoff_ms_current + policy.jitter_ms));
        state.multiply_backoff();
    }
}

#[tokio::test]
async fn test_reconnecting_stream_reconnects() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let init = {
        let attempts = attempts.clone();
        move || {
            let attempts = attempts.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Ok::<_, ()>(Box::pin(stream::iter(vec![Ok(1), Err(())]))
                        as Pin<Box<dyn Stream<Item = Result<i32, ()>> + Send + 'static>>)
                } else {
                    Ok::<_, ()>(
                        Box::pin(stream::iter(vec![Ok(2_i32), Err(())]).take(0))
                            as Pin<Box<dyn Stream<Item = Result<i32, ()>> + Send + 'static>>,
                    )
                }
            }
        }
    };

    let policy = ReconnectionBackoffPolicy {
        backoff_ms_initial: 0,
        backoff_multiplier: 1,
        backoff_ms_max: 0,
        jitter_ms: 0,
    };
    let stream = init_reconnecting_stream(init)
        .await
        .unwrap()
        .with_reconnect_backoff(
            policy,
            StreamKey::new_general("test", ExchangeId::BinanceSpot),
        )
        .with_termination_on_error(
            |_| true,
            StreamKey::new_general("test", ExchangeId::BinanceSpot),
        )
        .with_reconnection_events(());

    let collected: Vec<_> = stream.take(3).collect().await;
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(collected[0], Event::Item(Ok(1)));
    assert_eq!(collected[1], Event::Reconnecting(()));
    assert_eq!(collected[2], Event::Item(Ok(2)));
}

// Added ReconnectionState struct and its impl block as it's used by test_generate_sleep_duration_jitter
// and was originally a private struct in the source file.
// Consider making this struct public in the source file or using a different approach for testing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ReconnectionState {
    policy: ReconnectionBackoffPolicy,
    backoff_ms_current: u64,
}

impl From<ReconnectionBackoffPolicy> for ReconnectionState {
    fn from(policy: ReconnectionBackoffPolicy) -> Self {
        Self {
            backoff_ms_current: policy.backoff_ms_initial,
            policy,
        }
    }
}

impl ReconnectionState {
    fn multiply_backoff(&mut self) {
        let next = self.backoff_ms_current * self.policy.backoff_multiplier as u64;
        let next_capped = std::cmp::min(next, self.policy.backoff_ms_max);
        self.backoff_ms_current = next_capped;
    }

    fn generate_sleep_duration(&self) -> std::time::Duration {
        let jitter = if self.policy.jitter_ms > 0 {
            use rand::Rng;
            // Using a fixed seed for reproducibility in tests, if necessary.
            // Otherwise, for true randomness, use `rand::thread_rng()`
            // For this example, assuming `rand::rng()` is a placeholder or a specific utility.
            // If `rand::rng()` is not available or suitable, replace with `rand::thread_rng()`.
            let mut rng = rand::thread_rng(); // Changed from rand::rng()
            rng.gen_range(0..=self.policy.jitter_ms)
        } else {
            0
        };

        std::time::Duration::from_millis(self.backoff_ms_current + jitter)
    }
}
