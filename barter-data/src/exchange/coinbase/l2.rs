use super::{channel::CoinbaseChannel, Coinbase};
use crate::{
    Identifier, SnapshotFetcher,
    books::OrderBook,
    error::DataError,
    event::{MarketEvent, MarketIter},
    exchange::{Connector, subscription::ExchangeSub},
    instrument::InstrumentData,
    subscription::{
        Map, Subscription,
        book::{OrderBookEvent, OrderBooksL2},
    },
    transformer::ExchangeTransformer,
};
use async_trait::async_trait;
use barter_instrument::{Side, exchange::ExchangeId};
use barter_integration::{
    Transformer, error::SocketError, protocol::websocket::WsMessage, subscription::SubscriptionId,
    de::extract_next,
};
use chrono::{DateTime, Utc};
use derive_more::Constructor;
use futures_util::future::try_join_all;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::future::Future;
use tokio::sync::mpsc::UnboundedSender;

pub const HTTP_BOOK_L2_SNAPSHOT_URL_COINBASE: &str = "https://api.exchange.coinbase.com";

#[derive(Debug, Constructor)]
pub struct CoinbaseOrderBookL2Meta<InstrumentKey> {
    pub key: InstrumentKey,
    pub sequencer: CoinbaseOrderBookL2Sequencer,
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct CoinbaseOrderBookL2Snapshot {
    pub sequence: u64,
    pub bids: Vec<CoinbaseLevel>,
    pub asks: Vec<CoinbaseLevel>,
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, CoinbaseOrderBookL2Snapshot)>
    for MarketEvent<InstrumentKey, OrderBookEvent>
{
    fn from((exchange, instrument, snapshot): (ExchangeId, InstrumentKey, CoinbaseOrderBookL2Snapshot)) -> Self {
        let time_received = Utc::now();
        Self {
            time_exchange: time_received,
            time_received,
            exchange,
            instrument,
            kind: OrderBookEvent::from(snapshot),
        }
    }
}

impl From<CoinbaseOrderBookL2Snapshot> for OrderBookEvent {
    fn from(snapshot: CoinbaseOrderBookL2Snapshot) -> Self {
        Self::Snapshot(OrderBook::new(snapshot.sequence, None, snapshot.bids, snapshot.asks))
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Serialize)]
pub struct CoinbaseLevel {
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub size: Decimal,
}

impl<'de> Deserialize<'de> for CoinbaseLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SeqVisitor;
        impl<'de> serde::de::Visitor<'de> for SeqVisitor {
            type Value = CoinbaseLevel;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("CoinbaseLevel from sequence [price, size, ...]")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let price = extract_next(&mut seq, "price")?;
                let size = extract_next(&mut seq, "size")?;
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(CoinbaseLevel { price, size })
            }
        }
        deserializer.deserialize_seq(SeqVisitor)
    }
}

impl From<CoinbaseLevel> for crate::books::Level {
    fn from(level: CoinbaseLevel) -> Self {
        Self::new(level.price, level.size)
    }
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct CoinbaseOrderBookL2Update {
    #[serde(alias = "product_id", deserialize_with = "de_ob_l2_subscription_id")]
    pub subscription_id: SubscriptionId,
    pub sequence: u64,
    pub time: DateTime<Utc>,
    pub changes: Vec<CoinbaseChange>,
}

#[derive(Clone, PartialEq, PartialOrd, Debug)]
pub struct CoinbaseChange {
    pub side: Side,
    pub level: CoinbaseLevel,
}

impl<'de> Deserialize<'de> for CoinbaseChange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SeqVisitor;
        impl<'de> serde::de::Visitor<'de> for SeqVisitor {
            type Value = CoinbaseChange;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("CoinbaseChange from sequence [side, price, size]")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let side = extract_next(&mut seq, "side")?;
                let price = extract_next(&mut seq, "price")?;
                let size = extract_next(&mut seq, "size")?;
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(CoinbaseChange { side, level: CoinbaseLevel { price, size } })
            }
        }
        deserializer.deserialize_seq(SeqVisitor)
    }
}

