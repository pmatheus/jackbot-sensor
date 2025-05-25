use futures_util::StreamExt;
use jackbot_data::streams::{
    consumer::StreamKey,
    reconnect::stream::{ReconnectingStream, ReconnectionBackoffPolicy, init_reconnecting_stream},
    reconnect::{self, Event},
};
use jackbot_instrument::exchange::ExchangeId;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio_stream::StreamExt as TokioStreamExt;

#[tokio::test]
async fn test_reconnecting_stream_integration() {
    tokio::time::pause();

    let attempts = Arc::new(AtomicUsize::new(0));
    let init = {
        let attempts = attempts.clone();
        move || {
            let attempts = attempts.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Ok(tokio_stream::iter(vec![Ok::<_, ()>(1), Err(())]))
                } else {
                    Ok(tokio_stream::iter(vec![Ok::<_, ()>(2)]))
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
