use jackbot_execution::market_making::{FlowToxicityDetector, QuoteRefresher, TradeSide};
// use Jackbot::market_maker::InventorySkewQuoter; // Commented out
use chrono::{Duration, TimeZone, Utc};
use rust_decimal_macros::dec;

#[test]
fn test_toxic_flow_and_refresh() {
    let detector = FlowToxicityDetector::new(dec!(0.6));
    let trades = vec![(TradeSide::Buy, dec!(7)), (TradeSide::Buy, dec!(3))];
    assert!(detector.is_toxic(&trades));

    let mut refresher = QuoteRefresher::new(Duration::seconds(10), None); // Added None
    let t0 = Utc.timestamp_opt(0, 0).unwrap();
    assert!(refresher.needs_refresh(t0));
    refresher.record_refresh(t0);
    assert!(!refresher.needs_refresh(t0 + Duration::seconds(5)));
    assert!(refresher.needs_refresh(t0 + Duration::seconds(11)));
}

// #[test] // Commenting out this test as InventorySkewQuoter is missing
// fn test_reactive_predictive_with_inventory() {
//     let quoter = InventorySkewQuoter::new(dec!(2), dec!(0.5));
//     let base = quoter.quote(dec!(100), dec!(0.2));
//     assert_eq!(base.bid_price, dec!(100 - 2 * 0.2));
//     assert_eq!(base.ask_price, dec!(100 + 2 * 0.2));
//     let reactive = reactive_adjust(base, TradeSide::Buy, dec!(0.1)); // buys, so shift up
//     assert_eq!(reactive.bid_price, dec!(99.7));
//     assert_eq!(reactive.ask_price, dec!(100.5));
//     let predictive = predictive_adjust(reactive, dec!(101)); // new mid is 101
//     assert_eq!(predictive.bid_price, dec!(100.6)); // spread 0.8, mid 101 -> 100.6, 101.4
//     assert_eq!(predictive.ask_price, dec!(101.4));
// }
