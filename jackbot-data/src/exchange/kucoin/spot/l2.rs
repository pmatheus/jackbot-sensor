//! Level 2 order book types for Kucoin Spot.
//!
//! This module parses Kucoin spot WebSocket messages and converts them into
//! Jackbot's canonical [`OrderBook`] representation via the [`Canonicalizer`]
//! trait. Snapshots and deltas can then be persisted using a [`RedisStore`].

use crate::{
    Identifier,
    books::{Canonicalizer, Level, OrderBook, l2_sequencer::L2Sequencer},
    error::DataError,
    event::{MarketEvent, MarketIter},
    exchange::{kucoin::channel::KucoinChannel, subscription::ExchangeSub},
    redis_store::RedisStore,
    subscription::book::{OrderBookEvent, OrderBooksL2},
    error::DataError,
};
use chrono::{DateTime, Utc};
use jackbot_instrument::exchange::ExchangeId;
use jackbot_integration::subscription::SubscriptionId;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::books::l2_sequencer::{HasUpdateIds, L2Sequencer};

/// Kucoin real-time OrderBook Level2 message.
#[derive(Clone, PartialEq, PartialOrd, Debug, Deserialize, Serialize)]
pub struct KucoinOrderBookL2 {
    #[serde(alias = "symbol", deserialize_with = "de_ob_l2_subscription_id")]
    pub subscription_id: SubscriptionId,
    #[serde(default = "Utc::now")]
    pub time: DateTime<Utc>,
    #[serde(alias = "sequenceStart")]
    pub sequence_start: Option<u64>,
    #[serde(alias = "sequenceEnd")]
    pub sequence_end: Option<u64>,
    #[serde(alias = "sequence")]
    pub sequence: Option<u64>,
    #[serde(default, alias = "type")]
    pub kind: Option<String>,
    #[serde(alias = "bids")]
    pub bids: Vec<(Decimal, Decimal)>,
    #[serde(alias = "asks")]
    pub asks: Vec<(Decimal, Decimal)>,
}

impl Identifier<Option<SubscriptionId>> for KucoinOrderBookL2 {
    fn id(&self) -> Option<SubscriptionId> {
        Some(self.subscription_id.clone())
    }
}

impl Canonicalizer for KucoinOrderBookL2 {
    fn canonicalize(&self, timestamp: DateTime<Utc>) -> OrderBook {
        let bids = self.bids.iter().map(|(p, a)| Level::new(*p, *a));
        let asks = self.asks.iter().map(|(p, a)| Level::new(*p, *a));
        // Prefer sequence_end for updates, otherwise sequence for snapshots
        let sequence = self
            .sequence_end
            .or(self.sequence)
            .unwrap_or_default();

        OrderBook::new(sequence, Some(timestamp), bids, asks)
    }
}

impl HasUpdateIds for KucoinOrderBookL2 {
    fn first_update_id(&self) -> u64 {
        self.sequence_start.unwrap_or_default()
    }

    fn last_update_id(&self) -> u64 {
        self.sequence_end.or(self.sequence).unwrap_or_default()
    }
}

/// Sequencer implementation for Kucoin Spot order book.
#[derive(Debug, Clone)]
pub struct KucoinSpotOrderBookL2Sequencer {
    pub last_update_id: u64,
    pub updates_processed: u64,
}

impl L2Sequencer<KucoinOrderBookL2> for KucoinSpotOrderBookL2Sequencer {
    fn new(last_update_id: u64) -> Self {
        Self {
            last_update_id,
            updates_processed: 0,
        }
    }

    fn validate_sequence(
        &mut self,
        update: KucoinOrderBookL2,
    ) -> Result<Option<KucoinOrderBookL2>, DataError> {
        let start = update.first_update_id();
        let end = update.last_update_id();

        if self.updates_processed == 0 {
            if start <= self.last_update_id + 1 && end >= self.last_update_id + 1 {
                self.last_update_id = end;
                self.updates_processed += 1;
                Ok(Some(update))
            } else {
                Err(DataError::InvalidSequence {
                    prev_last_update_id: self.last_update_id,
                    first_update_id: start,
                })
            }
        } else if start == self.last_update_id + 1 {
            self.last_update_id = end;
            self.updates_processed += 1;
            Ok(Some(update))
        } else {
            Err(DataError::InvalidSequence {
                prev_last_update_id: self.last_update_id,
                first_update_id: start,
            })
        }
    }

    fn is_first_update(&self) -> bool {
        self.updates_processed == 0
    }
}

impl KucoinOrderBookL2 {
    /// Persist this order book snapshot to the provided [`RedisStore`].
    pub fn store_snapshot<Store: RedisStore>(&self, store: &Store) {
        let snapshot = self.canonicalize(self.time);
        store.store_snapshot(ExchangeId::Kucoin, self.subscription_id.as_ref(), &snapshot);
    }

    /// Persist this order book update to the provided [`RedisStore`].
    pub fn store_delta<Store: RedisStore>(&self, store: &Store) {
        let delta = OrderBookEvent::Update(self.canonicalize(self.time));
        store.store_delta(ExchangeId::Kucoin, self.subscription_id.as_ref(), &delta);
    }
}

