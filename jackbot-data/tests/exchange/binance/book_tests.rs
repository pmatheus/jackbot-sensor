use jackbot_data::exchange::binance::book::BinanceLevel;
use rust_decimal_macros::dec;

#[test]
fn test_binance_level() {
    let input = r#"["4.00000200", "12.00000000"]"#;
    assert_eq!(
        serde_json::from_str::<BinanceLevel>(input).unwrap(),
        BinanceLevel {
            price: dec!(4.00000200),
            amount: dec!(12.0)
        },
    )
}
