use self::{l2::GateioSpotOrderBooksL2SnapshotFetcher, l2::GateioSpotOrderBooksL2Transformer, trade::GateioSpotTrade};
use super::Gateio;
use crate::{
    ExchangeWsStream, NoInitialSnapshots,
    exchange::{ExchangeServer, StreamSelector},
    instrument::InstrumentData,
    subscription::trade::PublicTrades,
    transformer::stateless::StatelessTransformer,
};
use barter_instrument::exchange::ExchangeId;
use barter_macro::{DeExchange, SerExchange};
use std::fmt::Display;

/// Public trades types.
pub mod trade;
pub mod l2;

/// [`GateioSpot`] WebSocket server base url.
///
/// See docs: <https://www.gate.io/docs/developers/apiv4/ws/en/>
pub const WEBSOCKET_BASE_URL_GATEIO_SPOT: &str = "wss://api.gateio.ws/ws/v4/";

/// [`Gateio`] spot execution.
pub type GateioSpot = Gateio<GateioServerSpot>;

/// [`Gateio`] spot [`ExchangeServer`].
#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, DeExchange, SerExchange,
)]
pub struct GateioServerSpot;

impl ExchangeServer for GateioServerSpot {
    const ID: ExchangeId = ExchangeId::GateioSpot;

    fn websocket_url() -> &'static str {
        WEBSOCKET_BASE_URL_GATEIO_SPOT
    }
}

impl<Instrument> StreamSelector<Instrument, PublicTrades> for GateioSpot
where
    Instrument: InstrumentData,
{
    type SnapFetcher = NoInitialSnapshots;
    type Stream = ExchangeWsStream<
        StatelessTransformer<Self, Instrument::Key, PublicTrades, GateioSpotTrade>,
    >;
}

impl<Instrument> StreamSelector<Instrument, OrderBooksL2> for GateioSpot
where
    Instrument: InstrumentData,
{
    type SnapFetcher = GateioSpotOrderBooksL2SnapshotFetcher;
    type Stream = ExchangeWsStream<GateioSpotOrderBooksL2Transformer<Instrument::Key>>;
}

impl Display for GateioSpot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GateioSpot")
    }
}
