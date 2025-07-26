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

/// User WebSocket event sent by Binance.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "e")]
pub enum BinanceUserEvent {
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
}

impl BinanceUserEvent {
    fn parse(msg: &str) -> Option<Self> {
        serde_json::from_str::<Self>(msg).ok()
    }
}

async fn run_connection(
    mut ws: WebSocket,
    tx: &mpsc::UnboundedSender<BinanceUserEvent>,
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
    while let Some(msg) = ws.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(err) => {
                error!(?err, "WebSocket stream error");
                return Err(());
            }
        };
        match msg {
            WsMessage::Text(text) => {
                if let Some(event) = BinanceUserEvent::parse(&text) {
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

/// Connect to Binance user WebSocket and return a stream of [`BinanceUserEvent`].
pub async fn user_stream(
    url: Url,
    auth_payload: String,
) -> Result<UnboundedReceiverStream<BinanceUserEvent>, SocketError> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(5),
            half_open_max_calls: 3,
        });
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
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    breaker.record_failure();
                    warn!(?err, "failed to connect to WebSocket");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    });
    Ok(UnboundedReceiverStream::new(rx))
}
