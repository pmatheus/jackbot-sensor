//! User WebSocket handling for Bybit Spot.
pub use crate::exchange::user_ws_common::{UserWsEvent as BybitUserEvent, user_stream};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchange::user_ws_common::tests::run_server;
    use futures::StreamExt;
    use url::Url;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_user_stream_parse() {
        let first = r#"{"e":"balance","E":1,"asset":"BTC","free":"0.5","total":"1.0"}"#.to_string();
        let second =
            r#"{"e":"order","E":2,"s":"BTCUSDT","S":"BUY","p":"100","q":"0.1","i":1,"X":"NEW"}"#
                .to_string();
        let third = r#"{"e":"position","E":3,"s":"BTCUSDT","pa":"0.2","ps":"LONG"}"#.to_string();
        let addr = run_server(vec![first, second, third]).await;
        let mut stream = user_stream(
            Url::parse(&format!("ws://{}", addr)).unwrap(),
            "{}".to_string(),
        )
        .await
        .unwrap();

        // Add timeout to prevent test hanging
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let event1 = stream.next().await.unwrap();
            assert!(matches!(event1, BybitUserEvent::Balance { .. }));
            let event2 = stream.next().await.unwrap();
            assert!(matches!(event2, BybitUserEvent::Order { .. }));
            let event3 = stream.next().await.unwrap();
            assert!(matches!(event3, BybitUserEvent::Position { .. }));
        })
        .await;

        assert!(result.is_ok(), "Test timed out after 5 seconds");
    }
}
