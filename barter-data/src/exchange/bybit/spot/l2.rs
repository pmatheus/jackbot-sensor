use super::super::super::market::BybitMarket;
use super::super::channel::BybitChannel;
use crate::{
    Identifier, SnapshotFetcher,
    books::OrderBook,
    error::DataError,
    event::{MarketEvent, MarketIter},
    exchange::{
        Connector,
        bybit::spot::BybitSpot,
    },
    instrument::InstrumentData,
    subscription::{
        Map, Subscription,
        book::{OrderBookEvent, OrderBooksL2},
    },
    transformer::ExchangeTransformer,
};
use async_trait::async_trait;
use barter_instrument::exchange::ExchangeId;
use barter_integration::{
    Transformer, error::SocketError, protocol::websocket::WsMessage, subscription::SubscriptionId,
};
use chrono::{DateTime, Utc};
use futures_util::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::future::Future;
use tokio::sync::mpsc::UnboundedSender;
use rust_decimal::Decimal;

/// [`BybitSpot`] HTTP OrderBook L2 snapshot url.
pub const HTTP_BOOK_L2_SNAPSHOT_URL_BYBIT_SPOT: &str = "https://api.bybit.com/v5/market/orderbook";

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct BybitLevel {
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

impl From<BybitLevel> for crate::books::Level {
    fn from(level: BybitLevel) -> Self {
        Self { price: level.price, amount: level.amount }
    }
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct BybitOrderBookL2Snapshot {
    #[serde(alias = "u")]
    pub sequence: u64,
    #[serde(
        alias = "ts",
        deserialize_with = "barter_integration::de::de_u64_epoch_ms_as_datetime_utc",
    )]
    pub time_exchange: DateTime<Utc>,
    #[serde(alias = "b")]
    pub bids: Vec<BybitLevel>,
    #[serde(alias = "a")]
    pub asks: Vec<BybitLevel>,
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, BybitOrderBookL2Snapshot)> for MarketEvent<InstrumentKey, OrderBookEvent> {
    fn from((exchange, instrument, snapshot): (ExchangeId, InstrumentKey, BybitOrderBookL2Snapshot)) -> Self {
        let time_received = Utc::now();
        Self {
            time_exchange: snapshot.time_exchange,
            time_received,
            exchange,
            instrument,
            kind: OrderBookEvent::Snapshot(OrderBook::new(
                snapshot.sequence,
                None,
                snapshot.bids,
                snapshot.asks,
            )),
        }
    }
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct BybitSpotOrderBookL2UpdatePayload {
    #[serde(alias = "u")]
    pub sequence: u64,
    #[serde(alias = "b")]
    pub bids: Vec<BybitLevel>,
    #[serde(alias = "a")]
    pub asks: Vec<BybitLevel>,
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct BybitSpotOrderBookL2Update {
    #[serde(alias = "topic", deserialize_with = "de_ob_l2_subscription_id")]
    pub subscription_id: SubscriptionId,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(
        alias = "ts",
        deserialize_with = "barter_integration::de::de_u64_epoch_ms_as_datetime_utc",
    )]
    pub time_exchange: DateTime<Utc>,
    pub data: BybitSpotOrderBookL2UpdatePayload,
}

impl Identifier<Option<SubscriptionId>> for BybitSpotOrderBookL2Update {
    fn id(&self) -> Option<SubscriptionId> { Some(self.subscription_id.clone()) }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, BybitSpotOrderBookL2Update)> for MarketIter<InstrumentKey, OrderBookEvent> {
    fn from((exchange_id, instrument, update): (ExchangeId, InstrumentKey, BybitSpotOrderBookL2Update)) -> Self {
        Self(vec![Ok(MarketEvent {
            time_exchange: update.time_exchange,
            time_received: Utc::now(),
            exchange: exchange_id,
            instrument,
            kind: OrderBookEvent::Update(OrderBook::new(
                update.data.sequence,
                None,
                update.data.bids,
                update.data.asks,
            )),
        })])
    }
}

pub fn de_ob_l2_subscription_id<'de, D>(deserializer: D) -> Result<SubscriptionId, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let input = <&str as serde::Deserialize>::deserialize(deserializer)?;
    let mut tokens = input.split('.');
    match (tokens.next(), tokens.next(), tokens.next()) {
        (Some("orderbook"), Some(_), Some(market)) => Ok(SubscriptionId::from(format!("{}|{}", BybitChannel::ORDER_BOOK_L2.0, market))),
        _ => Err(serde::de::Error::custom("invalid message topic")),
    }
}

#[derive(Debug)]
pub struct BybitSpotOrderBookL2Sequencer { pub last_sequence: u64 }

impl BybitSpotOrderBookL2Sequencer {
    pub fn new(sequence: u64) -> Self { Self { last_sequence: sequence } }

    pub fn validate_sequence(&mut self, update: BybitSpotOrderBookL2Update) -> Result<Option<BybitSpotOrderBookL2Update>, DataError> {
        if update.data.sequence <= self.last_sequence { return Ok(None); }
        if update.data.sequence != self.last_sequence + 1 {
            return Err(DataError::InvalidSequence { prev_last_update_id: self.last_sequence, first_update_id: update.data.sequence });
        }
        self.last_sequence = update.data.sequence;
        Ok(Some(update))
    }
}

#[derive(Debug)]
pub struct BybitSpotOrderBooksL2SnapshotFetcher;

