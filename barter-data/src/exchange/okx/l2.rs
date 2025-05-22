use super::{market::OkxMarket, Okx};
use crate::{
    Identifier, SnapshotFetcher,
    books::{OrderBook, Level},
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
use barter_instrument::exchange::ExchangeId;
use barter_integration::{
    Transformer, error::SocketError, protocol::websocket::WsMessage,
    subscription::SubscriptionId,
};
use chrono::{DateTime, Utc};
use derive_more::Constructor;
use futures_util::future::try_join_all;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::future::Future;
use tokio::sync::mpsc::UnboundedSender;

/// [`Okx`] HTTP OrderBook L2 snapshot url.
///
/// See docs: <https://www.okx.com/docs-v5/en/#order-book-trading-market-books>
pub const HTTP_BOOK_L2_SNAPSHOT_URL_OKX: &str = "https://www.okx.com/api/v5/market/books";

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct OkxLevel {
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

impl From<OkxLevel> for Level {
    fn from(level: OkxLevel) -> Self {
        Self { price: level.price, amount: level.amount }
    }
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxOrderBookL2Snapshot {
    #[serde(rename = "seqId")]
    pub seq_id: u64,
    #[serde(default, rename = "prevSeqId")]
    pub prev_seq_id: u64,
    #[serde(
        rename = "ts",
        deserialize_with = "barter_integration::de::de_str_u64_epoch_ms_as_datetime_utc",
    )]
    pub time_exchange: DateTime<Utc>,
    pub bids: Vec<OkxLevel>,
    pub asks: Vec<OkxLevel>,
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, OkxOrderBookL2Snapshot)>
    for MarketEvent<InstrumentKey, OrderBookEvent>
{
    fn from((exchange, instrument, snapshot): (ExchangeId, InstrumentKey, OkxOrderBookL2Snapshot)) -> Self {
        let time_received = Utc::now();
        Self {
            time_exchange: snapshot.time_exchange,
            time_received,
            exchange,
            instrument,
            kind: OrderBookEvent::Snapshot(OrderBook::new(
                snapshot.seq_id,
                None,
                snapshot.bids,
                snapshot.asks,
            )),
        }
    }
}

#[derive(Deserialize)]
struct RestSnapshotResp {
    data: Vec<OkxOrderBookL2Snapshot>,
}

#[derive(Debug)]
pub struct OkxOrderBooksL2SnapshotFetcher;

impl SnapshotFetcher<Okx, OrderBooksL2> for OkxOrderBooksL2SnapshotFetcher {
    fn fetch_snapshots<Instrument>(
        subscriptions: &[Subscription<Okx, Instrument, OrderBooksL2>],
    ) -> impl Future<Output = Result<Vec<MarketEvent<Instrument::Key, OrderBookEvent>>, SocketError>> + Send
    where
        Instrument: InstrumentData,
        Subscription<Okx, Instrument, OrderBooksL2>: Identifier<OkxMarket>,
    {
        let futs = subscriptions.iter().map(|sub| {
            let market = sub.id();
            let url = format!("{}?instId={}&sz=400", HTTP_BOOK_L2_SNAPSHOT_URL_OKX, market.as_ref());
            async move {
                let resp = reqwest::get(url).await.map_err(SocketError::Http)?;
                let snapshot: RestSnapshotResp = resp.json().await.map_err(SocketError::Http)?;
                let snap = snapshot.data.into_iter().next().ok_or_else(|| SocketError::GetMessage("snapshot missing".into()))?;
                Ok(MarketEvent::from((ExchangeId::Okx, sub.instrument.key().clone(), snap)))
            }
        });
        try_join_all(futs)
    }
}

#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct OkxOrderBookL2Update {
    #[serde(
        rename = "arg",
        deserialize_with = "de_okx_message_arg_as_subscription_id",
    )]
    pub subscription_id: SubscriptionId,
    pub action: String,
    pub data: Vec<OkxOrderBookL2Snapshot>,
}

impl Identifier<Option<SubscriptionId>> for OkxOrderBookL2Update {
    fn id(&self) -> Option<SubscriptionId> {
        Some(self.subscription_id.clone())
    }
}

impl<InstrumentKey: Clone> From<(ExchangeId, InstrumentKey, OkxOrderBookL2Update)>
    for MarketIter<InstrumentKey, OrderBookEvent>
{
    fn from((exchange_id, instrument, update): (ExchangeId, InstrumentKey, OkxOrderBookL2Update)) -> Self {
        update
            .data
            .into_iter()
            .map(|delta| {
                Ok(MarketEvent {
                    time_exchange: delta.time_exchange,
                    time_received: Utc::now(),
                    exchange: exchange_id,
                    instrument: instrument.clone(),
                    kind: if update.action == "snapshot" {
                        OrderBookEvent::Snapshot(OrderBook::new(delta.seq_id, None, delta.bids, delta.asks))
                    } else {
                        OrderBookEvent::Update(OrderBook::new(delta.seq_id, None, delta.bids, delta.asks))
                    },
                })
            })
            .collect()
    }
}

