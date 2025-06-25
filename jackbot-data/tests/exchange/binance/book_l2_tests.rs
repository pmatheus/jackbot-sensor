// filepath: /Users/user/jackbot/jackbot-sensor/jackbot-data/tests/exchange/binance/book_l2_tests.rs
use jackbot_data::exchange::binance::book::{l2::BinanceOrderBookL2Snapshot, BinanceLevel};
use chrono::{DateTime, Utc};
use rust_decimal_macros::dec;
use serde_json;

#[test]
fn test_binance_order_book_l2_snapshot() {
    struct TestCase {
        input: &'static str,
        expected: BinanceOrderBookL2Snapshot,
    }

    let tests = vec![
        TestCase {
            // TC0: valid Spot BinanceOrderBookL2Snapshot
            input: r#"
            {
                "lastUpdateId": 1027024,
                "bids": [
                    [
                        "4.00000000",
                        "431.00000000"
                    ]
                ],
                "asks": [
                    [
                        "4.00000200",
                        "12.00000000"
                    ]
                ]
            }
            "#,
            expected: BinanceOrderBookL2Snapshot {
                last_update_id: 1027024,
                time_exchange: Default::default(),
                time_engine: Default::default(),
                bids: vec![BinanceLevel {
                    price: dec!(4.00000000),
                    amount: dec!(431.00000000),
                }],
                asks: vec![BinanceLevel {
                    price: dec!(4.00000200),
                    amount: dec!(12.00000000),
                }],
            },
        },
        TestCase {
            // TC1: valid FuturePerpetual BinanceOrderBookL2Snapshot
            input: r#"
            {
                "lastUpdateId": 1027024,
                "E": 1589436922972,
                "T": 1589436922959,
                "bids": [
                    [
                        "4.00000000",
                        "431.00000000"
                    ]
                ],
                "asks": [
                    [
                        "4.00000200",
                        "12.00000000"
                    ]
                ]
            }
            "#,
            expected: BinanceOrderBookL2Snapshot {
                last_update_id: 1027024,
                time_exchange: Some(
                    DateTime::from_timestamp_millis(1589436922972).unwrap(),
                ),
                time_engine: Some(DateTime::from_timestamp_millis(1589436922959).unwrap()),
                bids: vec![BinanceLevel {
                    price: dec!(4.0),
                    amount: dec!(431.0),
                }],
                asks: vec![BinanceLevel {
                    price: dec!(4.00000200),
                    amount: dec!(12.0),
                }],
            },
        },
    ];

    for (index, test) in tests.into_iter().enumerate() {
        assert_eq!(
            serde_json::from_str::<BinanceOrderBookL2Snapshot>(test.input).unwrap(),
            test.expected,
            "TC{} failed",
            index
        );
    }
}
