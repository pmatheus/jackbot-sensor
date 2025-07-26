//! Protocol implementations for networking and communication

pub mod websocket;

use crate::error::SocketError;

/// Generic stream parser trait for parsing incoming messages
pub trait StreamParser {
    type Message;
    
    fn parse<T>(data: Result<Self::Message, SocketError>) -> Option<Result<T, SocketError>>
    where
        T: serde::de::DeserializeOwned;
}