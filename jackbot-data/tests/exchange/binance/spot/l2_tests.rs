// filepath: /Users/user/jackbot/jackbot-sensor/jackbot-data/tests/exchange/binance/spot/l2_tests.rs
use jackbot_data::{
    books::Level,
    exchange::binance::{
        book::l2::BinanceLevel,
        spot::l2::{BinanceSpotOrderBookL2Update, BinanceSpotOrderBookL2Sequencer},
    },
    subscription::SubscriptionId,
    books::l2_sequencer::L2Sequencer,
    books::OrderBook,
    event::OrderBookEvent,
};
use chrono::{DateTime, Utc};
use rust_decimal_macros::dec;

#[test]
fn test_de_binance_spot_order_book_l2_update() {
    let input = r#"{
        \"e\": \"depthUpdate\",
        \"E\": 1571889248277,
        \"s\": \"BTCUSDT\",
        \"U\": 157,
        \"u\": 160,
        \"b\": [[\"0.0024\", \"10\"]],
        \"a\": [[\"0.0026\", \"100\"]]
    }"#;

    assert_eq!(
        serde_json::from_str::<BinanceSpotOrderBookL2Update>(input).unwrap(),
        BinanceSpotOrderBookL2Update {
            subscription_id: SubscriptionId::from("@depth@100ms|BTCUSDT"),
            time_exchange: DateTime::from_timestamp_millis(1571889248277).unwrap(),
            first_update_id: 157,
            last_update_id: 160,
            bids: vec![BinanceLevel {
                price: dec!(0.0024),
                amount: dec!(10)
            }],
            asks: vec![BinanceLevel {
                price: dec!(0.0026),
                amount: dec!(100)
            }],
        }
    );
}

#[test]
fn test_sequencer_is_first_update() {
    let sequencer =
        <BinanceSpotOrderBookL2Sequencer as L2Sequencer<BinanceSpotOrderBookL2Update>>::new(10);
    // Fresh sequencer should be first update
    assert!(<BinanceSpotOrderBookL2Sequencer as L2Sequencer<
        BinanceSpotOrderBookL2Update,
    >>::is_first_update(&sequencer));

    // After processing an update, it should no longer be first
    let mut sequencer =
        <BinanceSpotOrderBookL2Sequencer as L2Sequencer<BinanceSpotOrderBookL2Update>>::new(10);
    let update = BinanceSpotOrderBookL2Update {
        first_update_id: 11, // U <= lastUpdateId+1 -> true
        last_update_id: 11,  // u >= lastUpdateId+1 -> true
        subscription_id: SubscriptionId::from("ETHBTC@depth5"),
        time_exchange: Utc::now(),
        bids: vec![],
        asks: vec![],
    };
    // Process an update
    let _result = sequencer.validate_sequence(update).unwrap();
    // Now it should no longer be the first update
    assert!(!<BinanceSpotOrderBookL2Sequencer as L2Sequencer<
        BinanceSpotOrderBookL2Update,
    >>::is_first_update(&sequencer));
}

#[test]
fn test_sequencer_validate_first_update() {
    // Snapshot: last_update_id = 10
    // Update:   first_update_id = 11, last_update_id = 11
    let sequencer =
        <BinanceSpotOrderBookL2Sequencer as L2Sequencer<BinanceSpotOrderBookL2Update>>::new(10);
    let update = BinanceSpotOrderBookL2Update {
        first_update_id: 11,
        last_update_id: 11,
        subscription_id: SubscriptionId::from("ETHBTC@depth5"),
        time_exchange: Utc::now(),
        bids: vec![],
        asks: vec![],
    };
    assert!(sequencer.validate_first_update(&update).is_ok());

    let sequencer =
        <BinanceSpotOrderBookL2Sequencer as L2Sequencer<BinanceSpotOrderBookL2Update>>::new(10);
    let invalid = BinanceSpotOrderBookL2Update {
        first_update_id: 90,
        last_update_id: 95,
        subscription_id: SubscriptionId::from("ETHBTC@depth5"),
        time_exchange: Utc::now(),
        bids: vec![],
        asks: vec![],
    };
    assert!(sequencer.validate_first_update(&invalid).is_err());
}

#[test]
fn test_sequencer_validate_next_update() {
    let sequencer = BinanceSpotOrderBookL2Sequencer {
        updates_processed: 10,
        last_update_id: 100,
        prev_last_update_id: 100,
    };
    let ok_update = BinanceSpotOrderBookL2Update {
        subscription_id: SubscriptionId::from("id"),
        time_exchange: Default::default(),
        first_update_id: 101,
        last_update_id: 110,
        bids: vec![],
        asks: vec![],
    };
    assert!(sequencer.validate_next_update(&ok_update).is_ok());

    let bad_update = BinanceSpotOrderBookL2Update {
        first_update_id: 105,
        last_update_id: 110,
        subscription_id: SubscriptionId::from("id"),
        time_exchange: Default::default(),
        bids: vec![],
        asks: vec![],
    };
    assert!(sequencer.validate_next_update(&bad_update).is_err());
}

#[test]
fn test_update_order_book_with_sequenced_updates() {
    let mut sequencer = BinanceSpotOrderBookL2Sequencer {
        updates_processed: 100,
        last_update_id: 100,
        prev_last_update_id: 100,
    };
    let mut book = OrderBook::new(100, None, vec![Level::new(dec!(80), dec!(1))], vec![Level::new(dec!(120), dec!(1))]);

    let update = BinanceSpotOrderBookL2Update {
        subscription_id: SubscriptionId::from("id"),
        time_exchange: Default::default(),
        first_update_id: 101,
        last_update_id: 110,
        bids: vec![BinanceLevel {
            price: dec!(80),
            amount: dec!(0),
        }],
        asks: vec![BinanceLevel {
            price: dec!(130),
            amount: dec!(1),
        }],
    };

    if let Some(valid_update) = sequencer.validate_sequence(update).unwrap() {
        let event = OrderBookEvent::Update(OrderBook::new(
            valid_update.last_update_id(),
            None,
            valid_update.bids.into_iter().map(|l| Level::new(l.price, l.amount)),
            valid_update.asks.into_iter().map(|l| Level::new(l.price, l.amount)),
        ));
        book.update(event);
    }

    assert_eq!(book.sequence, 110);
    assert!(book.bids().levels().is_empty());
    assert_eq!(book.asks().levels()[1].price, dec!(130));
}
