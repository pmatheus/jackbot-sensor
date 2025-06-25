use jackbot_data::exchange::gate::spot::l2::*;
use jackbot_data::redis_store::InMemoryStore;
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal_macros::dec;
use chrono::Utc;

#[test]
fn test_gate_spot_order_book_l2() {
    let input = r#"{\"currency_pair\":\"BTC_USDT\",\"bids\":[[\"30000.0\",\"1.0\"]],\"asks\":[[\"30010.0\",\"2.0\"]]}"#;
    let book: GateOrderBookL2 = serde_json::from_str(input).unwrap();
    assert_eq!(book.bids[0], (dec!(30000.0), dec!(1.0)));
    assert_eq!(book.asks[0], (dec!(30010.0), dec!(2.0)));
}

#[test]
fn test_store_methods() {
    let store = InMemoryStore::new();
    let book = GateOrderBookL2 {
        subscription_id: "BTC_USDT".into(),
        time: Utc::now(),
        bids: vec![(dec!(30000.0), dec!(1.0))],
        asks: vec![(dec!(30010.0), dec!(2.0))],
    };
    book.store_snapshot(&store);
    assert!(store.get_snapshot(ExchangeId::Gateio, "BTC_USDT").is_some());

    let delta_book = GateOrderBookL2 { time: Utc::now(), ..book };
    delta_book.store_delta(&store);
    assert_eq!(store.delta_len(ExchangeId::Gateio, "BTC_USDT"), 1);
}
