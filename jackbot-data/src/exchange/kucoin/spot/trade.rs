//! Trade event types for Kucoin Spot.
//!
//! Provides convenient aliases for [`Kucoin`](super::super::super::Kucoin) trade streams.

use crate::{
    transformer::stateless::StatelessTransformer,
    subscription::trade::PublicTrades,
    ExchangeWsStream,
};
use super::super::super::Kucoin;

pub use super::super::trade::KucoinTrade;

/// [`ExchangeTransformer`](crate::transformer::ExchangeTransformer) used to
/// convert Kucoin WebSocket trade messages into [`PublicTrade`](PublicTrades)
/// events.
pub type KucoinSpotTradesTransformer<InstrumentKey> =
    StatelessTransformer<Kucoin, InstrumentKey, PublicTrades, KucoinTrade>;

/// Type alias for a Kucoin Spot trades WebSocket stream.
pub type KucoinSpotTradesStream<InstrumentKey> =
    ExchangeWsStream<KucoinSpotTradesTransformer<InstrumentKey>>;