impl<InstrumentKey> From<(ExchangeId, InstrumentKey, KucoinOrderBookL2)>
    for MarketIter<InstrumentKey, OrderBookEvent>
{
    fn from(
        (exchange_id, instrument, book): (ExchangeId, InstrumentKey, KucoinOrderBookL2),
    ) -> Self {
        let order_book = book.canonicalize(book.time);
        let kind = match book.kind.as_deref() {
            Some("snapshot") => OrderBookEvent::Snapshot(order_book),
            _ => OrderBookEvent::Update(order_book),
        };

        Self(vec![Ok(MarketEvent {
            time_exchange: book.time,
            time_received: Utc::now(),
            exchange: exchange_id,
            instrument,
            kind,
        })])
    }
}

/// Deserialize a KucoinOrderBookL2 "symbol" as the associated SubscriptionId.
pub fn de_ob_l2_subscription_id<'de, D>(deserializer: D) -> Result<SubscriptionId, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    <&str as Deserialize>::deserialize(deserializer)
        .map(|market| ExchangeSub::from((KucoinChannel::ORDER_BOOK_L2, market)).id())
}

/// Sequencer implementation for Kucoin spot order books.
#[derive(Debug, Clone)]
pub struct KucoinSpotOrderBookL2Sequencer {
    pub last_update_id: u64,
    pub updates_processed: u64,
}

impl L2Sequencer<KucoinOrderBookL2> for KucoinSpotOrderBookL2Sequencer {
    fn new(last_update_id: u64) -> Self {
        Self {
            last_update_id,
            updates_processed: 0,
        }
    }

    fn validate_sequence(
        &mut self,
        update: KucoinOrderBookL2,
    ) -> Result<Option<KucoinOrderBookL2>, DataError> {
        // Kucoin spot updates currently do not expose sequence numbers
        self.updates_processed += 1;
        Ok(Some(update))
    }

    fn is_first_update(&self) -> bool {
        self.updates_processed == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redis_store::InMemoryStore;
    use rust_decimal_macros::dec;

    #[test]
    fn test_kucoin_order_book_l2() {
        let input = r#"{\"type\":\"snapshot\",\"symbol\":\"BTC-USDT\",\"sequence\":100,\"bids\":[[\"30000.0\",\"1.0\"]],\"asks\":[[\"30010.0\",\"2.0\"]]}"#;
        let book: KucoinOrderBookL2 = serde_json::from_str(input).unwrap();
        assert_eq!(book.bids[0], (dec!(30000.0), dec!(1.0)));
        assert_eq!(book.asks[0], (dec!(30010.0), dec!(2.0)));
        assert_eq!(book.kind.as_deref(), Some("snapshot"));
        assert_eq!(book.sequence, Some(100));
    }

    #[test]
    fn test_store_methods() {
        let store = InMemoryStore::new();
        let book = KucoinOrderBookL2 {
            subscription_id: "BTC-USDT".into(),
            time: Utc::now(),
            kind: Some("snapshot".into()),
            sequence: Some(100),
            bids: vec![(dec!(30000.0), dec!(1.0))],
            asks: vec![(dec!(30010.0), dec!(2.0))],
            sequence_start: None,
            sequence_end: None,
        };
        book.store_snapshot(&store);
        assert!(store.get_snapshot_json(ExchangeId::Kucoin, "BTC-USDT").is_some());

        let delta_book = KucoinOrderBookL2 {
            time: Utc::now(),
            kind: Some("delta".into()),
            sequence_start: Some(101),
            sequence_end: Some(102),
            ..book
        };
        delta_book.store_delta(&store);
        assert_eq!(store.delta_len(ExchangeId::Kucoin, "BTC-USDT"), 1);
    }

    #[test]
    fn test_sequencer_is_first_update() {
        let seq = KucoinSpotOrderBookL2Sequencer::new(10);
        assert!(seq.is_first_update());
    }

    #[test]
    fn test_sequencer_validate_first_update() {
        let mut seq = KucoinSpotOrderBookL2Sequencer::new(100);
        let update = KucoinOrderBookL2 {
            subscription_id: "BTC-USDT".into(),
            time: Utc::now(),
            sequence_start: Some(101),
            sequence_end: Some(110),
            sequence: None,
            kind: Some("delta".into()),
            bids: vec![],
            asks: vec![],
        };
        assert!(seq.validate_sequence(update).is_ok());
    }

    #[test]
    fn test_sequencer_validate_next_update() {
        let mut seq = KucoinSpotOrderBookL2Sequencer {
            last_update_id: 110,
            updates_processed: 1,
        };
        let update = KucoinOrderBookL2 {
            subscription_id: "BTC-USDT".into(),
            time: Utc::now(),
            sequence_start: Some(111),
            sequence_end: Some(120),
            sequence: None,
            kind: Some("delta".into()),
            bids: vec![],
            asks: vec![],
        };
        assert!(seq.validate_sequence(update).is_ok());
    }
}
