//! Trade event types for Okx Spot.
//!
//! Provides convenient aliases for [`Okx`](super::super::super::Okx) trade streams.

use super::super::Okx;
use crate::{
    ExchangeWsStream, subscription::trade::PublicTrades,
    transformer::stateless::StatelessTransformer,
};

pub use super::super::trade::OkxTrades;

/// [`ExchangeTransformer`](crate::transformer::ExchangeTransformer) used to
/// convert Okx WebSocket trade messages into [`PublicTrade`](PublicTrades) events.
pub type OkxSpotTradesTransformer<InstrumentKey> =
    StatelessTransformer<Okx, InstrumentKey, PublicTrades, OkxTrades>;

/// Type alias for an Okx Spot trades WebSocket stream.
pub type OkxSpotTradesStream<InstrumentKey> =
    ExchangeWsStream<OkxSpotTradesTransformer<InstrumentKey>>;
