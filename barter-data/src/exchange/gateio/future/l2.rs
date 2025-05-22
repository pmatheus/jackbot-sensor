use super::super::{channel::GateioChannel, market::GateioMarket};
use super::{GateioFuturesBtc, GateioFuturesUsd};
use crate::{
    books::{OrderBook, Level},
    error::DataError,
    event::{MarketEvent, MarketIter},
    exchange::{subscription::ExchangeSub, Connector},
    instrument::InstrumentData,
    subscription::{
        Map, Subscription,
        book::{OrderBookEvent, OrderBooksL2},
    },
    transformer::ExchangeTransformer,
    Identifier, SnapshotFetcher,
};
use async_trait::async_trait;
use barter_instrument::exchange::ExchangeId;
use barter_integration::{
    error::SocketError,
    protocol::websocket::WsMessage,
    subscription::SubscriptionId,
    Transformer,
};
use chrono::{DateTime, Utc};
use derive_more::Constructor;
use futures_util::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::future::Future;
use tokio::sync::mpsc::UnboundedSender;

/// [`GateioFuturesUsd`] HTTP OrderBook L2 snapshot url.
pub const HTTP_BOOK_L2_SNAPSHOT_URL_GATEIO_FUTURES_USD: &str =
    "https://fx-api.gateio.ws/api/v4/delivery/usdt/order_book";

/// [`GateioFuturesBtc`] HTTP OrderBook L2 snapshot url.
pub const HTTP_BOOK_L2_SNAPSHOT_URL_GATEIO_FUTURES_BTC: &str =
    "https://fx-api.gateio.ws/api/v4/delivery/btc/order_book";

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct GateioLevel {
    #[serde(with = "rust_decimal::serde::str")]
    pub price: rust_decimal::Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: rust_decimal::Decimal,
}

impl From<GateioLevel> for Level {
    fn from(level: GateioLevel) -> Self {
        Self { price: level.price, amount: level.amount }
    }
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct GateioOrderBookL2Snapshot {
    #[serde(rename = "id")]
    pub last_update_id: u64,
    #[serde(default, rename = "t", with = "chrono::serde::ts_milliseconds_option")]
    pub time_exchange: Option<DateTime<Utc>>,
    pub bids: Vec<GateioLevel>,
    pub asks: Vec<GateioLevel>,
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, GateioOrderBookL2Snapshot)>
    for MarketEvent<InstrumentKey, OrderBookEvent>
{
    fn from((exchange, instrument, snapshot): (ExchangeId, InstrumentKey, GateioOrderBookL2Snapshot)) -> Self {
        let time_received = Utc::now();
        Self {
            time_exchange: snapshot.time_exchange.unwrap_or(time_received),
            time_received,
            exchange,
            instrument,
            kind: OrderBookEvent::from(snapshot),
        }
    }
}

impl From<GateioOrderBookL2Snapshot> for OrderBookEvent {
    fn from(snapshot: GateioOrderBookL2Snapshot) -> Self {
        Self::Snapshot(OrderBook::new(
            snapshot.last_update_id,
            snapshot.time_exchange,
            snapshot.bids,
            snapshot.asks,
        ))
    }
}

fn de_ob_l2_subscription_id<'de, D>(deserializer: D) -> Result<SubscriptionId, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    <&str as Deserialize>::deserialize(deserializer)
        .map(|market| ExchangeSub::from((GateioChannel::FUTURE_ORDER_BOOK_L2, market)).id())
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct GateioFuturesOrderBookL2Update {
    #[serde(deserialize_with = "de_ob_l2_subscription_id", rename = "s")]
    pub subscription_id: SubscriptionId,
    #[serde(
        rename = "t",
        deserialize_with = "barter_integration::de::de_u64_epoch_ms_as_datetime_utc",
    )]
    pub time_exchange: DateTime<Utc>,
    #[serde(rename = "u")]
    pub last_update_id: u64,
    pub bids: Vec<GateioLevel>,
    pub asks: Vec<GateioLevel>,
}

