use super::super::message::GateioMessage;
use crate::{
    Identifier, SnapshotFetcher,
    books::OrderBook,
    error::DataError,
    event::{MarketEvent, MarketIter},
    exchange::{Gateio, gateio::market::GateioMarket, Connector},
    instrument::InstrumentData,
    subscription::{
        Map, Subscription,
        book::{OrderBookEvent, OrderBooksL2},
    },
    transformer::ExchangeTransformer,
};
use async_trait::async_trait;
use barter_instrument::exchange::ExchangeId;
use barter_integration::{Transformer, error::SocketError, protocol::websocket::WsMessage, subscription::SubscriptionId};
use chrono::{DateTime, Utc};
use futures_util::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::future::Future;
use tokio::sync::mpsc::UnboundedSender;
use rust_decimal::Decimal;

pub const HTTP_BOOK_L2_SNAPSHOT_URL_GATEIO_SPOT: &str = "https://api.gateio.ws/api/v4/spot/order_book";

#[derive(Debug)]
pub struct GateioSpotOrderBooksL2SnapshotFetcher;

impl SnapshotFetcher<Gateio<super::GateioServerSpot>, OrderBooksL2> for GateioSpotOrderBooksL2SnapshotFetcher {
    fn fetch_snapshots<Instrument>(subs: &[Subscription<Gateio<super::GateioServerSpot>, Instrument, OrderBooksL2>]) -> impl Future<Output = Result<Vec<MarketEvent<Instrument::Key, OrderBookEvent>>, SocketError>> + Send
    where
        Instrument: InstrumentData,
        Subscription<Gateio<super::GateioServerSpot>, Instrument, OrderBooksL2>: Identifier<GateioMarket>,
    {
        let futs = subs.iter().map(|sub| {
            let market = sub.id();
            let url = format!("{}?currency_pair={}&limit=200", HTTP_BOOK_L2_SNAPSHOT_URL_GATEIO_SPOT, market.as_ref());
            async move {
                let snapshot = reqwest::get(url)
                    .await
                    .map_err(SocketError::Http)?
                    .json::<GateioOrderBookL2Snapshot>()
                    .await
                    .map_err(SocketError::Http)?;
                Ok(MarketEvent::from((ExchangeId::GateioSpot, sub.instrument.key().clone(), snapshot)))
            }
        });
        try_join_all(futs)
    }
}

#[derive(Debug)]
pub struct GateioSpotOrderBooksL2Transformer<InstrumentKey> {
    instrument_map: Map<GateioOrderBookL2Meta<InstrumentKey, GateioSpotOrderBookL2Sequencer>>,
}

#[async_trait]
impl<InstrumentKey> ExchangeTransformer<Gateio<super::GateioServerSpot>, InstrumentKey, OrderBooksL2>
    for GateioSpotOrderBooksL2Transformer<InstrumentKey>
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
            .map(|(sub_id, key)| {
                let snapshot = initial_snapshots
                    .iter()
                    .find(|s| s.instrument == key)
                    .ok_or_else(|| DataError::InitialSnapshotMissing(sub_id.clone()))?;
                let OrderBookEvent::Snapshot(snapshot) = &snapshot.kind else {
                    return Err(DataError::InitialSnapshotInvalid("expected OrderBookEvent::Snapshot but found OrderBookEvent::Update".into()));
                };
                Ok((sub_id, GateioOrderBookL2Meta::new(key, GateioSpotOrderBookL2Sequencer::new(snapshot.sequence))))
            })
            .collect::<Result<Map<_>, _>>()?;
        Ok(Self { instrument_map })
    }
}

