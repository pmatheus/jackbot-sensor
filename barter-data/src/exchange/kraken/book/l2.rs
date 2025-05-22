use super::super::KrakenMessage;
use crate::{
    books::{OrderBook, Level},
    event::{MarketEvent, MarketIter},
    exchange::{kraken::channel::KrakenChannel, subscription::ExchangeSub},
    exchange::kraken::market::KrakenMarket,
    subscription::{book::OrderBookEvent, Map, Subscription},
    transformer::ExchangeTransformer,
    SnapshotFetcher, Identifier, instrument::InstrumentData,
};
use barter_instrument::exchange::ExchangeId;
use barter_integration::{
    de::extract_next, subscription::SubscriptionId, protocol::websocket::WsMessage,
    Transformer, error::SocketError,
};
use chrono::Utc;
use derive_more::Constructor;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use std::future::Future;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Constructor)]
pub struct KrakenOrderBookL2Meta<InstrumentKey, Sequencer> {
    pub key: InstrumentKey,
    pub sequencer: Sequencer,
}

#[derive(Debug)]
pub struct KrakenOrderBooksL2SnapshotFetcher;

impl SnapshotFetcher<super::super::Kraken, OrderBooksL2> for KrakenOrderBooksL2SnapshotFetcher {
    fn fetch_snapshots<Instrument>(
        _: &[Subscription<super::super::Kraken, Instrument, OrderBooksL2>],
    ) -> impl Future<Output = Result<Vec<MarketEvent<Instrument::Key, OrderBookEvent>>, SocketError>> + Send
    where
        Instrument: InstrumentData,
        Subscription<super::super::Kraken, Instrument, OrderBooksL2>: Identifier<KrakenMarket>,
    {
        std::future::ready(Ok(vec![]))
    }
}

#[derive(Debug)]
pub struct KrakenOrderBooksL2Transformer<InstrumentKey> {
    instrument_map: Map<KrakenOrderBookL2Meta<InstrumentKey, KrakenOrderBookL2Sequencer>>,
}

#[async_trait]
impl<InstrumentKey> ExchangeTransformer<super::super::Kraken, InstrumentKey, OrderBooksL2>
    for KrakenOrderBooksL2Transformer<InstrumentKey>
where
    InstrumentKey: Clone + PartialEq + Send + Sync,
{
    async fn init(
        instrument_map: Map<InstrumentKey>,
        _initial_snapshots: &[MarketEvent<InstrumentKey, OrderBookEvent>],
        _: UnboundedSender<WsMessage>,
    ) -> Result<Self, crate::error::DataError> {
        let instrument_map = instrument_map
            .0
            .into_iter()
            .map(|(sub_id, instrument_key)| {
                Ok((
                    sub_id,
                    KrakenOrderBookL2Meta::new(instrument_key, KrakenOrderBookL2Sequencer::default()),
                ))
            })
            .collect::<Result<Map<_>, _>>()?;

        Ok(Self { instrument_map })
    }
}

impl<InstrumentKey> Transformer for KrakenOrderBooksL2Transformer<InstrumentKey>
where
    InstrumentKey: Clone,
{
    type Error = crate::error::DataError;
    type Input = KrakenOrderBookL2;
    type Output = MarketEvent<InstrumentKey, OrderBookEvent>;
    type OutputIter = Vec<Result<Self::Output, Self::Error>>;

    fn transform(&mut self, input: Self::Input) -> Self::OutputIter {
        let data = match input {
            KrakenMessage::Data(data) => data,
            KrakenMessage::Event(_) => return vec![],
        };

        let subscription_id = match data.id() {
            Some(id) => id,
            None => return vec![],
        };

        let instrument = match self.instrument_map.find_mut(&subscription_id) {
            Ok(instr) => instr,
            Err(unidentifiable) => return vec![Err(crate::error::DataError::from(unidentifiable))],
        };

        let valid = match instrument.sequencer.validate_sequence(data) {
            Ok(Some(v)) => v,
            Ok(None) => return vec![],
            Err(e) => return vec![Err(e)],
        };

        MarketIter::<InstrumentKey, OrderBookEvent>::from((
            super::super::Kraken::ID,
            instrument.key.clone(),
            valid,
        ))
        .0
    }
}

/// Terse type alias for a [`Kraken`] real-time OrderBook Level2 WebSocket message.
pub type KrakenOrderBookL2 = KrakenMessage<KrakenOrderBookL2Inner>;

