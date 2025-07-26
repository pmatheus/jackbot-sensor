//! # Jackbot Integration
//! 
//! Integration components for Jackbot providing WebSocket protocols, circuit breakers,
//! and connectivity utilities for real-time trading applications.

pub mod circuit_breaker;
pub mod protocol;
pub mod error;
pub mod stream;
pub mod subscription;
pub mod validator;
pub mod rate_limit;
pub mod metric;
pub mod de;
pub mod channel;
pub mod collection;
pub mod terminal;

use crate::error::SocketError;
use async_trait::async_trait;

pub use validator::Validator;
pub use terminal::{Terminal, FeedEnded, Unrecoverable};

/// Generic transformer trait for processing and transforming data streams
#[async_trait]
pub trait Transformer {
    type Input;
    type Output;
    type OutputIter: IntoIterator<Item = Result<Self::Output, Self::Error>>;
    type Error;
    
    /// Initialize the transformer with configuration
    async fn init<T>(
        instrument_map: T,
        initial_snapshots: &[Self::Output], 
        sink_tx: tokio::sync::mpsc::UnboundedSender<protocol::websocket::Message>
    ) -> Result<Self, SocketError>
    where
        Self: Sized,
        T: Send;
    
    /// Transform input data to output format
    fn transform(&mut self, input: Self::Input) -> Self::OutputIter;
}