use crate::{
    Identifier,
    exchange::bybit::Bybit,
    subscription::{
        Subscription,
        trade::PublicTrades,
        book::OrderBooksL2,
    },
};
use serde::Serialize;

/// Type that defines how to translate a Jackbot [`Subscription`] into a [`Bybit`]
/// channel to be subscribed to.
///
/// See docs: <https://bybit-exchange.github.io/docs/v5/ws/connect>
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
pub struct BybitChannel(pub &'static str);

impl BybitChannel {
    /// [`Bybit`] real-time trades channel name.
    ///
    /// See docs: <https://bybit-exchange.github.io/docs/v5/websocket/public/trade>
    pub const TRADES: Self = Self("publicTrade");

    /// [`Bybit`] OrderBook Level2 channel name.
    pub const ORDER_BOOK_L2: Self = Self("orderbook");
}

impl<Server, Instrument> Identifier<BybitChannel>
    for Subscription<Bybit<Server>, Instrument, PublicTrades>
{
    fn id(&self) -> BybitChannel {
        BybitChannel::TRADES
    }
}

impl<Server, Instrument> Identifier<BybitChannel>
    for Subscription<Bybit<Server>, Instrument, OrderBooksL2>
{
    fn id(&self) -> BybitChannel {
        BybitChannel::ORDER_BOOK_L2
    }
}

impl AsRef<str> for BybitChannel {
    fn as_ref(&self) -> &str {
        self.0
    }
}
