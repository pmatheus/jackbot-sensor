//! Trade event types for Okx Futures.
//!
//! Provides convenient aliases for [`Okx`](super::super::super::Okx) futures trade streams.

use super::super::Okx;
use crate::{
    ExchangeWsStream, subscription::trade::PublicTrades,
    transformer::stateless::StatelessTransformer,
};

pub use super::super::trade::OkxTrades;

/// [`ExchangeTransformer`](crate::transformer::ExchangeTransformer) used to
/// convert Okx WebSocket trade messages into [`PublicTrade`](PublicTrades) events.
pub type OkxFuturesTradesTransformer<InstrumentKey> =
    StatelessTransformer<Okx, InstrumentKey, PublicTrades, OkxTrades>;

/// Type alias for an Okx Futures trades WebSocket stream.
pub type OkxFuturesTradesStream<InstrumentKey> =
    ExchangeWsStream<OkxFuturesTradesTransformer<InstrumentKey>>;
