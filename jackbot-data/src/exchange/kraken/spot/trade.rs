//! Trade event types for Kraken Spot.
//!
//! Provides convenient aliases for [`Kraken`](super::super::super::Kraken) trade streams.

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
pub type KrakenSpotTradesTransformer<InstrumentKey> =
    StatelessTransformer<Kraken, InstrumentKey, PublicTrades, KrakenTrades>;

/// Type alias for a Kraken Spot trades WebSocket stream.
pub type KrakenSpotTradesStream<InstrumentKey> =
    ExchangeWsStream<KrakenSpotTradesTransformer<InstrumentKey>>;
