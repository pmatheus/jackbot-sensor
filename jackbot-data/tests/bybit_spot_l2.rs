mod exchange {
    mod bybit {
        mod spot {
            mod l2 {
                use fnv::FnvHashMap;
                use jackbot_data::{
                    exchange::bybit::{
                        book::l2::BybitOrderBookL2Data, // Updated import path
                        message::BybitPayload,
                        spot::l2::BybitSpotOrderBooksL2Transformer,
                    },
                    subscription::{Map, book::OrderBookEvent}, // Added Map import
                    transformer::ExchangeTransformer,
                };
                use jackbot_integration::{Transformer, subscription::SubscriptionId};
                use rust_decimal_macros::dec;

                /// Helper to create a transformer for a single BTCUSDT subscription
                async fn init_transformer() -> BybitSpotOrderBooksL2Transformer<String> {
                    let sub_id = SubscriptionId::from("orderbook|BTCUSDT");
                    let map = Map(FnvHashMap::from_iter([(sub_id, "BTCUSDT".to_string())]));
                    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
                    BybitSpotOrderBooksL2Transformer::init(map, &[], tx)
                        .await
                        .unwrap()
                }

                #[tokio::test]
                async fn test_snapshot_and_update() {
                    let mut transformer = init_transformer().await;

                    let snapshot_json = r#"{
                        "topic": "orderbook.BTCUSDT",
                        "type": "snapshot",
                        "ts": 1672304486868,
                        "data": {
                            "s": "BTCUSDT",
                            "bids": [{"price":"16578.50","size":"0.5"}],
                            "asks": [{"price":"16579.00","size":"0.4"}]
                        }
                    }"#;

                    let snapshot: BybitPayload<BybitOrderBookL2Data> =
                        serde_json::from_str(snapshot_json).unwrap();
                    let events = transformer.transform(snapshot);
                    assert_eq!(events.len(), 1);
                    let event = events.into_iter().next().unwrap().unwrap();
                    match event.kind {
                        OrderBookEvent::Snapshot(book) => {
                            assert_eq!(book.sequence, 0);
                            assert_eq!(book.bids().levels()[0].price, dec!(16578.50));
                            assert_eq!(book.asks().levels()[0].price, dec!(16579.00));
                        }
                        _ => panic!("expected snapshot"),
                    }

                    let update_json = r#"{
                        "topic": "orderbook.BTCUSDT",
                        "type": "delta",
                        "ts": 1672304486869,
                        "data": {
                            "s": "BTCUSDT",
                            "bids": [{"price":"16578.50","size":"0.1"}],
                            "asks": [{"price":"16579.50","size":"0.6"}]
                        }
                    }"#;

                    let update: BybitPayload<BybitOrderBookL2Data> =
                        serde_json::from_str(update_json).unwrap();
                    let events = transformer.transform(update);
                    assert_eq!(events.len(), 1);
                    let event = events.into_iter().next().unwrap().unwrap();
                    match event.kind {
                        OrderBookEvent::Update(book) => {
                            assert_eq!(book.sequence, 0);
                            assert_eq!(book.bids().levels()[0].price, dec!(16578.50));
                            assert_eq!(book.asks().levels()[0].price, dec!(16579.50));
                        }
                        _ => panic!("expected update"),
                    }
                }

                #[tokio::test]
                async fn test_out_of_order_and_reconnect() {
                    let mut transformer = init_transformer().await;

                    let update1_json = r#"{
                        "topic": "orderbook.BTCUSDT",
                        "type": "delta",
                        "ts": 2,
                        "data": {"s":"BTCUSDT","bids":[],"asks":[]}
                    }"#;

                    let update2_json = r#"{
                        "topic": "orderbook.BTCUSDT",
                        "type": "delta",
                        "ts": 1,
                        "data": {"s":"BTCUSDT","bids":[],"asks":[]}
                    }"#;

                    let u1: BybitPayload<BybitOrderBookL2Data> =
                        serde_json::from_str(update1_json).unwrap();
                    let u2: BybitPayload<BybitOrderBookL2Data> =
                        serde_json::from_str(update2_json).unwrap();

                    let e1 = transformer.transform(u1)[0].as_ref().unwrap().time_exchange;
                    let e2 = transformer.transform(u2.clone())[0]
                        .as_ref()
                        .unwrap()
                        .time_exchange;

                    assert!(e1 > e2, "events should keep original timestamps");

                    // Simulate reconnect by reinitialising transformer
                    let mut transformer = init_transformer().await;
                    let events = transformer.transform(u2);
                    assert!(matches!(
                        events[0].as_ref().unwrap().kind,
                        OrderBookEvent::Update(_)
                    ));
                }
            }
        }
    }
}
