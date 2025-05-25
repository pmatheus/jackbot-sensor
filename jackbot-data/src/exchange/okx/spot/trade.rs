//! Trade event types for Okx Spot.
//!
//! Provides convenient aliases for [`Okx`](super::super::super::Okx) trade streams.

use crate::{
    transformer::stateless::StatelessTransformer,
    subscription::trade::PublicTrades,
    ExchangeWsStream,
};
use super::super::Okx;

pub use super::super::trade::OkxTrades;

/// [`ExchangeTransformer`](crate::transformer::ExchangeTransformer) used to
/// convert Okx WebSocket trade messages into [`PublicTrade`](PublicTrades) events.
pub type OkxSpotTradesTransformer<InstrumentKey> =
    StatelessTransformer<Okx, InstrumentKey, PublicTrades, OkxTrades>;

/// Type alias for an Okx Spot trades WebSocket stream.
pub type OkxSpotTradesStream<InstrumentKey> =
    ExchangeWsStream<OkxSpotTradesTransformer<InstrumentKey>>;
