//! Public trade stream types for Binance Spot.
//!
//! This module exposes a [`StatelessTransformer`](crate::transformer::stateless::StatelessTransformer)
//! based implementation for transforming raw Binance Spot trade messages into
//! normalised [`MarketEvent`](crate::event::MarketEvent)s. It simply re-exports
//! the common [`BinanceTrade`](super::super::trade::BinanceTrade) type and
//! provides convenient type aliases for transformer and stream usage.

use super::BinanceSpot;
use crate::{
    ExchangeWsStream, subscription::trade::PublicTrades,
    transformer::stateless::StatelessTransformer,
};

pub use super::super::trade::BinanceTrade;

/// [`ExchangeTransformer`](crate::transformer::ExchangeTransformer) used to
/// convert Binance Spot WebSocket trade messages into [`PublicTrade`](PublicTrades)
/// events.
pub type BinanceSpotTradesTransformer<InstrumentKey> =
    StatelessTransformer<BinanceSpot, InstrumentKey, PublicTrades, BinanceTrade>;

/// Type alias for a Binance Spot trades WebSocket stream.
pub type BinanceSpotTradesStream<InstrumentKey> =
    ExchangeWsStream<BinanceSpotTradesTransformer<InstrumentKey>>;
