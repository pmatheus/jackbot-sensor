use jackbot_data::streams::{
    consumer::StreamKey,
    reconnect::Event,
    reconnect::stream::{ReconnectingStream, ReconnectionBackoffPolicy, init_reconnecting_stream},
};
use jackbot_instrument::exchange::ExchangeId;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn test_reconnecting_stream_integration() {
    // Uncomment this if tokio::time::pause is available in your version
    // tokio::time::pause();

    let attempts = Arc::new(AtomicUsize::new(0));
    let init = {
        let attempts = attempts.clone();
        move || {
            let attempts = attempts.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Ok::<tokio_stream::Iter<std::vec::IntoIter<Result<i32, ()>>>, ()>(
                        tokio_stream::iter(vec![Ok::<i32, ()>(1), Err::<i32, ()>(())]),
                    )
                } else {
                    Ok::<tokio_stream::Iter<std::vec::IntoIter<Result<i32, ()>>>, ()>(
                        tokio_stream::iter(vec![Ok::<i32, ()>(2)]),
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

    // Use a manual approach to avoid the ambiguous collect method
    let limited_stream = tokio_stream::StreamExt::take(stream, 3);
    tokio::pin!(limited_stream);

    let mut collected = Vec::new();
    while let Some(item) = tokio_stream::StreamExt::next(&mut limited_stream).await {
        collected.push(item);
    }

    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(collected[0], Event::Item(Ok(1)));
    assert_eq!(collected[1], Event::Reconnecting(()));
    assert_eq!(collected[2], Event::Item(Ok(2)));
}