impl Identifier<Option<SubscriptionId>> for CoinbaseOrderBookL2Update {
    fn id(&self) -> Option<SubscriptionId> {
        Some(self.subscription_id.clone())
    }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, CoinbaseOrderBookL2Update)>
    for MarketIter<InstrumentKey, OrderBookEvent>
{
    fn from((exchange, instrument, update): (ExchangeId, InstrumentKey, CoinbaseOrderBookL2Update)) -> Self {
        let (bids, asks): (Vec<_>, Vec<_>) = update
            .changes
            .into_iter()
            .partition(|c| c.side == Side::Buy);
        Self(vec![Ok(MarketEvent {
            time_exchange: update.time,
            time_received: Utc::now(),
            exchange,
            instrument,
            kind: OrderBookEvent::Update(OrderBook::new(
                update.sequence,
                None,
                bids.into_iter().map(|c| c.level),
                asks.into_iter().map(|c| c.level),
            )),
        })])
    }
}

pub fn de_ob_l2_subscription_id<'de, D>(deserializer: D) -> Result<SubscriptionId, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    <&str as Deserialize>::deserialize(deserializer)
        .map(|p| ExchangeSub::from((CoinbaseChannel::ORDER_BOOK_L2, p)).id())
}

#[derive(Debug)]
pub struct CoinbaseOrderBookL2Sequencer {
    pub sequence: u64,
}

impl CoinbaseOrderBookL2Sequencer {
    pub fn new(sequence: u64) -> Self {
        Self { sequence }
    }

    pub fn validate_sequence(
        &mut self,
        update: CoinbaseOrderBookL2Update,
    ) -> Result<Option<CoinbaseOrderBookL2Update>, DataError> {
        if update.sequence <= self.sequence {
            return Ok(None);
        }
        if update.sequence != self.sequence + 1 {
            return Err(DataError::InvalidSequence {
                prev_last_update_id: self.sequence,
                first_update_id: update.sequence,
            });
        }
        self.sequence = update.sequence;
        Ok(Some(update))
    }
}

#[derive(Debug)]
pub struct CoinbaseOrderBooksL2SnapshotFetcher;

impl SnapshotFetcher<Coinbase, OrderBooksL2> for CoinbaseOrderBooksL2SnapshotFetcher {
    fn fetch_snapshots<Instrument>(
        subscriptions: &[Subscription<Coinbase, Instrument, OrderBooksL2>],
    ) -> impl Future<Output = Result<Vec<MarketEvent<Instrument::Key, OrderBookEvent>>, SocketError>> + Send
    where
        Instrument: InstrumentData,
        Subscription<Coinbase, Instrument, OrderBooksL2>: Identifier<CoinbaseMarket>,
    {
        let futures = subscriptions.iter().map(|sub| {
            let market = sub.id();
            let snapshot_url = format!(
                "{}/products/{}/book?level=2",
                HTTP_BOOK_L2_SNAPSHOT_URL_COINBASE,
                market.as_ref()
            );
            async move {
                let snapshot = reqwest::get(snapshot_url)
                    .await
                    .map_err(SocketError::Http)?
                    .json::<CoinbaseOrderBookL2Snapshot>()
                    .await
                    .map_err(SocketError::Http)?;
                Ok(MarketEvent::from((ExchangeId::Coinbase, sub.instrument.key().clone(), snapshot)))
            }
        });
        try_join_all(futures)
    }
}

#[derive(Debug)]
pub struct CoinbaseOrderBooksL2Transformer<InstrumentKey> {
    instrument_map: Map<CoinbaseOrderBookL2Meta<InstrumentKey>>,
}

#[async_trait]
impl<InstrumentKey> ExchangeTransformer<Coinbase, InstrumentKey, OrderBooksL2>
    for CoinbaseOrderBooksL2Transformer<InstrumentKey>