#[derive(Debug, Constructor)]
pub struct OkxOrderBookL2Meta<InstrumentKey, Sequencer> {
    pub key: InstrumentKey,
    pub sequencer: Sequencer,
}

#[derive(Debug)]
pub struct OkxOrderBookL2Sequencer {
    pub updates_processed: u64,
    pub last_seq_id: u64,
}

impl OkxOrderBookL2Sequencer {
    pub fn new(seq_id: u64) -> Self {
        Self { updates_processed: 0, last_seq_id: seq_id }
    }

    pub fn validate_sequence(
        &mut self,
        mut update: OkxOrderBookL2Update,
    ) -> Result<Option<OkxOrderBookL2Update>, DataError> {
        let Some(mut data) = update.data.into_iter().next() else { return Ok(None); };

        if data.seq_id < self.last_seq_id {
            return Ok(None);
        }

        if self.updates_processed == 0 {
            if data.prev_seq_id != self.last_seq_id {
                return Err(DataError::InvalidSequence {
                    prev_last_update_id: self.last_seq_id,
                    first_update_id: data.prev_seq_id,
                });
            }
        } else if data.prev_seq_id != self.last_seq_id {
            return Err(DataError::InvalidSequence {
                prev_last_update_id: self.last_seq_id,
                first_update_id: data.prev_seq_id,
            });
        }

        self.updates_processed += 1;
        self.last_seq_id = data.seq_id;
        update.data = vec![data];
        Ok(Some(update))
    }
}

#[derive(Debug)]
pub struct OkxOrderBooksL2Transformer<InstrumentKey> {
    instrument_map: Map<OkxOrderBookL2Meta<InstrumentKey, OkxOrderBookL2Sequencer>>,
}

#[async_trait]
impl<InstrumentKey> ExchangeTransformer<Okx, InstrumentKey, OrderBooksL2>
    for OkxOrderBooksL2Transformer<InstrumentKey>
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
                    return Err(DataError::InitialSnapshotInvalid(String::from(
                        "expected OrderBookEvent::Snapshot but found OrderBookEvent::Update",
                    )));
                };

                let meta = OkxOrderBookL2Meta::new(
                    instrument_key,
                    OkxOrderBookL2Sequencer::new(snapshot.sequence),
                );

                Ok((sub_id, meta))
            })
            .collect::<Result<Map<_>, _>>()?;

        Ok(Self { instrument_map })
    }
}

impl<InstrumentKey> Transformer for OkxOrderBooksL2Transformer<InstrumentKey>
where
    InstrumentKey: Clone,
{
    type Error = DataError;
    type Input = OkxOrderBookL2Update;
    type Output = MarketEvent<InstrumentKey, OrderBookEvent>;
    type OutputIter = Vec<Result<Self::Output, Self::Error>>;

    fn transform(&mut self, input: Self::Input) -> Self::OutputIter {
        let subscription_id = match input.id() {
            Some(id) => id,
            None => return vec![],
        };

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
            Okx::ID,
            instrument.key.clone(),
            valid_update,
        ))
        .0
    }
}

fn de_okx_message_arg_as_subscription_id<'de, D>(deserializer: D) -> Result<SubscriptionId, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Arg<'a> {
        channel: &'a str,
        inst_id: &'a str,
    }

    Deserialize::deserialize(deserializer)
        .map(|arg: Arg<'_>| ExchangeSub::from((arg.channel, arg.inst_id)).id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_de_okx_order_book_l2_update() {
        let input = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "update",
            "data": [{
                "seqId": 2,
                "prevSeqId": 1,
                "ts": "1630048897000",
                "bids": [["41000", "1"]],
                "asks": [["41001", "2"]]
            }]
        }"#;

        let expected = OkxOrderBookL2Update {
            subscription_id: SubscriptionId::from("books|BTC-USDT"),
            action: "update".to_string(),
            data: vec![OkxOrderBookL2Snapshot {
                seq_id: 2,
                prev_seq_id: 1,
                time_exchange: DateTime::from_timestamp_millis(1630048897000).unwrap(),
                bids: vec![OkxLevel { price: dec!(41000), amount: dec!(1) }],
                asks: vec![OkxLevel { price: dec!(41001), amount: dec!(2) }],
            }],
        };

        let actual: OkxOrderBookL2Update = serde_json::from_str(input).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_sequencer_validate_sequence() {
        let mut seq = OkxOrderBookL2Sequencer::new(1);
        let update = OkxOrderBookL2Update {
            subscription_id: SubscriptionId::from("id"),
            action: "update".into(),
            data: vec![OkxOrderBookL2Snapshot {
                seq_id: 2,
                prev_seq_id: 1,
                time_exchange: Utc::now(),
                bids: vec![],
                asks: vec![],
            }],
        };

        assert!(seq.validate_sequence(update.clone()).unwrap().is_some());
        let invalid = OkxOrderBookL2Update {
            subscription_id: SubscriptionId::from("id"),
            action: "update".into(),
            data: vec![OkxOrderBookL2Snapshot {
                seq_id: 3,
                prev_seq_id: 1,
                time_exchange: Utc::now(),
                bids: vec![],
                asks: vec![],
            }],
        };
        assert!(seq.validate_sequence(invalid).is_err());
    }
}