impl Identifier<Option<SubscriptionId>> for GateioFuturesOrderBookL2Update {
    fn id(&self) -> Option<SubscriptionId> { Some(self.subscription_id.clone()) }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, GateioFuturesOrderBookL2Update)>
    for MarketIter<InstrumentKey, OrderBookEvent>
{
    fn from((exchange, instrument, update): (ExchangeId, InstrumentKey, GateioFuturesOrderBookL2Update)) -> Self {
        Self(vec![Ok(MarketEvent {
            time_exchange: update.time_exchange,
            time_received: Utc::now(),
            exchange,
            instrument,
            kind: OrderBookEvent::Update(OrderBook::new(
                update.last_update_id,
                Some(update.time_exchange),
                update.bids,
                update.asks,
            )),
        })])
    }
}

#[derive(Debug, Constructor)]
pub struct GateioOrderBookL2Meta<InstrumentKey, Sequencer> {
    pub key: InstrumentKey,
    pub sequencer: Sequencer,
}

#[derive(Debug)]
pub struct GateioFuturesOrderBookL2Sequencer {
    pub last_update_id: u64,
}

impl GateioFuturesOrderBookL2Sequencer {
    pub fn new(last_update_id: u64) -> Self { Self { last_update_id } }

    pub fn validate_sequence(
        &mut self,
        update: GateioFuturesOrderBookL2Update,
    ) -> Result<Option<GateioFuturesOrderBookL2Update>, DataError> {
        if update.last_update_id <= self.last_update_id {
            return Ok(None);
        }
        self.last_update_id = update.last_update_id;
        Ok(Some(update))
    }
}

#[derive(Debug)]
pub struct GateioFuturesOrderBooksL2Transformer<InstrumentKey> {
    instrument_map: Map<GateioOrderBookL2Meta<InstrumentKey, GateioFuturesOrderBookL2Sequencer>>,
}
#[async_trait]
impl<InstrumentKey> ExchangeTransformer<GateioFuturesUsd, InstrumentKey, OrderBooksL2>
    for GateioFuturesOrderBooksL2Transformer<InstrumentKey>
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
                    .find(|s| s.instrument == instrument_key)
                    .ok_or_else(|| DataError::InitialSnapshotMissing(sub_id.clone()))?;
                let OrderBookEvent::Snapshot(snapshot) = &snapshot.kind else {
                    return Err(DataError::InitialSnapshotInvalid(String::from(
                        "expected OrderBookEvent::Snapshot but found OrderBookEvent::Update",
                    )));
                };
                let seq = GateioFuturesOrderBookL2Sequencer::new(snapshot.sequence);
                Ok((sub_id, GateioOrderBookL2Meta::new(instrument_key, seq)))
            })
            .collect::<Result<Map<_>, _>>()?;
        Ok(Self { instrument_map })
    }
}

#[async_trait]
impl<InstrumentKey> ExchangeTransformer<GateioFuturesBtc, InstrumentKey, OrderBooksL2>
    for GateioFuturesOrderBooksL2Transformer<InstrumentKey>
where
    InstrumentKey: Clone + PartialEq + Send + Sync,
{
    async fn init(
        instrument_map: Map<InstrumentKey>,
        initial_snapshots: &[MarketEvent<InstrumentKey, OrderBookEvent>],
        ws: UnboundedSender<WsMessage>,
    ) -> Result<Self, DataError> {
        <Self as ExchangeTransformer<GateioFuturesUsd, InstrumentKey, OrderBooksL2>>::init(
            instrument_map,
            initial_snapshots,
            ws,
        )
        .await
    }
}

impl<InstrumentKey> Transformer for GateioFuturesOrderBooksL2Transformer<InstrumentKey>
where
    InstrumentKey: Clone,
{
    type Error = DataError;
    type Input = GateioFuturesOrderBookL2Update;
    type Output = MarketEvent<InstrumentKey, OrderBookEvent>;
    type OutputIter = Vec<Result<Self::Output, Self::Error>>;

    fn transform(&mut self, input: Self::Input) -> Self::OutputIter {
        let subscription_id = match input.id() { Some(id) => id, None => return vec![] };
        let instrument = match self.instrument_map.find_mut(&subscription_id) {
            Ok(inst) => inst,
            Err(unidentifiable) => return vec![Err(DataError::from(unidentifiable))],
        };
        let valid_update = match instrument.sequencer.validate_sequence(input) {
            Ok(Some(update)) => update,
            Ok(None) => return vec![],
            Err(err) => return vec![Err(err)],
        };
        MarketIter::<InstrumentKey, OrderBookEvent>::from((
            GateioFuturesUsd::ID,
            instrument.key.clone(),
            valid_update,
        ))
        .0
    }
}

