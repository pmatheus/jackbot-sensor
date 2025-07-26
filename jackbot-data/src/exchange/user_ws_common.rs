use crate::exchange::DEFAULT_HEARTBEAT_INTERVAL;
use futures::{SinkExt, StreamExt};
use jackbot_integration::{
    circuit_breaker::{CircuitBreaker, CircuitBreakerConfig},
    error::SocketError,
    protocol::websocket::{WebSocket, connect},
};
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{error, warn};
use url::Url;

/// Generic user WebSocket event used across exchanges.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "e")]
pub enum UserWsEvent {
    /// Balance update event.
    #[serde(rename = "balance")]
    Balance {
        #[serde(rename = "E")]
        time: u64,
        asset: String,
        free: String,
        total: String,
    },
    /// Order update event.
    #[serde(rename = "order")]
    Order {
        #[serde(rename = "E")]
        time: u64,
        #[serde(rename = "s")]
        symbol: String,
        #[serde(rename = "S")]
        side: String,
        #[serde(rename = "p")]
        price: String,
        #[serde(rename = "q")]
        quantity: String,
        #[serde(rename = "i")]
        order_id: u64,
        #[serde(rename = "X")]
        status: String,
    },
    /// Position update event.
    #[serde(rename = "position")]
    Position {
        #[serde(rename = "E")]
        time: u64,
        #[serde(rename = "s")]
        symbol: String,
        #[serde(rename = "pa")]
        qty: String,
        #[serde(rename = "ps")]
        side: String,
    },
}

impl UserWsEvent {
    fn parse(msg: &str) -> Option<Self> {
        serde_json::from_str::<Self>(msg).ok()
    }
}

async fn run_connection(
    mut ws: WebSocket,
    tx: &mpsc::UnboundedSender<UserWsEvent>,
    auth_payload: &str,
) -> Result<(), ()> {
    if ws
        .send(WsMessage::text(auth_payload.to_string()))
        .await
        .is_err()
    {
        error!("failed to send auth payload over WebSocket");
        return Err(());
    }
    while let Some(msg) = match tokio::time::timeout(DEFAULT_HEARTBEAT_INTERVAL, ws.next()).await {
        Ok(m) => m,
        Err(_) => return Err(()),
    } {
        let msg = match msg {
            Ok(m) => m,
            Err(err) => {
                error!(?err, "WebSocket stream error");
                return Err(());
            }
        };
        match msg {
            WsMessage::Text(text) => {
                if let Some(event) = UserWsEvent::parse(&text) {
                    let _ = tx.send(event);
                }
            }
            WsMessage::Close(_) => {
                warn!("received close frame from server");
                return Err(());
            }
            _ => {}
        }
    }
    Err(())
}

/// Connect to a user WebSocket and return a stream of [`UserWsEvent`].
pub async fn user_stream(
    url: Url,
    auth_payload: String,
) -> Result<UnboundedReceiverStream<UserWsEvent>, SocketError> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(5),
            half_open_max_calls: 3,
        });

        let mut backoff = Duration::from_millis(50);

        loop {
            if breaker.is_open() {
                if let Some(wait) = breaker.remaining() {
                    warn!(?wait, "circuit breaker open, waiting before reconnect");
                    tokio::time::sleep(wait).await;
                    continue;
                }
            }
            match connect(url.clone()).await {
                Ok(ws) => {
                    breaker.reset();
                    if run_connection(ws, &tx, &auth_payload).await.is_err() {
                        breaker.record_failure();
                        tokio::time::sleep(backoff).await;
                        backoff = std::cmp::min(backoff * 2, Duration::from_secs(30));
                        continue;
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    breaker.record_failure();
                    warn!(?err, "failed to connect to WebSocket");
                    tokio::time::sleep(backoff).await;
                    backoff = std::cmp::min(backoff * 2, Duration::from_secs(30));
                }
            }
        }
    });
    Ok(UnboundedReceiverStream::new(rx))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    pub async fn run_server(payloads: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for payload in payloads {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = accept_async(stream).await.unwrap();
                ws.next().await.unwrap().unwrap();
                ws.send(Message::Text(payload.into())).await.unwrap();
                ws.close(None).await.unwrap();
            }
        });
        format!("127.0.0.1:{}", addr.port())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_user_stream_parse() {
        let first = r#"{"e":"balance","E":1,"asset":"BTC","free":"0.5","total":"1.0"}"#.to_string();
        let second =
            r#"{"e":"order","E":2,"s":"BTCUSDT","S":"BUY","p":"100","q":"0.1","i":1,"X":"NEW"}"#
                .to_string();
        let third = r#"{"e":"position","E":3,"s":"BTCUSDT","pa":"0.2","ps":"LONG"}"#.to_string();
        let addr = run_server(vec![first.clone(), second.clone(), third.clone()]).await;

        let mut stream = user_stream(
            Url::parse(&format!("ws://{}", addr)).unwrap(),
            "{}".to_string(),
        )
        .await
        .unwrap();

        // Add timeout to prevent test hanging
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            let ev1 = stream.next().await.unwrap();
            assert!(matches!(ev1, UserWsEvent::Balance { .. }));
            let ev2 = stream.next().await.unwrap();
            assert!(matches!(ev2, UserWsEvent::Order { .. }));
            let ev3 = stream.next().await.unwrap();
            assert!(matches!(ev3, UserWsEvent::Position { .. }));
        })
        .await;

        assert!(result.is_ok(), "Test timed out after 5 seconds");
    }

    async fn run_timeout_server(first: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // first connection - no messages, triggers heartbeat timeout
            let (stream1, _) = listener.accept().await.unwrap();
            let mut ws1 = accept_async(stream1).await.unwrap();
            ws1.next().await.unwrap().unwrap();
            // Use shorter timeout for testing - 2 seconds instead of 90
            tokio::time::sleep(Duration::from_secs(2) + Duration::from_secs(1)).await;
            ws1.close(None).await.unwrap();

            // second connection - send real payload
            let (stream2, _) = listener.accept().await.unwrap();
            let mut ws2 = accept_async(stream2).await.unwrap();
            ws2.next().await.unwrap().unwrap();
            ws2.send(Message::Text(first.into())).await.unwrap();
            ws2.close(None).await.unwrap();
        });
        format!("127.0.0.1:{}", addr.port())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_user_stream_reconnect_on_timeout() {
        tokio::time::pause();
        let first = r#"{"e":"balance","E":1,"asset":"BTC","free":"0.5","total":"1.0"}"#.to_string();
        let addr = run_timeout_server(first.clone()).await;
        let mut stream = user_stream(
            Url::parse(&format!("ws://{}", addr)).unwrap(),
            "{}".to_string(),
        )
        .await
        .unwrap();

        // Use shorter heartbeat interval for testing
        let test_heartbeat = Duration::from_secs(2);
        tokio::time::advance(test_heartbeat + Duration::from_secs(1)).await;

        // Add timeout to prevent test hanging
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            let ev1 = stream.next().await.unwrap();
            assert!(matches!(ev1, UserWsEvent::Balance { .. }));
        })
        .await;

        assert!(result.is_ok(), "Test timed out after 5 seconds");
    }
}
