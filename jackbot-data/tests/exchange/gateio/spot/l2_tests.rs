use jackbot_data::exchange::gateio::spot::l2::GateioOrderBookL2;
use rust_decimal_macros::dec;
use serde_json;

#[test]
fn test_gateio_spot_order_book_l2() {
    let input = r#"{\"symbol\":\"BTC_USDT\",\"bids\":[[\"30000.0\",\"1.0\"]],\"asks\":[[\"30010.0\",\"2.0\"]]}"#;
    let book: GateioOrderBookL2 = serde_json::from_str(input).unwrap();
    assert_eq!(book.bids[0], (dec!(30000.0), dec!(1.0)));
    assert_eq!(book.asks[0], (dec!(30010.0), dec!(2.0)));
}