impl<InstrumentKey> Transformer for GateioSpotOrderBooksL2Transformer<InstrumentKey>
where
    InstrumentKey: Clone,
{
    type Error = DataError;
    type Input = GateioSpotOrderBookL2;
    type Output = MarketEvent<InstrumentKey, OrderBookEvent>;
    type OutputIter = Vec<Result<Self::Output, Self::Error>>;

    fn transform(&mut self, input: Self::Input) -> Self::OutputIter {
        let update = match input.data {
            GateioSpotOrderBookL2Inner::Update(u) => u,
            GateioSpotOrderBookL2Inner::Other => return vec![],
        };
        let sub_id = match update.id() { Some(id) => id, None => return vec![] };
        let instrument = match self.instrument_map.find_mut(&sub_id) {
            Ok(i) => i,
            Err(e) => return vec![Err(DataError::from(e))],
        };
        let update = match instrument.sequencer.validate_sequence(update) {
            Ok(Some(u)) => u,
            Ok(None) => return vec![],
            Err(e) => return vec![Err(e)],
        };
        MarketIter::<InstrumentKey, OrderBookEvent>::from((ExchangeId::GateioSpot, instrument.key.clone(), update)).0
    }
}
#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct GateioOrderBookL2Snapshot {
    #[serde(alias = "id")]
    pub sequence: u64,
    #[serde(alias = "bids")]
    pub bids: Vec<GateioLevel>,
    #[serde(alias = "asks")]
    pub asks: Vec<GateioLevel>,
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, GateioOrderBookL2Snapshot)> for MarketEvent<InstrumentKey, OrderBookEvent> {
    fn from((exchange, instrument, snapshot): (ExchangeId, InstrumentKey, GateioOrderBookL2Snapshot)) -> Self {
        let time_received = Utc::now();
        Self {
            time_exchange: time_received,
            time_received,
            exchange,
            instrument,
            kind: OrderBookEvent::Snapshot(OrderBook::new(snapshot.sequence, None, snapshot.bids, snapshot.asks)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Deserialize, Serialize)]
pub struct GateioLevel {
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

impl From<GateioLevel> for crate::books::Level {
    fn from(l: GateioLevel) -> Self { Self { price: l.price, amount: l.amount } }
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct GateioSpotOrderBookL2Update {
    #[serde(alias = "s", deserialize_with = "de_ob_l2_subscription_id")]
    pub subscription_id: SubscriptionId,
    #[serde(alias = "t", deserialize_with = "barter_integration::de::de_u64_epoch_ms_as_datetime_utc")]
    pub time_exchange: DateTime<Utc>,
    #[serde(alias = "U")]
    pub first_update_id: u64,
    #[serde(alias = "u")]
    pub last_update_id: u64,
    #[serde(alias = "b")]
    pub bids: Vec<GateioLevel>,
    #[serde(alias = "a")]
    pub asks: Vec<GateioLevel>,
}

type GateioSpotOrderBookL2 = GateioMessage<GateioSpotOrderBookL2Inner>;

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GateioSpotOrderBookL2Inner {
    Update(GateioSpotOrderBookL2Update),
    Other,
}

pub fn de_ob_l2_subscription_id<'de, D>(deserializer: D) -> Result<SubscriptionId, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    <&str as Deserialize>::deserialize(deserializer)
        .map(|m| crate::exchange::subscription::ExchangeSub::from((super::super::channel::GateioChannel::SPOT_ORDER_BOOK_L2, m)).id())
}

impl Identifier<Option<SubscriptionId>> for GateioSpotOrderBookL2Update {
    fn id(&self) -> Option<SubscriptionId> { Some(self.subscription_id.clone()) }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, GateioSpotOrderBookL2Update)> for MarketIter<InstrumentKey, OrderBookEvent> {
    fn from((exchange, instrument, update): (ExchangeId, InstrumentKey, GateioSpotOrderBookL2Update)) -> Self {
        Self(vec![Ok(MarketEvent {
            time_exchange: update.time_exchange,
            time_received: Utc::now(),
            exchange,
            instrument,
            kind: OrderBookEvent::Update(OrderBook::new(update.last_update_id, None, update.bids, update.asks)),
        })])
    }
}

#[derive(Debug)]
pub struct GateioSpotOrderBookL2Sequencer {
    pub updates_processed: u64,
    pub last_update_id: u64,
    pub prev_last_update_id: u64,
}

impl GateioSpotOrderBookL2Sequencer {
    pub fn new(last_update_id: u64) -> Self {
        Self { updates_processed: 0, last_update_id, prev_last_update_id: last_update_id }
    }

    pub fn validate_sequence(&mut self, update: GateioSpotOrderBookL2Update) -> Result<Option<GateioSpotOrderBookL2Update>, DataError> {
        if update.last_update_id <= self.last_update_id { return Ok(None); }
        if self.is_first_update() {
            self.validate_first_update(&update)?;
        } else {
            self.validate_next_update(&update)?;
        }
        self.updates_processed += 1;
        self.prev_last_update_id = self.last_update_id;
        self.last_update_id = update.last_update_id;
        Ok(Some(update))
    }

    pub fn is_first_update(&self) -> bool { self.updates_processed == 0 }

    pub fn validate_first_update(&self, update: &GateioSpotOrderBookL2Update) -> Result<(), DataError> {
        let expected = self.last_update_id + 1;
        if update.first_update_id <= expected && update.last_update_id >= expected {
            Ok(())
        } else {
            Err(DataError::InvalidSequence { prev_last_update_id: self.last_update_id, first_update_id: update.first_update_id })
        }
    }

    pub fn validate_next_update(&self, update: &GateioSpotOrderBookL2Update) -> Result<(), DataError> {
        let expected = self.last_update_id + 1;
        if update.first_update_id == expected {
            Ok(())
        } else {
            Err(DataError::InvalidSequence { prev_last_update_id: self.last_update_id, first_update_id: update.first_update_id })
        }
    }
}

#[derive(Debug)]
pub struct GateioOrderBookL2Meta<InstrumentKey, Sequencer> {
    pub key: InstrumentKey,
    pub sequencer: Sequencer,
}

impl<InstrumentKey, Sequencer> GateioOrderBookL2Meta<InstrumentKey, Sequencer> {
    pub fn new(key: InstrumentKey, sequencer: Sequencer) -> Self { Self { key, sequencer } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books::Level;
    use rust_decimal_macros::dec;

    #[test]
    fn test_de_gateio_spot_order_book_l2_update() {
        let input = r#"{
            \"s\": \"ETH_USDT\",
            \"t\": 1671656397761,
            \"U\": 22611425143,
            \"u\": 22611425151,
            \"b\": [[\"1209.67000000\",\"85.48210000\"],[\"1209.66000000\",\"20.68790000\"]],
            \"a\": []
        }"#;
        assert_eq!(serde_json::from_str::<GateioSpotOrderBookL2Update>(input).unwrap(), GateioSpotOrderBookL2Update {
            subscription_id: SubscriptionId::from("spot.order_book_update|ETH_USDT"),
            time_exchange: DateTime::from_timestamp_millis(1671656397761).unwrap(),
            first_update_id: 22611425143,
            last_update_id: 22611425151,
            bids: vec![GateioLevel { price: dec!(1209.67000000), amount: dec!(85.48210000) }, GateioLevel { price: dec!(1209.66000000), amount: dec!(20.68790000) }],
            asks: vec![]
        });
    }

    #[test]
    fn test_sequencer_is_first_update() {
        assert!(GateioSpotOrderBookL2Sequencer::new(10).is_first_update());
        assert!(!GateioSpotOrderBookL2Sequencer { updates_processed: 1, last_update_id: 10, prev_last_update_id: 9 }.is_first_update());
    }

    #[test]
    fn test_update_jackbot_order_book_with_sequenced_updates() {
        let mut sequencer = GateioSpotOrderBookL2Sequencer { updates_processed: 0, last_update_id: 100, prev_last_update_id: 100 };
        let mut book = OrderBook::new(100, None, vec![Level::new(80, 1)], vec![Level::new(150, 1)]);
        let update = GateioSpotOrderBookL2Update {
            subscription_id: SubscriptionId::from("spot.order_book_update|ETH_USDT"),
            time_exchange: Default::default(),
            first_update_id: 101,
            last_update_id: 110,
            bids: vec![GateioLevel { price: dec!(90), amount: dec!(10) }],
            asks: vec![GateioLevel { price: dec!(200), amount: dec!(1) }],
        };
        if let Some(valid) = sequencer.validate_sequence(update).unwrap() {
            let evt = OrderBookEvent::Update(OrderBook::new(valid.last_update_id, None, valid.bids, valid.asks));
            book.update(evt);
        }
        assert_eq!(book, OrderBook::new(110, None, vec![Level::new(80, 1), Level::new(90, 10)], vec![Level::new(150, 1), Level::new(200, 1)]));
    }
}