impl SnapshotFetcher<BybitSpot, OrderBooksL2> for BybitSpotOrderBooksL2SnapshotFetcher {
    fn fetch_snapshots<Instrument>(
        subscriptions: &[Subscription<BybitSpot, Instrument, OrderBooksL2>],
    ) -> impl Future<Output = Result<Vec<MarketEvent<Instrument::Key, OrderBookEvent>>, SocketError>> + Send
    where
        Instrument: InstrumentData,
        Subscription<BybitSpot, Instrument, OrderBooksL2>: Identifier<BybitMarket>,
    {
        let futs = subscriptions.iter().map(|sub| {
            let market = sub.id();
            let url = format!("{}?category=spot&symbol={}&limit=200", HTTP_BOOK_L2_SNAPSHOT_URL_BYBIT_SPOT, market.as_ref());
            async move {
                let resp = reqwest::get(url).await.map_err(SocketError::Http)?;
                let value = resp.json::<serde_json::Value>().await.map_err(SocketError::Http)?;
                let data = value.get("result").cloned().unwrap_or(value);
                let snapshot: BybitOrderBookL2Snapshot = serde_json::from_value(data).map_err(SocketError::Serde)?;
                Ok(MarketEvent::from((ExchangeId::BybitSpot, sub.instrument.key().clone(), snapshot)))
            }
        });
        try_join_all(futs)
    }
}

#[derive(Debug)]
pub struct BybitSpotOrderBooksL2Transformer<InstrumentKey> {
    instrument_map: Map<BybitOrderBookL2Meta<InstrumentKey, BybitSpotOrderBookL2Sequencer>>,
}

#[derive(Debug)]
pub struct BybitOrderBookL2Meta<InstrumentKey, Sequencer> { pub key: InstrumentKey, pub sequencer: Sequencer }

impl<InstrumentKey, Sequencer> BybitOrderBookL2Meta<InstrumentKey, Sequencer> {
    pub fn new(key: InstrumentKey, sequencer: Sequencer) -> Self { Self { key, sequencer } }
}

#[async_trait]
impl<InstrumentKey> ExchangeTransformer<BybitSpot, InstrumentKey, OrderBooksL2> for BybitSpotOrderBooksL2Transformer<InstrumentKey>
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
                    return Err(DataError::InitialSnapshotInvalid("expected snapshot".into()));
                };
                Ok((sub_id, BybitOrderBookL2Meta::new(instrument_key, BybitSpotOrderBookL2Sequencer::new(snapshot.sequence))))
            })
            .collect::<Result<Map<_>, _>>()?;
        Ok(Self { instrument_map })
    }
}

impl<InstrumentKey> Transformer for BybitSpotOrderBooksL2Transformer<InstrumentKey>
where
    InstrumentKey: Clone,
{
    type Error = DataError;
    type Input = BybitSpotOrderBookL2Update;
    type Output = MarketEvent<InstrumentKey, OrderBookEvent>;
    type OutputIter = Vec<Result<Self::Output, Self::Error>>;

    fn transform(&mut self, input: Self::Input) -> Self::OutputIter {
        let sub_id = match input.id() { Some(id) => id, None => return vec![] };
        let instrument = match self.instrument_map.find_mut(&sub_id) {
            Ok(inst) => inst,
            Err(e) => return vec![Err(DataError::from(e))],
        };
        let valid = match instrument.sequencer.validate_sequence(input) {
            Ok(Some(update)) => update,
            Ok(None) => return vec![],
            Err(e) => return vec![Err(e)],
        };
        MarketIter::<InstrumentKey, OrderBookEvent>::from((BybitSpot::ID, instrument.key.clone(), valid)).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books::Level;
    use rust_decimal_macros::dec;

    #[test]
    fn test_de_bybit_spot_order_book_l2_update() {
        let input = r#"{\"topic\":\"orderbook.50.BTCUSDT\",\"type\":\"delta\",\"ts\":1000,\"data\":{\"u\":2,\"b\":[[\"100\",\"1\"]],\"a\":[]}}"#;
        let parsed: BybitSpotOrderBookL2Update = serde_json::from_str(input).unwrap();
        assert_eq!(parsed.subscription_id, SubscriptionId::from("orderbook|BTCUSDT"));
        assert_eq!(parsed.data.sequence, 2);
        assert_eq!(parsed.data.bids, vec![BybitLevel { price: dec!(100), amount: dec!(1) }]);
    }

    #[test]
    fn test_sequencer_validate_sequence() {
        let mut seq = BybitSpotOrderBookL2Sequencer::new(1);
        let update = BybitSpotOrderBookL2Update {
            subscription_id: SubscriptionId::from("orderbook|BTCUSDT"),
            r#type: "delta".into(),
            time_exchange: DateTime::from_timestamp_millis(0).unwrap(),
            data: BybitSpotOrderBookL2UpdatePayload { sequence: 2, bids: vec![], asks: vec![] },
        };
        assert!(seq.validate_sequence(update).is_ok());
    }

    #[test]
    fn test_update_jackbot_order_book_with_sequenced_updates() {
        let mut seq = BybitSpotOrderBookL2Sequencer::new(1);
        let mut book = OrderBook::new(1, None, vec![Level::new(50,1)], vec![Level::new(100,1)]);
        let update = BybitSpotOrderBookL2Update {
            subscription_id: SubscriptionId::from("orderbook|BTCUSDT"),
            r#type: "delta".into(),
            time_exchange: DateTime::from_timestamp_millis(0).unwrap(),
            data: BybitSpotOrderBookL2UpdatePayload {
                sequence: 2,
                bids: vec![BybitLevel { price: dec!(80), amount: dec!(0) }],
                asks: vec![BybitLevel { price: dec!(110), amount: dec!(2) }],
            },
        };
        if let Some(valid) = seq.validate_sequence(update).unwrap() {
            book.update(OrderBookEvent::Update(OrderBook::new(valid.data.sequence, None, valid.data.bids, valid.data.asks)));
        }
        assert_eq!(book, OrderBook::new(2, None, vec![Level::new(100,1)], vec![Level::new(110,2)]));
    }
}

