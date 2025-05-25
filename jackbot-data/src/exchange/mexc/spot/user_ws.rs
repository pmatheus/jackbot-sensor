//! User WebSocket handling for Mexc Spot.
pub use crate::exchange::user_ws_common::{user_stream, UserWsEvent as MexcUserEvent};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchange::user_ws_common::tests::run_server;
    use futures::StreamExt;
    use url::Url;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_user_stream_parse() {
        let first = r#"{\"e\":\"balance\",\"E\":1,\"asset\":\"BTC\",\"free\":\"0.5\",\"total\":\"1.0\"}"#.to_string();
        let second = r#"{\"e\":\"order\",\"E\":2,\"s\":\"BTCUSDT\",\"S\":\"BUY\",\"p\":\"100\",\"q\":\"0.1\",\"i\":1,\"X\":\"NEW\"}"#.to_string();
        let third = r#"{\"e\":\"position\",\"E\":3,\"s\":\"BTCUSDT\",\"pa\":\"0.2\",\"ps\":\"LONG\"}"#.to_string();
        let addr = run_server(vec![first, second, third]).await;
        let mut stream = user_stream(Url::parse(&format!("ws://{}", addr)).unwrap(), "{}".to_string()).await.unwrap();
        assert!(matches!(stream.next().await.unwrap(), MexcUserEvent::Balance{..}));
        assert!(matches!(stream.next().await.unwrap(), MexcUserEvent::Order{..}));
        assert!(matches!(stream.next().await.unwrap(), MexcUserEvent::Position{..}));
    }
}
