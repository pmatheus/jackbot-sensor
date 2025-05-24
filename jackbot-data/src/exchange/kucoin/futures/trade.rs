//! Trade event types for Kucoin Futures.
//!
//! Provides convenient aliases for [`Kucoin`](super::super::super::Kucoin) futures trade streams.

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
pub type KucoinFuturesTradesTransformer<InstrumentKey> =
    StatelessTransformer<Kucoin, InstrumentKey, PublicTrades, KucoinTrade>;

/// Type alias for a Kucoin Futures trades WebSocket stream.
pub type KucoinFuturesTradesStream<InstrumentKey> =
    ExchangeWsStream<KucoinFuturesTradesTransformer<InstrumentKey>>;
