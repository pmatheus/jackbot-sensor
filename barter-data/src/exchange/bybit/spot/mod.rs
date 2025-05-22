use super::{Bybit, ExchangeServer};
use crate::{
    ExchangeWsStream,
    exchange::{
        StreamSelector,
        bybit::spot::l2::{
            BybitSpotOrderBooksL2SnapshotFetcher, BybitSpotOrderBooksL2Transformer,
        },
    },
    instrument::InstrumentData,
    subscription::book::OrderBooksL2,
};
use barter_instrument::exchange::ExchangeId;
use std::fmt::Display;

/// Level 2 OrderBook types.
pub mod l2;

/// [`BybitSpot`] WebSocket server base url.
///
/// See docs: <https://bybit-exchange.github.io/docs/v5/ws/connect>
pub const WEBSOCKET_BASE_URL_BYBIT_SPOT: &str = "wss://stream.bybit.com/v5/public/spot";

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
    type SnapFetcher = BybitSpotOrderBooksL2SnapshotFetcher;
    type Stream = ExchangeWsStream<BybitSpotOrderBooksL2Transformer<Instrument::Key>>;
}

impl Display for BybitSpot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BybitSpot")
    }
}
