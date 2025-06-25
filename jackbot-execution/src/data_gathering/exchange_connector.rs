// Stub file for exchange connector module
// This module provides placeholder implementations for exchange connectivity

use jackbot_instrument::instrument::name::InstrumentNameExchange;
use std::future::Future;
use std::pin::Pin;

/// Placeholder trait for exchange connectors
pub trait ExchangeConnector: Send + Sync + std::fmt::Debug {
    fn connect(&self) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + '_>>;
    fn disconnect(&self) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + '_>>;
    fn subscribe_market_data(
        &self,
        _instrument: InstrumentNameExchange,
        _data_types: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + '_>>;
    fn subscribe_order_book(
        &self,
        _instrument: InstrumentNameExchange,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + '_>>;
    fn subscribe_trades(
        &self,
        _instrument: InstrumentNameExchange,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectorError>> + Send + '_>>;
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Subscription failed: {0}")]
    SubscriptionFailed(String),
}
