//! Trade event types for Hyperliquid Spot.
//!
//! Provides convenient aliases for [`Hyperliquid`](super::super::super::Hyperliquid)
//! trade streams.

use crate::{
    transformer::stateless::StatelessTransformer,
    subscription::trade::PublicTrades,
    ExchangeWsStream,
};
use super::super::Hyperliquid;

pub use super::super::trade::HyperliquidTrades;

/// [`ExchangeTransformer`](crate::transformer::ExchangeTransformer) used to
/// convert Hyperliquid WebSocket trade messages into [`PublicTrade`](PublicTrades)
/// events.
pub type HyperliquidSpotTradesTransformer<InstrumentKey> =
    StatelessTransformer<Hyperliquid, InstrumentKey, PublicTrades, HyperliquidTrades>;

/// Type alias for a Hyperliquid Spot trades WebSocket stream.
pub type HyperliquidSpotTradesStream<InstrumentKey> =
    ExchangeWsStream<HyperliquidSpotTradesTransformer<InstrumentKey>>;