where
    InstrumentKey: Clone + PartialEq + Send + Sync,
{
    async fn init(
        instrument_map: Map<InstrumentKey>,
        initial_snapshots: &[MarketEvent<InstrumentKey, OrderBookEvent>],
        _: UnboundedSender<WsMessage>,
    ) -> Result<Self, DataError> {
        let instrument_map = instrument_map
            .0
            .into_iter()
            .map(|(sub_id, instrument_key)| {
                let snapshot = initial_snapshots
                    .iter()
                    .find(|snapshot| snapshot.instrument == instrument_key)
                    .ok_or_else(|| DataError::InitialSnapshotMissing(sub_id.clone()))?;
                let OrderBookEvent::Snapshot(snapshot) = &snapshot.kind else {
                    return Err(DataError::InitialSnapshotInvalid(String::from("expected OrderBookEvent::Snapshot but found OrderBookEvent::Update")));
                };
                let meta = CoinbaseOrderBookL2Meta::new(
                    instrument_key,
                    CoinbaseOrderBookL2Sequencer::new(snapshot.sequence),
                );
                Ok((sub_id, meta))
            })
            .collect::<Result<Map<_>, _>>()?;
        Ok(Self { instrument_map })
    }
}

impl<InstrumentKey> Transformer for CoinbaseOrderBooksL2Transformer<InstrumentKey>
where
    InstrumentKey: Clone,
{
    type Error = DataError;
    type Input = CoinbaseOrderBookL2Update;
    type Output = MarketEvent<InstrumentKey, OrderBookEvent>;
    type OutputIter = Vec<Result<Self::Output, Self::Error>>;

    fn transform(&mut self, input: Self::Input) -> Self::OutputIter {
        let subscription_id = match input.id() { Some(id) => id, None => return vec![] };
        let instrument = match self.instrument_map.find_mut(&subscription_id) {
            Ok(inst) => inst,
            Err(err) => return vec![Err(DataError::from(err))],
        };
        let valid_update = match instrument.sequencer.validate_sequence(input) {
            Ok(Some(update)) => update,
            Ok(None) => return vec![],
            Err(e) => return vec![Err(e)],
        };
        MarketIter::<InstrumentKey, OrderBookEvent>::from((Coinbase::ID, instrument.key.clone(), valid_update)).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_de_coinbase_order_book_l2_snapshot() {
        let input = r#"{\"sequence\":100,\"bids\":[[\"10101.10\",\"0.50\",\"1\"]],\"asks\":[[\"10102.55\",\"1.0\",\"1\"]]}"#;
        assert_eq!(
            serde_json::from_str::<CoinbaseOrderBookL2Snapshot>(input).unwrap(),
            CoinbaseOrderBookL2Snapshot {
                sequence: 100,
                bids: vec![CoinbaseLevel { price: dec!(10101.10), size: dec!(0.50) }],
                asks: vec![CoinbaseLevel { price: dec!(10102.55), size: dec!(1.0) }],
            }
        );
    }

    #[test]
    fn test_de_coinbase_order_book_l2_update() {
        let input = r#"{\"type\":\"l2update\",\"product_id\":\"ETH-USD\",\"time\":\"2014-11-07T08:19:27.028459Z\",\"sequence\":10,\"changes\":[[\"buy\",\"10101.80\",\"0.1\"],[\"sell\",\"10102.02\",\"0\"]]}"#;
        assert_eq!(
            serde_json::from_str::<CoinbaseOrderBookL2Update>(input).unwrap(),
            CoinbaseOrderBookL2Update {
                subscription_id: SubscriptionId::from("level2|ETH-USD"),
                sequence: 10,
                time: DateTime::from_timestamp_millis(1415357967028).unwrap(),
                changes: vec![
                    CoinbaseChange { side: Side::Buy, level: CoinbaseLevel { price: dec!(10101.80), size: dec!(0.1) } },
                    CoinbaseChange { side: Side::Sell, level: CoinbaseLevel { price: dec!(10102.02), size: dec!(0) } },
                ],
            }
        );
    }

    #[test]
    fn test_sequencer_validate_sequence() {
        let mut seq = CoinbaseOrderBookL2Sequencer::new(1);
        let update = CoinbaseOrderBookL2Update {
            subscription_id: SubscriptionId::from("level2|ETH-USD"),
            sequence: 2,
            time: Utc::now(),
            changes: vec![],
        };
        assert!(seq.validate_sequence(update.clone()).unwrap().is_some());
        assert!(seq.validate_sequence(update).unwrap().is_none());
        let invalid = CoinbaseOrderBookL2Update {
            subscription_id: SubscriptionId::from("level2|ETH-USD"),
            sequence: 4,
            time: Utc::now(),
            changes: vec![],
        };
        assert!(seq.validate_sequence(invalid).is_err());
    }
}
