//! Level 2 order book types for Gate.io futures.
use crate::subscription::book::OrderBookEvent;
use crate::{
    Identifier,
    books::{Canonicalizer, Level, OrderBook},
    event::{MarketEvent, MarketIter},
    redis_store::RedisStore,
};
use chrono::{DateTime, Utc};
use jackbot_instrument::exchange::ExchangeId;
use jackbot_integration::subscription::SubscriptionId;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct GateioFuturesOrderBookL2 {
    #[serde(alias = "symbol")]
    pub subscription_id: SubscriptionId,
    #[serde(default = "Utc::now")]
    pub time: DateTime<Utc>,
    #[serde(alias = "bids")]
    pub bids: Vec<(Decimal, Decimal)>,
    #[serde(alias = "asks")]
    pub asks: Vec<(Decimal, Decimal)>,
}

impl Identifier<Option<SubscriptionId>> for GateioFuturesOrderBookL2 {
    fn id(&self) -> Option<SubscriptionId> {
        Some(self.subscription_id.clone())
    }
}

impl Canonicalizer for GateioFuturesOrderBookL2 {
    fn canonicalize(&self, timestamp: DateTime<Utc>) -> OrderBook {
        let bids = self.bids.iter().map(|(p, a)| Level::new(*p, *a));
        let asks = self.asks.iter().map(|(p, a)| Level::new(*p, *a));
        OrderBook::new(0, Some(timestamp), bids, asks)
    }
}

impl GateioFuturesOrderBookL2 {
    /// Persist this order book snapshot to the provided [`RedisStore`].
    pub fn store_snapshot<Store: RedisStore>(&self, store: &Store) {
        let snapshot = self.canonicalize(self.time);
        store.store_snapshot(ExchangeId::Gateio, self.subscription_id.as_ref(), &snapshot);
    }

    /// Persist this order book update to the provided [`RedisStore`].
    pub fn store_delta<Store: RedisStore>(&self, store: &Store) {
        let delta = OrderBookEvent::Update(self.canonicalize(self.time));
        store.store_delta(ExchangeId::Gateio, self.subscription_id.as_ref(), &delta);
    }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, GateioFuturesOrderBookL2)>
    for MarketIter<InstrumentKey, OrderBookEvent>
{
    fn from(
        (exchange_id, instrument, book): (ExchangeId, InstrumentKey, GateioFuturesOrderBookL2),
    ) -> Self {
        let order_book = book.canonicalize(book.time);
        Self(vec![Ok(MarketEvent {
            time_exchange: book.time,
            time_received: Utc::now(),
            exchange: exchange_id,
            instrument,
            kind: OrderBookEvent::Update(order_book),
        })])
    }
}
