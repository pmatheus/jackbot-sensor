//! User WebSocket handling for Bitget Spot with health monitoring.
use crate::exchange::{DEFAULT_HEARTBEAT_INTERVAL, user_ws_common::UserWsEvent as BitgetUserEvent};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use jackbot_instrument::exchange::ExchangeId;
use jackbot_integration::{
    error::SocketError,
    metric::{Field, Metric, Tag},
    protocol::websocket::{WebSocket, WsMessage, connect, with_heartbeat},
};
use rand::Rng;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};
use url::Url;

async fn run_connection(
    ws: WebSocket,
    tx: &mpsc::UnboundedSender<BitgetUserEvent>,
    auth_payload: &str,
) -> Result<(), ()> {
    let (mut sink, stream) = ws.split();
    if sink.send(WsMessage::text(auth_payload)).await.is_err() {
        return Err(());
    }
    let mut stream = with_heartbeat(stream, DEFAULT_HEARTBEAT_INTERVAL, ExchangeId::BitgetSpot);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => return Err(()),
        };
        match msg {
            Message::Text(text) => {
                if let Ok(event) = serde_json::from_str::<BitgetUserEvent>(&text) {
                    let _ = tx.send(event);
                }
            }
            Message::Close(_) => return Err(()),
            _ => {}
        }
    }
    Err(())
}

/// Connect to the Bitget Spot user WebSocket with reconnection and metrics.
pub async fn user_stream(
    url: Url,
    auth_payload: String,
) -> Result<UnboundedReceiverStream<BitgetUserEvent>, SocketError> {
    const BACKOFF_INITIAL: u64 = 50;
    const BACKOFF_MAX: u64 = 1_000;
    const BACKOFF_MULT: u64 = 2;
    const JITTER: u64 = 50;

    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut backoff = BACKOFF_INITIAL;
        loop {
            match connect(url.clone()).await {
                Ok(ws) => {
                    let metric = Metric {
                        name: "ws_user_connect_success",
                        time: Utc::now().timestamp_millis() as u64,
                        tags: vec![Tag::new("exchange", ExchangeId::BitgetSpot.as_str())],
                        fields: vec![],
                    };
                    info!(?metric, "connected to Bitget user WebSocket");
                    backoff = BACKOFF_INITIAL;
                    if run_connection(ws, &tx, &auth_payload).await.is_err() {
                        let jitter = rand::thread_rng().gen_range(0..=JITTER);
                        let delay = std::time::Duration::from_millis(backoff + jitter);
                        let metric = Metric {
                            name: "ws_user_reconnect_backoff",
                            time: Utc::now().timestamp_millis() as u64,
                            tags: vec![Tag::new("exchange", ExchangeId::BitgetSpot.as_str())],
                            fields: vec![Field::new("backoff_ms", delay.as_millis() as u64)],
                        };
                        warn!(?metric, "Bitget user WebSocket disconnected, reconnecting");
                        tokio::time::sleep(delay).await;
                        backoff = (backoff * BACKOFF_MULT).min(BACKOFF_MAX);
                        continue;
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    error!(?err, "failed to connect to Bitget user WebSocket");
                    let jitter = rand::thread_rng().gen_range(0..=JITTER);
                    let delay = std::time::Duration::from_millis(backoff + jitter);
                    tokio::time::sleep(delay).await;
                    backoff = (backoff * BACKOFF_MULT).min(BACKOFF_MAX);
                }
            }
        }
    });
    Ok(UnboundedReceiverStream::new(rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchange::user_ws_common::tests::run_server;
    use futures::StreamExt;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_user_stream_parse() {
        let first =
            r#"{\"e\":\"balance\",\"E\":1,\"asset\":\"BTC\",\"free\":\"0.5\",\"total\":\"1.0\"}"#
                .to_string();
        let second = r#"{\"e\":\"order\",\"E\":2,\"s\":\"BTCUSDT\",\"S\":\"BUY\",\"p\":\"100\",\"q\":\"0.1\",\"i\":1,\"X\":\"NEW\"}"#.to_string();
        let third =
            r#"{\"e\":\"position\",\"E\":3,\"s\":\"BTCUSDT\",\"pa\":\"0.2\",\"ps\":\"LONG\"}"#
                .to_string();
        let addr = run_server(vec![first, second, third]).await;
        let mut stream = user_stream(
            Url::parse(&format!("ws://{}", addr)).unwrap(),
            "{}".to_string(),
        )
        .await
        .unwrap();
        assert!(matches!(
            stream.next().await.unwrap(),
            BitgetUserEvent::Balance { .. }
        ));
        assert!(matches!(
            stream.next().await.unwrap(),
            BitgetUserEvent::Order { .. }
        ));
        assert!(matches!(
            stream.next().await.unwrap(),
            BitgetUserEvent::Position { .. }
        ));
    }
}
