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

/// User WebSocket event sent by Binance Futures.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "e")]
pub enum BinanceUserEvent {
    #[serde(rename = "balance")]
    Balance {
        #[serde(rename = "E")]
        time: u64,
        asset: String,
        free: String,
        total: String,
    },
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
    if ws.send(WsMessage::Text(auth_payload.to_string())).await.is_err() {
        return Err(());
    }
    while let Some(msg) = ws.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => return Err(()),
        };
        match msg {
            WsMessage::Text(text) => {
                if let Some(event) = BinanceUserEvent::parse(&text) {
                    let _ = tx.send(event);
                }
            }
            WsMessage::Close(_) => return Err(()),
            _ => {}
        }
    }
    Err(())
}

/// Connect to Binance Futures user WebSocket and return a stream of [`BinanceUserEvent`].
pub async fn user_stream(
    url: Url,
    auth_payload: String,
) -> Result<UnboundedReceiverStream<BinanceUserEvent>, SocketError> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            match connect(url.clone()).await {
                Ok(ws) => {
                    if run_connection(ws, &tx, &auth_payload).await.is_err() {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    } else {
                        break;
                    }
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    });
    Ok(UnboundedReceiverStream::new(rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    async fn run_server(addr: &str, first: String, second: String) {
        let listener = TcpListener::bind(addr).await.unwrap();
        for payload in [first, second] {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            ws.next().await.unwrap().unwrap();
            ws.send(Message::Text(payload)).await.unwrap();
            ws.close(None).await.unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_user_stream_parse() {
        let addr = "127.0.0.1:18100";
        let first = r#"{\"e\":\"balance\",\"E\":1,\"asset\":\"BTC\",\"free\":\"0.5\",\"total\":\"1.0\"}"#.to_string();
        let second = r#"{\"e\":\"order\",\"E\":2,\"s\":\"BTCUSDT\",\"S\":\"BUY\",\"p\":\"100\",\"q\":\"0.1\",\"i\":1,\"X\":\"NEW\"}"#.to_string();
        tokio::spawn(run_server(addr, first.clone(), second.clone()));

        let mut stream = user_stream(Url::parse(&format!("ws://{}", addr)).unwrap(), "{}".to_string()).await.unwrap();
        let ev1 = stream.next().await.unwrap();
        assert!(matches!(ev1, BinanceUserEvent::Balance{..}));
        let ev2 = stream.next().await.unwrap();
        assert!(matches!(ev2, BinanceUserEvent::Order{..}));
    }
}
