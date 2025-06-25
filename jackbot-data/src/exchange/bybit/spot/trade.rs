//! Public trade stream types for Bybit Spot.
//!
//! Provides a [`StatelessTransformer`](crate::transformer::stateless::StatelessTransformer)
//! implementation for converting raw Bybit WebSocket trade messages into
//! normalised [`MarketEvent`](crate::event::MarketEvent)s.

use super::BybitSpot;
use crate::{
    ExchangeWsStream, subscription::trade::PublicTrades,
    transformer::stateless::StatelessTransformer,
};

pub use super::super::{message::BybitMessage, trade::BybitTrade};

/// [`ExchangeTransformer`](crate::transformer::ExchangeTransformer) used to
/// convert Bybit Spot WebSocket trade messages into [`PublicTrade`](PublicTrades)
/// events.
pub type BybitSpotTradesTransformer<InstrumentKey> =
    StatelessTransformer<BybitSpot, InstrumentKey, PublicTrades, BybitMessage>;

/// Type alias for a Bybit Spot trades WebSocket stream.
pub type BybitSpotTradesStream<InstrumentKey> =
    ExchangeWsStream<BybitSpotTradesTransformer<InstrumentKey>>;
