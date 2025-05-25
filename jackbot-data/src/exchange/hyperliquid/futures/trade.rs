//! Trade event types for Hyperliquid Futures.
//!
//! Provides convenient aliases for [`Hyperliquid`](super::super::Hyperliquid)
//! futures trade streams.

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
pub type HyperliquidFuturesTradesTransformer<InstrumentKey> =
    StatelessTransformer<Hyperliquid, InstrumentKey, PublicTrades, HyperliquidTrades>;

/// Type alias for a Hyperliquid Futures trades WebSocket stream.
pub type HyperliquidFuturesTradesStream<InstrumentKey> =
    ExchangeWsStream<HyperliquidFuturesTradesTransformer<InstrumentKey>>;
