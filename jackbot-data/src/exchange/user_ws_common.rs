use serde::Deserialize;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use futures::{StreamExt, SinkExt};
use url::Url;
use jackbot_integration::{
    protocol::websocket::{connect, WebSocket},
    error::SocketError,
};
use crate::exchange::DEFAULT_HEARTBEAT_INTERVAL;

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
    if ws.send(WsMessage::text(auth_payload)).await.is_err() {
        return Err(());
    }
    while let Some(msg) = match tokio::time::timeout(DEFAULT_HEARTBEAT_INTERVAL, ws.next()).await {
        Ok(m) => m,
        Err(_) => return Err(()),
    } {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => return Err(()),
        };
        match msg {
            WsMessage::Text(text) => {
                if let Some(event) = UserWsEvent::parse(&text) {
                    let _ = tx.send(event);
                }
            }
            WsMessage::Close(_) => return Err(()),
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
        let mut backoff = Duration::from_millis(50);
        loop {
            match connect(url.clone()).await {
                Ok(ws) => {
                    if run_connection(ws, &tx, &auth_payload).await.is_err() {
                        tokio::time::sleep(backoff).await;
                        backoff = std::cmp::min(backoff * 2, Duration::from_secs(30));
                        continue;
                    } else {
                        break;
                    }
                }
                Err(_) => {
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
                ws.send(Message::Text(payload)).await.unwrap();
                ws.close(None).await.unwrap();
            }
        });
        format!("127.0.0.1:{}", addr.port())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_user_stream_parse() {
        let first = r#"{\"e\":\"balance\",\"E\":1,\"asset\":\"BTC\",\"free\":\"0.5\",\"total\":\"1.0\"}"#.to_string();
        let second = r#"{\"e\":\"order\",\"E\":2,\"s\":\"BTCUSDT\",\"S\":\"BUY\",\"p\":\"100\",\"q\":\"0.1\",\"i\":1,\"X\":\"NEW\"}"#.to_string();
        let third = r#"{\"e\":\"position\",\"E\":3,\"s\":\"BTCUSDT\",\"pa\":\"0.2\",\"ps\":\"LONG\"}"#.to_string();
        let addr = run_server(vec![first.clone(), second.clone(), third.clone()]).await;

        let mut stream = user_stream(Url::parse(&format!("ws://{}", addr)).unwrap(), "{}".to_string()).await.unwrap();
        let ev1 = stream.next().await.unwrap();
        assert!(matches!(ev1, UserWsEvent::Balance{..}));
        let ev2 = stream.next().await.unwrap();
        assert!(matches!(ev2, UserWsEvent::Order{..}));
        let ev3 = stream.next().await.unwrap();
        assert!(matches!(ev3, UserWsEvent::Position{..}));
    }

    async fn run_timeout_server(first: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // first connection - no messages, triggers heartbeat
            let (stream1, _) = listener.accept().await.unwrap();
            let mut ws1 = accept_async(stream1).await.unwrap();
            ws1.next().await.unwrap().unwrap();
            tokio::time::sleep(DEFAULT_HEARTBEAT_INTERVAL + Duration::from_secs(1)).await;
            ws1.close(None).await.unwrap();

            // second connection - send real payload
            let (stream2, _) = listener.accept().await.unwrap();
            let mut ws2 = accept_async(stream2).await.unwrap();
            ws2.next().await.unwrap().unwrap();
            ws2.send(Message::Text(first)).await.unwrap();
            ws2.close(None).await.unwrap();
        });
        format!("127.0.0.1:{}", addr.port())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_user_stream_reconnect_on_timeout() {
        tokio::time::pause();
        let first = r#"{\"e\":\"balance\",\"E\":1,\"asset\":\"BTC\",\"free\":\"0.5\",\"total\":\"1.0\"}"#.to_string();
        let addr = run_timeout_server(first.clone()).await;
        let mut stream = user_stream(Url::parse(&format!("ws://{}", addr)).unwrap(), "{}".to_string()).await.unwrap();
        tokio::time::advance(DEFAULT_HEARTBEAT_INTERVAL + Duration::from_secs(2)).await;
        let ev1 = stream.next().await.unwrap();
        assert!(matches!(ev1, UserWsEvent::Balance{..}));
    }
}

