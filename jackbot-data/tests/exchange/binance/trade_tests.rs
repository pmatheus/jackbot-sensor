use jackbot_data::exchange::binance::trade::BinanceTrade;
use jackbot_data::subscription::SubscriptionId;
use jackbot_instrument::Side;
use jackbot_integration::{de::datetime_utc_from_epoch_duration, error::SocketError};
use serde::de::Error;
use std::time::Duration;

#[test]
fn test_binance_trade() {
    struct TestCase {
        input: &'static str,
        expected: Result<BinanceTrade, SocketError>,
    }

    let tests = vec![
        TestCase {
            // TC0: Spot trade valid
            input: r#"
            {
                "e":"trade","E":1649324825173,"s":"ETHUSDT","t":1000000000,
                "p":"10000.19","q":"0.239000","b":10108767791,"a":10108764858,
                "T":1749354825200,"m":false,"M":true
            }
            "#,
            expected: Ok(BinanceTrade {
                subscription_id: SubscriptionId::from("@trade|ETHUSDT"),
                time: datetime_utc_from_epoch_duration(Duration::from_millis(
                    1749354825200,
                )),
                id: 1000000000,
                price: 10000.19,
                amount: 0.239000,
                side: Side::Buy,
            }),
        },
        TestCase {
            // TC1: Spot trade malformed w/ "yes" is_buyer_maker field
            input: r#"{"e":"trade","E":1649324825173,"s":"ETHUSDT","t":1000000000,
                "p":"10000.19000000","q":"0.239000","b":10108767791,"a":10108764858,
                "T":1649324825173,"m":"yes","M":true
            }"#,
            expected: Err(SocketError::Deserialise {
                error: serde_json::Error::custom(""),
                payload: "".to_owned(),
            }),
        },
        TestCase {
            // TC2: FuturePerpetual trade w/ type MARKET
            input: r#"
            {
                "e": "trade","E": 1649839266194,"T": 1749354825200,"s": "ETHUSDT",
                "t": 1000000000,"p":"10000.19","q":"0.239000","X": "MARKET","m": true
            }
            "#,
            expected: Ok(BinanceTrade {
                subscription_id: SubscriptionId::from("@trade|ETHUSDT"),
                time: datetime_utc_from_epoch_duration(Duration::from_millis(
                    1749354825200,
                )),
                id: 1000000000,
                price: 10000.19,
                amount: 0.239000,
                side: Side::Sell,
            }),
        },
        TestCase {
            // TC3: FuturePerpetual trade w/ type LIQUIDATION
            input: r#"
            {
                "e": "trade","E": 1649839266194,"T": 1749354825200,"s": "ETHUSDT",
                "t": 1000000000,"p":"10000.19","q":"0.239000","X": "LIQUIDATION","m": false
            }
            "#,
            expected: Ok(BinanceTrade {
                subscription_id: SubscriptionId::from("@trade|ETHUSDT"),
                time: datetime_utc_from_epoch_duration(Duration::from_millis(
                    1749354825200,
                )),
                id: 1000000000,
                price: 10000.19,
                amount: 0.239000,
                side: Side::Buy,
            }),
        },
        TestCase {
            // TC4: FuturePerpetual trade w/ type LIQUIDATION
            input: r#"{"e": "trade","E": 1649839266194,"T": 1749354825200,"s": "ETHUSDT",
                "t": 1000000000,"p":"10000.19","q":"0.239000","X": "INSURANCE_FUND","m": false
            }"#,
            expected: Ok(BinanceTrade {
                subscription_id: SubscriptionId::from("@trade|ETHUSDT"),
                time: datetime_utc_from_epoch_duration(Duration::from_millis(
                    1749354825200,
                )),
                id: 1000000000,
                price: 10000.19,
                amount: 0.239000,
                side: Side::Buy,
            }),
        },
    ];

    for (index, test) in tests.into_iter().enumerate() {
        let actual = serde_json::from_str::<BinanceTrade>(test.input);
        match (actual, test.expected) {
            (Ok(actual), Ok(expected)) => {
                assert_eq!(actual, expected, "TC{} failed", index)
            }
            (Err(_), Err(_)) => {
                // Test passed
            }
            (actual, expected) => {
                // Test failed
                panic!(
                    "TC{index} failed because actual != expected. \nActual: {actual:?}\nExpected: {expected:?}\n"
                );
            }
        }
    }
}
