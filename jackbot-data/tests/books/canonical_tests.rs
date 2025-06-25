use jackbot_data::books::canonical::*;
use jackbot_data::books::{Level, OrderBook};
use chrono::Utc;
use rust_decimal_macros::dec;

#[test]
fn test_canonical_orderbook_creation() {
    let bids = vec![
        Level::new(dec!(1000.0), dec!(1.5)),
        Level::new(dec!(999.0), dec!(2.0)),
    ];
    let asks = vec![
        Level::new(dec!(1001.0), dec!(1.0)),
        Level::new(dec!(1002.0), dec!(3.0)),
    ];

    let orderbook = OrderBook::new(123, Some(Utc::now()), bids, asks);
    let canonical = CanonicalOrderBook::from(orderbook);

    assert_eq!(canonical.inner().sequence, 123);
    assert_eq!(canonical.inner().bids().levels()[0].price, dec!(1000.0));
    assert_eq!(canonical.inner().asks().levels()[0].price, dec!(1001.0));
}

#[test]
fn test_mid_price_and_spread() {
    let bids = vec![Level::new(dec!(1000.0), dec!(1.5))];
    let asks = vec![Level::new(dec!(1010.0), dec!(1.0))];

    let orderbook = OrderBook::new(123, Some(Utc::now()), bids, asks);
    let canonical = CanonicalOrderBook::from(orderbook);

    assert_eq!(canonical.mid_price(), Some(1005.0));
    assert_eq!(canonical.spread(), Some(10.0));

    // Use approximate equality for floating point numbers
    let relative_spread = canonical.relative_spread().unwrap();
    assert!(
        (relative_spread - 0.995024875621891).abs() < 1e-10,
        "Expected 0.995024875621891, got {}",
        relative_spread
    );
}

#[test]
fn test_volume_at_depth() {
    let bids = vec![
        Level::new(dec!(1000.0), dec!(1.5)),
        Level::new(dec!(999.0), dec!(2.0)),
        Level::new(dec!(998.0), dec!(3.0)),
    ];
    let asks = vec![
        Level::new(dec!(1001.0), dec!(1.0)),
        Level::new(dec!(1002.0), dec!(2.0)),
        Level::new(dec!(1003.0), dec!(3.0)),
    ];

    let orderbook = OrderBook::new(123, Some(Utc::now()), bids, asks);
    let canonical = CanonicalOrderBook::from(orderbook);

    let (bid_volume, ask_volume) = canonical.volume_at_depth(2);
    assert_eq!(bid_volume, 3.5);
    assert_eq!(ask_volume, 3.0);
}