#[derive(Clone, PartialEq, PartialOrd, Debug, Serialize)]
#[serde(tag = "kind")]
pub enum KrakenOrderBookL2Inner {
    Snapshot {
        subscription_id: SubscriptionId,
        sequence: u64,
        bids: Vec<KrakenLevel>,
        asks: Vec<KrakenLevel>,
    },
    Update {
        subscription_id: SubscriptionId,
        sequence: u64,
        bids: Vec<KrakenLevel>,
        asks: Vec<KrakenLevel>,
    },
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct KrakenLevel {
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

impl From<KrakenLevel> for Level {
    fn from(level: KrakenLevel) -> Self {
        Self {
            price: level.price,
            amount: level.amount,
        }
    }
}

impl Identifier<Option<SubscriptionId>> for KrakenOrderBookL2Inner {
    fn id(&self) -> Option<SubscriptionId> {
        match self {
            Self::Snapshot { subscription_id, .. } => Some(subscription_id.clone()),
            Self::Update { subscription_id, .. } => Some(subscription_id.clone()),
        }
    }
}

impl<'de> Deserialize<'de> for KrakenOrderBookL2Inner {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SeqVisitor;

        impl<'de> serde::de::Visitor<'de> for SeqVisitor {
            type Value = KrakenOrderBookL2Inner;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("KrakenOrderBookL2Inner struct from the Kraken WebSocket API")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                // Format: [channelID, {data}, channelName, pair]
                let _: serde::de::IgnoredAny = extract_next(&mut seq, "channelID")?;
                let data: serde_json::Value = extract_next(&mut seq, "data")?;
                let _: serde::de::IgnoredAny = extract_next(&mut seq, "channelName")?;
                let pair = extract_next::<A, String>(&mut seq, "pair")?;
                let subscription_id = ExchangeSub::from((KrakenChannel::ORDER_BOOK_L2, pair)).id();

                let sequence = data
                    .get("c")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_default();

                let bids = if let Some(levels) = data.get("bs").or_else(|| data.get("b")) {
                    serde_json::from_value::<Vec<KrakenLevel>>(levels.clone()).map_err(serde::de::Error::custom)?
                } else {
                    vec![]
                };

                let asks = if let Some(levels) = data.get("as").or_else(|| data.get("a")) {
                    serde_json::from_value::<Vec<KrakenLevel>>(levels.clone()).map_err(serde::de::Error::custom)?
                } else {
                    vec![]
                };

                let kind = if data.get("as").is_some() || data.get("bs").is_some() {
                    KrakenOrderBookL2Inner::Snapshot {
                        subscription_id,
                        sequence,
                        bids,
                        asks,
                    }
                } else {
                    KrakenOrderBookL2Inner::Update {
                        subscription_id,
                        sequence,
                        bids,
                        asks,
                    }
                };

                Ok(kind)
            }
        }

        deserializer.deserialize_seq(SeqVisitor)
    }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, KrakenOrderBookL2Inner)>
    for MarketIter<InstrumentKey, OrderBookEvent>
{
    fn from((exchange, instrument, book): (ExchangeId, InstrumentKey, KrakenOrderBookL2Inner)) -> Self {
        match book {
            KrakenOrderBookL2Inner::Snapshot { sequence, bids, asks, .. } => {
                vec![Ok(MarketEvent {
                    time_exchange: Utc::now(),
                    time_received: Utc::now(),
                    exchange,
                    instrument,
                    kind: OrderBookEvent::Snapshot(OrderBook::new(sequence, None, bids, asks)),
                })]
            }
            KrakenOrderBookL2Inner::Update { sequence, bids, asks, .. } => {
                vec![Ok(MarketEvent {
                    time_exchange: Utc::now(),
                    time_received: Utc::now(),
                    exchange,
                    instrument,
                    kind: OrderBookEvent::Update(OrderBook::new(sequence, None, bids, asks)),
                })]
            }
        }
        .into()
    }
}

#[derive(Debug, Default)]
pub struct KrakenOrderBookL2Sequencer {
    pub last_sequence: u64,
}

impl KrakenOrderBookL2Sequencer {
    pub fn new(sequence: u64) -> Self {
        Self { last_sequence: sequence }
    }

    pub fn validate_sequence(
        &mut self,
        update: KrakenOrderBookL2Inner,
    ) -> Result<Option<KrakenOrderBookL2Inner>, crate::error::DataError> {
        let sequence = match &update {
            KrakenOrderBookL2Inner::Snapshot { sequence, .. } => *sequence,
            KrakenOrderBookL2Inner::Update { sequence, .. } => *sequence,
        };

        if sequence <= self.last_sequence {
            return Ok(None);
        }

        if self.last_sequence != 0 && sequence != self.last_sequence + 1 {
            return Err(crate::error::DataError::InvalidSequence {
                prev_last_update_id: self.last_sequence,
                first_update_id: sequence,
            });
        }

        self.last_sequence = sequence;
        Ok(Some(update))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_de_kraken_order_book_l2_snapshot() {
        let input = r#"
            [
                0,
                {"as": [["1.0","0.5"],["2.0","1.0"]], "bs": [["0.9","0.3"],["0.8","0.4"], ["0.7","0.2"]], "c":1},
                "book",
                "XBT/USD"
            ]
        "#;
        let expected = KrakenOrderBookL2Inner::Snapshot {
            subscription_id: SubscriptionId::from("book|XBT/USD"),
            sequence: 1,
            bids: vec![
                KrakenLevel { price: dec!(0.9), amount: dec!(0.3) },
                KrakenLevel { price: dec!(0.8), amount: dec!(0.4) },
                KrakenLevel { price: dec!(0.7), amount: dec!(0.2) },
            ],
            asks: vec![
                KrakenLevel { price: dec!(1.0), amount: dec!(0.5) },
                KrakenLevel { price: dec!(2.0), amount: dec!(1.0) },
            ],
        };
        assert_eq!(serde_json::from_str::<KrakenOrderBookL2>(input).unwrap(), KrakenMessage::Data(expected));
    }

    #[test]
    fn test_sequencer_validate_sequence() {
        let mut seq = KrakenOrderBookL2Sequencer::new(0);
        let update = KrakenOrderBookL2Inner::Update {
            subscription_id: SubscriptionId::from("book|XBT/USD"),
            sequence: 1,
            bids: vec![],
            asks: vec![],
        };
        assert!(seq.validate_sequence(update.clone()).unwrap().is_some());
        let invalid = KrakenOrderBookL2Inner::Update {
            subscription_id: SubscriptionId::from("book|XBT/USD"),
            sequence: 3,
            bids: vec![],
            asks: vec![],
        };
        assert!(seq.validate_sequence(invalid).is_err());
    }
}

