use super::{Bybit, ExchangeServer};
use crate::ExchangeWsStream;
use crate::event::MarketEvent;
use crate::exchange::StreamSelector;
use crate::instrument::InstrumentData;
use crate::subscription::book::{OrderBooksL2, OrderBookEvent};
use jackbot_instrument::exchange::ExchangeId;
use std::fmt::Display;

/// Level 2 OrderBook types.
pub mod l2;

/// [`BybitSpot`] WebSocket server base url.
///
/// See docs: <https://bybit-exchange.github.io/docs/v5/ws/connect>
pub const WEBSOCKET_BASE_URL_BYBIT_SPOT: &str = "wss://stream.bybit.com/v5/public/spot";

pub mod trade;
/// User WebSocket utilities.
pub mod user_ws;

/// [`Bybit`] spot execution.
pub type BybitSpot = Bybit<BybitServerSpot>;

/// [`Bybit`] spot [`ExchangeServer`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct BybitServerSpot;

impl ExchangeServer for BybitServerSpot {
    const ID: ExchangeId = ExchangeId::BybitSpot;

    fn websocket_url() -> &'static str {
        WEBSOCKET_BASE_URL_BYBIT_SPOT
    }
}

impl<Instrument> StreamSelector<Instrument, OrderBooksL2> for BybitSpot
where
    Instrument: InstrumentData,
{
    type SnapFetcher = l2::BybitSpotOrderBooksL2SnapshotFetcher;
    type Stream = ExchangeWsStream<MarketEvent<Instrument::Key, OrderBookEvent>>;
}

impl Display for BybitSpot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BybitSpot")
    }
}
