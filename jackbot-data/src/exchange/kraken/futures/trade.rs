//! Trade event types for Kraken Futures.
//!
//! Provides convenient aliases for [`Kraken`](super::super::super::Kraken) futures trade streams.

use crate::{
    transformer::stateless::StatelessTransformer,
    subscription::trade::PublicTrades,
    ExchangeWsStream,
};
use super::super::super::Kraken;

pub use super::super::trade::KrakenTrades;

/// [`ExchangeTransformer`](crate::transformer::ExchangeTransformer) used to
/// convert Kraken WebSocket trade messages into [`PublicTrade`](PublicTrades)
/// events.
pub type KrakenFuturesTradesTransformer<InstrumentKey> =
    StatelessTransformer<Kraken, InstrumentKey, PublicTrades, KrakenTrades>;

/// Type alias for a Kraken Futures trades WebSocket stream.
pub type KrakenFuturesTradesStream<InstrumentKey> =
    ExchangeWsStream<KrakenFuturesTradesTransformer<InstrumentKey>>;
