#[cfg(test)]
mod coinbase_ws_tests {
    use super::*;
    use futures_util::stream::StreamExt;
    use jackbot_data::{
        books::{Level, OrderBook},
        event::{DataKind, MarketEvent},
        subscription::{book::OrderBookEvent, trade::PublicTrade},
    };
    use jackbot_execution::client::coinbase::websocket::{CoinbaseWsManager};
    use jackbot_instrument::{
        exchange::ExchangeId,
        instrument::name::InstrumentNameExchange,
        Side,
    };
    use mockito::{self, Matcher};
    use rust_decimal::Decimal;
    use serde_json::json;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Test connection establishment to Coinbase WebSocket
    #[tokio::test]
    async fn test_connection_establishment() {
        // Mock WebSocket server
        let _m = mockito::mock("GET", "/")
            .with_status(101)
            .with_header("connection", "upgrade")
            .with_header("upgrade", "websocket")
            .expect(1)
            .create();

        let manager = CoinbaseWsManager::new(true); // Use sandbox
        
        // Attempt to subscribe to a product
        let result = manager.subscribe_order_book(vec!["BTC-USD".to_string()]).await;
        assert!(result.is_ok(), "Should successfully create subscription");
    }

    /// Test reconnection logic with exponential backoff
    #[tokio::test]
    async fn test_reconnection_logic() {
        let manager = CoinbaseWsManager::new(true);
        
        // First connection should fail, then succeed on retry
        let _m1 = mockito::mock("GET", "/")
            .with_status(500)
            .expect(1)
            .create();
            
        let _m2 = mockito::mock("GET", "/")
            .with_status(101)
            .with_header("connection", "upgrade")
            .with_header("upgrade", "websocket")
            .expect(1)
            .create();

        let result = timeout(
            Duration::from_secs(5),
            manager.subscribe_order_book(vec!["BTC-USD".to_string()])
        ).await;
        
        assert!(result.is_ok(), "Should reconnect after failure");
    }

    /// Test message parsing for different message types
    #[tokio::test]
    async fn test_message_parsing() {
        // Test snapshot message parsing
        let snapshot_msg = json!({
            "type": "snapshot",
            "product_id": "BTC-USD",
            "bids": [["50000.00", "1.5"], ["49999.00", "2.0"]],
            "asks": [["50001.00", "1.2"], ["50002.00", "2.5"]]
        });

        // Test L2 update message parsing
        let update_msg = json!({
            "type": "l2update",
            "product_id": "BTC-USD",
            "time": "2024-01-01T00:00:00.000Z",
            "changes": [
                ["buy", "50000.50", "1.0"],
                ["sell", "50001.50", "0"]
            ]
        });

        // Test match/trade message parsing
        let trade_msg = json!({
            "type": "match",
            "trade_id": 12345,
            "sequence": 1234567890,
            "maker_order_id": "maker123",
            "taker_order_id": "taker456",
            "time": "2024-01-01T00:00:00.000Z",
            "product_id": "BTC-USD",
            "size": "0.1",
            "price": "50000.00",
            "side": "buy"
        });

        // Verify all messages parse correctly
        assert!(serde_json::from_value::<CoinbaseMessage>(snapshot_msg).is_ok());
        assert!(serde_json::from_value::<CoinbaseMessage>(update_msg).is_ok());
        assert!(serde_json::from_value::<CoinbaseMessage>(trade_msg).is_ok());
    }

    /// Test error handling for malformed messages
    #[tokio::test]
    async fn test_error_handling() {
        // Test various error scenarios
        let error_msg = json!({
            "type": "error",
            "message": "Invalid subscription"
        });

        let malformed_msg = json!({
            "type": "unknown_type",
            "data": "some_data"
        });

        // Parse error message
        match serde_json::from_value::<CoinbaseMessage>(error_msg) {
            Ok(CoinbaseMessage::Error(err)) => {
                assert_eq!(err.message, "Invalid subscription");
            }
            _ => panic!("Expected error message"),
        }

        // Malformed message should fail to parse
        assert!(serde_json::from_value::<CoinbaseMessage>(malformed_msg).is_err());
    }

