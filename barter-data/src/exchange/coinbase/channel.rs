use super::Coinbase;
use crate::{
    Identifier,
    subscription::{
        Subscription,
        trade::PublicTrades,
        book::OrderBooksL2,
    },
};
use serde::Serialize;

/// Type that defines how to translate a Jackbot [`Subscription`] into a
/// [`Coinbase`] channel to be subscribed to.
///
/// See docs: <https://docs.cloud.coinbase.com/exchange/docs/websocket-overview#subscribe>
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize)]
pub struct CoinbaseChannel(pub &'static str);

impl CoinbaseChannel {
    /// [`Coinbase`] real-time trades channel.
    ///
    /// See docs: <https://docs.cloud.coinbase.com/exchange/docs/websocket-channels#match>
    pub const TRADES: Self = Self("matches");

    /// [`Coinbase`] OrderBook Level2 channel.
    pub const ORDER_BOOK_L2: Self = Self("level2");
}

impl<Instrument> Identifier<CoinbaseChannel> for Subscription<Coinbase, Instrument, PublicTrades> {
    fn id(&self) -> CoinbaseChannel {
        CoinbaseChannel::TRADES
    }
}

impl<Instrument> Identifier<CoinbaseChannel> for Subscription<Coinbase, Instrument, OrderBooksL2> {
    fn id(&self) -> CoinbaseChannel {
        CoinbaseChannel::ORDER_BOOK_L2
    }
}

impl AsRef<str> for CoinbaseChannel {
    fn as_ref(&self) -> &str {
        self.0
    }
}