#[derive(Debug)]
pub struct GateioFuturesUsdOrderBooksL2SnapshotFetcher;

impl SnapshotFetcher<GateioFuturesUsd, OrderBooksL2> for GateioFuturesUsdOrderBooksL2SnapshotFetcher {
    fn fetch_snapshots<Instrument>(
        subscriptions: &[Subscription<GateioFuturesUsd, Instrument, OrderBooksL2>],
    ) -> impl Future<Output = Result<Vec<MarketEvent<Instrument::Key, OrderBookEvent>>, SocketError>> + Send
    where
        Instrument: InstrumentData,
        Subscription<GateioFuturesUsd, Instrument, OrderBooksL2>: Identifier<GateioMarket>,
    {
        let futures = subscriptions.iter().map(|sub| {
            let market = sub.id();
            let url = format!(
                "{}?contract={}&limit=200",
                HTTP_BOOK_L2_SNAPSHOT_URL_GATEIO_FUTURES_USD,
                market.as_ref()
            );
            async move {
                let snapshot = reqwest::get(url)
                    .await
                    .map_err(SocketError::Http)?
                    .json::<GateioOrderBookL2Snapshot>()
                    .await
                    .map_err(SocketError::Http)?;
                Ok(MarketEvent::from((
                    ExchangeId::GateioFuturesUsd,
                    sub.instrument.key().clone(),
                    snapshot,
                )))
            }
        });
        try_join_all(futures)
    }
}

#[derive(Debug)]
pub struct GateioFuturesBtcOrderBooksL2SnapshotFetcher;

impl SnapshotFetcher<GateioFuturesBtc, OrderBooksL2> for GateioFuturesBtcOrderBooksL2SnapshotFetcher {
    fn fetch_snapshots<Instrument>(
        subscriptions: &[Subscription<GateioFuturesBtc, Instrument, OrderBooksL2>],
    ) -> impl Future<Output = Result<Vec<MarketEvent<Instrument::Key, OrderBookEvent>>, SocketError>> + Send
    where
        Instrument: InstrumentData,
        Subscription<GateioFuturesBtc, Instrument, OrderBooksL2>: Identifier<GateioMarket>,
    {
        let futures = subscriptions.iter().map(|sub| {
            let market = sub.id();
            let url = format!(
                "{}?contract={}&limit=200",
                HTTP_BOOK_L2_SNAPSHOT_URL_GATEIO_FUTURES_BTC,
                market.as_ref()
            );
            async move {
                let snapshot = reqwest::get(url)
                    .await
                    .map_err(SocketError::Http)?
                    .json::<GateioOrderBookL2Snapshot>()
                    .await
                    .map_err(SocketError::Http)?;
                Ok(MarketEvent::from((
                    ExchangeId::GateioFuturesBtc,
                    sub.instrument.key().clone(),
                    snapshot,
                )))
            }
        });
        try_join_all(futures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_de_gateio_futures_order_book_l2_update() {
        let input = r#"{
            \"s\":\"BTC_USDT\",
            \"t\":1600000000000,
            \"u\":100,
            \"bids\":[[\"100\",\"1\"]],
            \"asks\":[[\"101\",\"2\"]]
        }"#;
        assert_eq!(
            serde_json::from_str::<GateioFuturesOrderBookL2Update>(input).unwrap(),
            GateioFuturesOrderBookL2Update {
                subscription_id: SubscriptionId::from("futures.order_book|BTC_USDT"),
                time_exchange: DateTime::from_timestamp_millis(1600000000000).unwrap(),
                last_update_id: 100,
                bids: vec![GateioLevel { price: dec!(100), amount: dec!(1) }],
                asks: vec![GateioLevel { price: dec!(101), amount: dec!(2) }],
            }
        );
    }

    #[test]
    fn test_sequencer_validate_sequence() {
        let mut seq = GateioFuturesOrderBookL2Sequencer::new(10);
        let base = GateioFuturesOrderBookL2Update {
            subscription_id: SubscriptionId::from("futures.order_book|BTC_USDT"),
            time_exchange: DateTime::from_timestamp_millis(0).unwrap(),
            last_update_id: 11,
            bids: vec![],
            asks: vec![],
        };
        assert!(seq.validate_sequence(base.clone()).unwrap().is_some());
        let outdated = GateioFuturesOrderBookL2Update { last_update_id: 11, ..base };
        assert!(seq.validate_sequence(outdated).unwrap().is_none());
    }
}
