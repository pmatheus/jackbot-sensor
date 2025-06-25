//! Trade event types for Coinbase Spot.
//!
//! Provides type aliases for working with [`Coinbase`](super::super::super::Coinbase)
//! trade WebSocket streams.

use super::super::Coinbase;
use crate::{
    ExchangeWsStream, subscription::trade::PublicTrades,
    transformer::stateless::StatelessTransformer,
};

pub use super::super::trade::CoinbaseTrade;

/// [`ExchangeTransformer`](crate::transformer::ExchangeTransformer) used to
/// convert Coinbase WebSocket trade messages into [`PublicTrade`](PublicTrades)
/// events.
pub type CoinbaseSpotTradesTransformer<InstrumentKey> =
    StatelessTransformer<Coinbase, InstrumentKey, PublicTrades, CoinbaseTrade>;

/// Type alias for a Coinbase Spot trades WebSocket stream.
pub type CoinbaseSpotTradesStream<InstrumentKey> =
    ExchangeWsStream<CoinbaseSpotTradesTransformer<InstrumentKey>>;