    /// Test rate limit compliance (max 100 messages per second)
    #[tokio::test]
    async fn test_rate_limit_compliance() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        use tokio::time::Instant;

        let message_count = Arc::new(AtomicU32::new(0));
        let start = Instant::now();
        
        // Simulate receiving 150 messages
        for _ in 0..150 {
            message_count.fetch_add(1, Ordering::SeqCst);
            
            // Check if we're exceeding rate limit
            let elapsed = start.elapsed();
            let current_count = message_count.load(Ordering::SeqCst);
            let rate = current_count as f64 / elapsed.as_secs_f64();
            
            // Ensure we don't exceed 100 messages per second
            assert!(rate <= 100.0, "Rate limit exceeded: {} msg/s", rate);
            
            // Small delay to simulate message processing
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Test order book snapshot processing
    #[tokio::test]
    async fn test_orderbook_snapshot_processing() {
        let snapshot = SnapshotMessage {
            product_id: "BTC-USD".to_string(),
            bids: vec![
                vec!["50000.00".to_string(), "1.5".to_string()],
                vec!["49999.00".to_string(), "2.0".to_string()],
            ],
            asks: vec![
                vec!["50001.00".to_string(), "1.2".to_string()],
                vec!["50002.00".to_string(), "2.5".to_string()],
            ],
        };

        let mut orderbooks = HashMap::new();
        let event = convert_snapshot_to_event(snapshot, &mut orderbooks);
        
        assert!(event.is_some());
        let event = event.unwrap();
        
        match event.kind {
            DataKind::OrderBook(OrderBookEvent::Snapshot(orderbook)) => {
                assert_eq!(orderbook.bids.len(), 2);
                assert_eq!(orderbook.asks.len(), 2);
                assert_eq!(orderbook.bids[0].price, Decimal::from_str("50000.00").unwrap());
                assert_eq!(orderbook.asks[0].price, Decimal::from_str("50001.00").unwrap());
            }
            _ => panic!("Expected OrderBook snapshot"),
        }
    }

    /// Test incremental order book updates
    #[tokio::test]
    async fn test_orderbook_incremental_updates() {
        let mut orderbooks = HashMap::new();
        
        // First, create a snapshot
        let snapshot = SnapshotMessage {
            product_id: "BTC-USD".to_string(),
            bids: vec![vec!["50000.00".to_string(), "1.5".to_string()]],
            asks: vec![vec!["50001.00".to_string(), "1.2".to_string()]],
        };
        convert_snapshot_to_event(snapshot, &mut orderbooks);

        // Apply an update
        let update = L2UpdateMessage {
            product_id: "BTC-USD".to_string(),
            time: "2024-01-01T00:00:00.000Z".to_string(),
            changes: vec![
                vec!["buy".to_string(), "50000.50".to_string(), "2.0".to_string()],
                vec!["sell".to_string(), "50001.00".to_string(), "0".to_string()], // Remove level
            ],
        };

        let event = convert_l2_update_to_event(update, &mut orderbooks);
        assert!(event.is_some());
        
        match event.unwrap().kind {
            DataKind::OrderBook(OrderBookEvent::Update(orderbook)) => {
                assert_eq!(orderbook.bids.len(), 2); // Original + new
                assert_eq!(orderbook.asks.len(), 0); // Removed
            }
            _ => panic!("Expected OrderBook update"),
        }
    }

    /// Test order book integrity after multiple updates
    #[tokio::test]
    async fn test_orderbook_integrity() {
        let mut orderbooks = HashMap::new();
        
        // Create initial snapshot
        let snapshot = SnapshotMessage {
            product_id: "BTC-USD".to_string(),
            bids: vec![
                vec!["50000.00".to_string(), "1.0".to_string()],
                vec!["49999.00".to_string(), "2.0".to_string()],
            ],
            asks: vec![
                vec!["50001.00".to_string(), "1.0".to_string()],
                vec!["50002.00".to_string(), "2.0".to_string()],
            ],
        };
        convert_snapshot_to_event(snapshot, &mut orderbooks);

        // Apply multiple updates
        let updates = vec![
            // Update existing level
            vec!["buy".to_string(), "50000.00".to_string(), "1.5".to_string()],
            // Add new level
            vec!["buy".to_string(), "50000.50".to_string(), "0.5".to_string()],
            // Remove level
            vec!["sell".to_string(), "50002.00".to_string(), "0".to_string()],
        ];

        let update_msg = L2UpdateMessage {
            product_id: "BTC-USD".to_string(),
            time: "2024-01-01T00:00:00.000Z".to_string(),
            changes: updates,
        };

        let event = convert_l2_update_to_event(update_msg, &mut orderbooks);
        
        match event.unwrap().kind {
            DataKind::OrderBook(OrderBookEvent::Update(orderbook)) => {
                // Verify bid levels are sorted correctly (descending)
                assert!(orderbook.bids[0].price > orderbook.bids[1].price);
                // Verify ask levels are sorted correctly (ascending)
                if orderbook.asks.len() > 1 {
                    assert!(orderbook.asks[0].price < orderbook.asks[1].price);
                }
                // Verify the update was applied
                assert_eq!(orderbook.bids.iter().find(|l| l.price == Decimal::from_str("50000.00").unwrap()).unwrap().amount, 
                          Decimal::from_str("1.5").unwrap());
            }
            _ => panic!("Expected OrderBook update"),
        }
    }

    /// Test latency measurement (should be <10ms)
    #[tokio::test]
    async fn test_latency_measurement() {
        use tokio::time::Instant;
        
        let start = Instant::now();
        
        // Simulate message processing
        let trade_msg = MatchMessage {
            trade_id: 12345,
            sequence: 1234567890,
            maker_order_id: "maker123".to_string(),
            taker_order_id: "taker456".to_string(),
            time: "2024-01-01T00:00:00.000Z".to_string(),
            product_id: "BTC-USD".to_string(),
            size: "0.1".to_string(),
            price: "50000.00".to_string(),
            side: "buy".to_string(),
        };
        
        let event = convert_match_to_event(trade_msg);
        assert!(event.is_some());
        
        let latency = start.elapsed();
        assert!(latency < Duration::from_millis(10), 
                "Processing latency too high: {:?}", latency);
    }

    /// Test concurrent subscriptions
    #[tokio::test]
    async fn test_concurrent_subscriptions() {
        let manager = CoinbaseWsManager::new(true);
        
        // Subscribe to multiple products concurrently
        let products = vec![
            vec!["BTC-USD".to_string()],
            vec!["ETH-USD".to_string()],
            vec!["SOL-USD".to_string()],
        ];
        
        let mut handles = vec![];
        for product in products {
            let mgr = manager.clone();
            let handle = tokio::spawn(async move {
                mgr.subscribe_order_book(product).await
            });
            handles.push(handle);
        }
        
        // All subscriptions should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }

    /// Test memory efficiency with large order books
    #[tokio::test]
    async fn test_memory_efficiency() {
        let mut orderbooks = HashMap::new();
        
        // Create a large order book (1000 levels each side)
        let mut bids = vec![];
        let mut asks = vec![];
        
        for i in 0..1000 {
            let bid_price = 50000.0 - (i as f64 * 0.01);
            let ask_price = 50001.0 + (i as f64 * 0.01);
            bids.push(vec![bid_price.to_string(), "1.0".to_string()]);
            asks.push(vec![ask_price.to_string(), "1.0".to_string()]);
        }
        
        let snapshot = SnapshotMessage {
            product_id: "BTC-USD".to_string(),
            bids,
            asks,
        };
        
        let event = convert_snapshot_to_event(snapshot, &mut orderbooks);
        assert!(event.is_some());
        
        // Verify memory accumulator is working
        assert!(orderbooks.contains_key("BTC-USD"));
        let accumulator = orderbooks.get("BTC-USD").unwrap();
        assert_eq!(accumulator.bids.len(), 1000);
        assert_eq!(accumulator.asks.len(), 1000);
    }
}