//! Error types for integration components

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SocketError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    
    #[error("Timeout error")]
    Timeout,
    
    #[error("Authentication error: {0}")]
    AuthError(String),
    
    #[error("Rate limit exceeded")]
    RateLimit,
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("Subscribe error: {0}")]
    Subscribe(String),
    
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
    
    #[error("Unsupported: {entity} - {item}")]
    Unsupported {
        entity: String,
        item: String,
    },
    
    #[error("Terminated: {0:?}")]
    Terminated(Option<tokio_tungstenite::tungstenite::protocol::CloseFrame>),
    
    #[error("Deserialize error: {0}")]
    Deserialise(String),
}