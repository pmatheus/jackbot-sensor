use fnv::FnvHashMap;
use jackbot_data::{
    exchange::{
        binance::{futures::BinanceFuturesUsd, spot::BinanceSpot},
        coinbase::Coinbase,
        okx::Okx,
    },
    subscription::{Map, Subscription},
    subscription::{book::OrderBooksL2, trade::PublicTrades},
};
use jackbot_instrument::instrument::market_data::{
    MarketDataInstrument, kind::MarketDataInstrumentKind,
};
use jackbot_integration::{Validator, error::SocketError, subscription::SubscriptionId};

mod subscription {
    use super::*;

    mod de {
        use super::*;

        #[test]
        fn test_subscription_okx_spot_public_trades() {
            let input = r#"
            {
                "exchange": "okx",
                "base": "btc",
                "quote": "usdt",
                "instrument_kind": "spot",
                "kind": "public_trades"
            }
            "#;

            serde_json::from_str::<Subscription<Okx, MarketDataInstrument, PublicTrades>>(input)
                .unwrap();
        }

        #[test]
        fn test_subscription_binance_spot_public_trades() {
            let input = r#"
            {
                "exchange": "binance_spot",
                "base": "btc",
                "quote": "usdt",
                "instrument_kind": "spot",
                "kind": "public_trades"
            }
            "#;

            serde_json::from_str::<Subscription<BinanceSpot, MarketDataInstrument, PublicTrades>>(
                input,
            )
            .unwrap();
        }

        #[test]
        fn test_subscription_binance_futures_usd_order_books_l2() {
            let input = r#"
            {
                "exchange": "binance_futures_usd",
                "base": "btc",
                "quote": "usdt",
                "instrument_kind": "perpetual",
                "kind": "order_books_l2"
            }
            "#;

            serde_json::from_str::<
                Subscription<BinanceFuturesUsd, MarketDataInstrument, OrderBooksL2>,
            >(input)
            .unwrap();
        }
    }

    #[test]
    fn test_validate_bitfinex_public_trades() {
        struct TestCase {
            input: Subscription<Coinbase, MarketDataInstrument, PublicTrades>,
            expected:
                Result<Subscription<Coinbase, MarketDataInstrument, PublicTrades>, SocketError>,
        }

        let tests = vec![
            TestCase {
                // TC0: Valid Coinbase Spot PublicTrades subscription
                input: Subscription::from((
                    Coinbase,
                    "base",
                    "quote",
                    MarketDataInstrumentKind::Spot,
                    PublicTrades,
                )),
                expected: Ok(Subscription::from((
                    Coinbase,
                    "base",
                    "quote",
                    MarketDataInstrumentKind::Spot,
                    PublicTrades,
                ))),
            },
            TestCase {
                // TC1: Invalid Coinbase FuturePerpetual PublicTrades subscription
                input: Subscription::from((
                    Coinbase,
                    "base",
                    "quote",
                    MarketDataInstrumentKind::Perpetual,
                    PublicTrades,
                )),
                expected: Err(SocketError::Unsupported {
                    entity: "".to_string(),
                    item: "".to_string(),
                }),
            },
        ];

        for (index, test) in tests.into_iter().enumerate() {
            let actual = test.input.validate();
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

    #[test]
    fn test_validate_okx_public_trades() {
        struct TestCase {
            input: Subscription<Okx, MarketDataInstrument, PublicTrades>,
            expected: Result<Subscription<Okx, MarketDataInstrument, PublicTrades>, SocketError>,
        }

        let tests = vec![
            TestCase {
                // TC0: Valid Okx Spot PublicTrades subscription
                input: Subscription::from((
                    Okx,
                    "base",
                    "quote",
                    MarketDataInstrumentKind::Spot,
                    PublicTrades,
                )),
                expected: Ok(Subscription::from((
                    Okx,
                    "base",
                    "quote",
                    MarketDataInstrumentKind::Spot,
                    PublicTrades,
                ))),
            },
            TestCase {
                // TC1: Valid Okx FuturePerpetual PublicTrades subscription
                input: Subscription::from((
                    Okx,
                    "base",
                    "quote",
                    MarketDataInstrumentKind::Perpetual,
                    PublicTrades,
                )),
                expected: Ok(Subscription::from((
                    Okx,
                    "base",
                    "quote",
                    MarketDataInstrumentKind::Perpetual,
                    PublicTrades,
                ))),
            },
        ];

        for (index, test) in tests.into_iter().enumerate() {
            let actual = test.input.validate();
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
}

mod instrument_map {
    use super::*;

    #[test]
    fn test_find_instrument() {
        // Initialise SubscriptionId-InstrumentKey HashMap
        let ids = Map(FnvHashMap::from_iter([(
            SubscriptionId::from("present"),
            MarketDataInstrument::from(("base", "quote", MarketDataInstrumentKind::Spot)),
        )]));

        struct TestCase {
            input: SubscriptionId,
            expected: Result<MarketDataInstrument, SocketError>,
        }

        let cases = vec![
            TestCase {
                // TC0: SubscriptionId (channel) is present in the HashMap
                input: SubscriptionId::from("present"),
                expected: Ok(MarketDataInstrument::from((
                    "base",
                    "quote",
                    MarketDataInstrumentKind::Spot,
                ))),
            },
            TestCase {
                // TC1: SubscriptionId (channel) is not present in the HashMap
                input: SubscriptionId::from("not present"),
                expected: Err(SocketError::Unidentifiable(SubscriptionId::from(
                    "not present",
                ))),
            },
        ];

        for (index, test) in cases.into_iter().enumerate() {
            let actual = ids.find(&test.input);
            match (actual, test.expected) {
                (Ok(actual), Ok(expected)) => {
                    assert_eq!(*actual, expected, "TC{} failed", index)
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
}
