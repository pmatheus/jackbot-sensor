//! WebSocket protocol implementation

use crate::error::SocketError;
use crate::protocol::StreamParser;
use futures::{stream::BoxStream, Sink, SinkExt, Stream, StreamExt};
use serde::de::DeserializeOwned;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;
use url::Url;

pub type WsMessage = TungsteniteMessage;
pub type WsSink = Pin<Box<dyn Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Send>>;
pub type WsError = tokio_tungstenite::tungstenite::Error;

/// Generic message type for internal messaging
pub type Message = WsMessage;

/// WebSocket connection wrapper
#[derive(Debug)]
pub struct WebSocket {
    inner: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
}

impl WebSocket {
    pub fn split(self) -> (WsSink, BoxStream<'static, Result<WsMessage, WsError>>) {
        let (sink, stream) = StreamExt::split(self.inner);
        (
            Box::pin(sink),
            Box::pin(stream)
        )
    }
}

impl Stream for WebSocket {
    type Item = Result<WsMessage, WsError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl Sink<WsMessage> for WebSocket {
    type Error = WsError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: WsMessage) -> Result<(), Self::Error> {
        Pin::new(&mut self.inner).start_send(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

/// WebSocket parser for parsing JSON messages
pub struct WebSocketParser;

impl StreamParser for WebSocketParser {
    type Message = WsMessage;
    
    fn parse<T>(data: Result<Self::Message, SocketError>) -> Option<Result<T, SocketError>>
    where
        T: DeserializeOwned,
    {
        match data {
            Ok(WsMessage::Text(text)) => {
                Some(serde_json::from_str(&text).map_err(|e| SocketError::ParseError(e.to_string())))
            }
            Ok(WsMessage::Binary(bin)) => {
                Some(serde_json::from_slice(&bin).map_err(|e| SocketError::ParseError(e.to_string())))
            }
            Ok(_) => None, // Ignore non-data messages
            Err(e) => Some(Err(e)),
        }
    }
}

/// Connect to a WebSocket endpoint
pub async fn connect(url: Url) -> Result<WebSocket, SocketError> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| SocketError::ConnectionError(e.to_string()))?;
    
    Ok(WebSocket { inner: ws_stream })
}

/// Check if a WebSocket error indicates disconnection
pub fn is_websocket_disconnected(error: &WsError) -> bool {
    matches!(error, 
        WsError::ConnectionClosed | 
        WsError::AlreadyClosed |
        WsError::Io(_)
    )
}

/// Add heartbeat monitoring to a WebSocket stream
pub fn with_heartbeat<S, ExchangeId>(
    stream: S,
    _timeout: std::time::Duration,
    _exchange_id: ExchangeId,
) -> impl Stream<Item = Result<WsMessage, WsError>>
where
    S: Stream<Item = Result<WsMessage, WsError>> + Send + 'static,
{
    // For now, just pass through the stream
    // In a full implementation, this would add timeout monitoring
    stream
}