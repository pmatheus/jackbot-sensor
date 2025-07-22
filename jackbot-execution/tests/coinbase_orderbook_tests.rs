#[cfg(test)]
mod coinbase_orderbook_tests {
    use super::*;
    use jackbot_data::books::{Level, OrderBook};
    use jackbot_execution::client::coinbase::orderbook::{
        CoinbaseOrderBook, OrderBookUpdate, OrderBookSnapshot,
    };
    use rust_decimal::Decimal;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::sync::RwLock;

    /// Test order book snapshot initialization
    #[tokio::test]
    async fn test_snapshot_processing() {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids: vec![
                (Decimal::from_str("50000.00").unwrap(), Decimal::from_str("1.5").unwrap()),
                (Decimal::from_str("49999.00").unwrap(), Decimal::from_str("2.0").unwrap()),
                (Decimal::from_str("49998.00").unwrap(), Decimal::from_str("1.0").unwrap()),
            ],
            asks: vec![
                (Decimal::from_str("50001.00").unwrap(), Decimal::from_str("1.2").unwrap()),
                (Decimal::from_str("50002.00").unwrap(), Decimal::from_str("2.5").unwrap()),
                (Decimal::from_str("50003.00").unwrap(), Decimal::from_str("0.5").unwrap()),
            ],
        };
        
        orderbook.apply_snapshot(snapshot).await;
        
        let book = orderbook.get_snapshot().await;
        assert_eq!(book.bids.len(), 3);
        assert_eq!(book.asks.len(), 3);
        assert_eq!(book.sequence, 1000);
        
        // Verify correct ordering
        assert!(book.bids[0].0 > book.bids[1].0); // Descending (price is first element of tuple)
        assert!(book.asks[0].0 < book.asks[1].0); // Ascending (price is first element of tuple)
    }

    /// Test incremental order book updates
    #[tokio::test]
    async fn test_incremental_updates() {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        // Initialize with snapshot
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids: vec![
                (Decimal::from_str("50000.00").unwrap(), Decimal::from_str("1.0").unwrap()),
            ],
            asks: vec![
                (Decimal::from_str("50001.00").unwrap(), Decimal::from_str("1.0").unwrap()),
            ],
        };
        orderbook.apply_snapshot(snapshot).await;
        
        // Apply updates
        let updates = vec![
            OrderBookUpdate {
                sequence: 1001,
                side: "buy",
                price: Decimal::from_str("50000.50").unwrap(),
                size: Decimal::from_str("2.0").unwrap(),
            },
            OrderBookUpdate {
                sequence: 1002,
                side: "sell",
                price: Decimal::from_str("50000.50").unwrap(),
                size: Decimal::from_str("1.5").unwrap(),
            },
            OrderBookUpdate {
                sequence: 1003,
                side: "buy",
                price: Decimal::from_str("50000.00").unwrap(),
                size: Decimal::ZERO, // Remove level
            },
        ];
        
        for update in updates {
            orderbook.apply_update(update).await.unwrap();
        }
        
        let book = orderbook.get_snapshot().await;
        assert_eq!(book.sequence, 1003);
        assert_eq!(book.bids.len(), 1); // One removed, one added
        assert_eq!(book.asks.len(), 2); // One original, one added
        
        // Verify the new bid level
        assert_eq!(book.bids[0].price, Decimal::from_str("50000.50").unwrap());
        assert_eq!(book.bids[0].amount, Decimal::from_str("2.0").unwrap());
    }

    /// Test order book integrity with out-of-order updates
    #[tokio::test]
    async fn test_out_of_order_updates() {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids: vec![],
            asks: vec![],
        };
        orderbook.apply_snapshot(snapshot).await;
        
        // Try to apply update with lower sequence (should fail)
        let old_update = OrderBookUpdate {
            sequence: 999,
            side: "buy",
            price: Decimal::from_str("50000.00").unwrap(),
            size: Decimal::from_str("1.0").unwrap(),
        };
        
        let result = orderbook.apply_update(old_update).await;
        assert!(result.is_err(), "Should reject out-of-order update");
        
        // Try to apply update with gap in sequence (should fail)
        let gap_update = OrderBookUpdate {
            sequence: 1002, // Missing 1001
            side: "buy",
            price: Decimal::from_str("50000.00").unwrap(),
            size: Decimal::from_str("1.0").unwrap(),
        };
        
        let result = orderbook.apply_update(gap_update).await;
        assert!(result.is_err(), "Should reject update with sequence gap");
    }

    /// Test atomic updates under concurrent access
    #[tokio::test]
    async fn test_atomic_updates() {
        let orderbook = Arc::new(CoinbaseOrderBook::new("BTC-USD"));
        
        // Initialize
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids: vec![(Decimal::from_str("50000.00").unwrap(), Decimal::from_str("1.0").unwrap())],
            asks: vec![(Decimal::from_str("50001.00").unwrap(), Decimal::from_str("1.0").unwrap())],
        };
        orderbook.apply_snapshot(snapshot).await;
        
        // Spawn multiple tasks that try to update concurrently
        let mut handles = vec![];
        
        for i in 0..10 {
            let ob = orderbook.clone();
            let handle = tokio::spawn(async move {
                let update = OrderBookUpdate {
                    sequence: 1001 + i as u64,
                    side: if i % 2 == 0 { "buy" } else { "sell" },
                    price: Decimal::from_str(&format!("{}.00", 50000 + i)).unwrap(),
                    size: Decimal::from_str("1.0").unwrap(),
                };
                ob.apply_update(update).await
            });
            handles.push(handle);
        }
        
        // Wait for all updates
        for handle in handles {
            let _ = handle.await.unwrap();
        }
        
        // Verify final state
        let book = orderbook.get_snapshot().await;
        assert!(book.sequence >= 1001, "Sequence should have advanced");
        assert!(book.bids.len() > 0, "Should have bid levels");
        assert!(book.asks.len() > 0, "Should have ask levels");
    }

    /// Test checksum validation
    #[tokio::test]
    async fn test_checksum_validation() {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids: vec![
                (Decimal::from_str("50000.00").unwrap(), Decimal::from_str("1.5").unwrap()),
                (Decimal::from_str("49999.00").unwrap(), Decimal::from_str("2.0").unwrap()),
            ],
            asks: vec![
                (Decimal::from_str("50001.00").unwrap(), Decimal::from_str("1.2").unwrap()),
                (Decimal::from_str("50002.00").unwrap(), Decimal::from_str("2.5").unwrap()),
            ],
        };
        
        orderbook.apply_snapshot(snapshot).await;
        
        // Calculate and verify checksum
        let checksum = orderbook.calculate_checksum().await;
        assert!(checksum != 0, "Checksum should be non-zero");
        
        // Apply update and verify checksum changes
        let update = OrderBookUpdate {
            sequence: 1001,
            side: "buy",
            price: Decimal::from_str("50000.50").unwrap(),
            size: Decimal::from_str("1.0").unwrap(),
        };
        orderbook.apply_update(update).await.unwrap();
        
        let new_checksum = orderbook.calculate_checksum().await;
        assert_ne!(checksum, new_checksum, "Checksum should change after update");
    }

    /// Test performance - updates should be processed in <10ms
    #[tokio::test]
    async fn test_update_latency() {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        // Initialize with large snapshot
        let mut bids = vec![];
        let mut asks = vec![];
        for i in 0..100 {
            bids.push((
                Decimal::from_str(&format!("{}.00", 50000 - i)).unwrap(),
                Decimal::from_str("1.0").unwrap(),
            ));
            asks.push((
                Decimal::from_str(&format!("{}.00", 50001 + i)).unwrap(),
                Decimal::from_str("1.0").unwrap(),
            ));
        }
        
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids,
            asks,
        };
        orderbook.apply_snapshot(snapshot).await;
        
        // Measure update latency
        let start = Instant::now();
        
        let update = OrderBookUpdate {
            sequence: 1001,
            side: "buy",
            price: Decimal::from_str("50000.50").unwrap(),
            size: Decimal::from_str("2.0").unwrap(),
        };
        
        orderbook.apply_update(update).await.unwrap();
        
        let latency = start.elapsed();
        assert!(
            latency < Duration::from_millis(10),
            "Update latency too high: {:?}",
            latency
        );
    }

    /// Test memory efficiency with large order books
    #[tokio::test]
    async fn test_memory_efficiency() {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        // Create very large order book
        let mut bids = vec![];
        let mut asks = vec![];
        
        for i in 0..1000 {
            bids.push((
                Decimal::from_str(&format!("{}.{:02}", 50000 - i, i % 100)).unwrap(),
                Decimal::from_str(&format!("{}.0", i % 10 + 1)).unwrap(),
            ));
            asks.push((
                Decimal::from_str(&format!("{}.{:02}", 50001 + i, i % 100)).unwrap(),
                Decimal::from_str(&format!("{}.0", i % 10 + 1)).unwrap(),
            ));
        }
        
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids,
            asks,
        };
        
        orderbook.apply_snapshot(snapshot).await;
        
        // Apply many updates
        for i in 0..100 {
            let update = OrderBookUpdate {
                sequence: 1001 + i,
                side: if i % 2 == 0 { "buy" } else { "sell" },
                price: Decimal::from_str(&format!("{}.{:02}", 50000 + (i % 20), i % 100)).unwrap(),
                size: if i % 10 == 0 { Decimal::ZERO } else { Decimal::from(i % 5 + 1) },
            };
            orderbook.apply_update(update).await.unwrap();
        }
        
        // Verify the book is still consistent
        let book = orderbook.get_snapshot().await;
        assert_eq!(book.sequence, 1100);
        
        // Verify ordering is maintained
        for i in 1..book.bids.len() {
            assert!(book.bids[i-1].price > book.bids[i].price);
        }
        for i in 1..book.asks.len() {
            assert!(book.asks[i-1].price < book.asks[i].price);
        }
    }

    /// Test best bid/ask tracking
    #[tokio::test]
    async fn test_best_bid_ask_tracking() {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids: vec![
                (Decimal::from_str("50000.00").unwrap(), Decimal::from_str("1.0").unwrap()),
                (Decimal::from_str("49999.00").unwrap(), Decimal::from_str("2.0").unwrap()),
            ],
            asks: vec![
                (Decimal::from_str("50001.00").unwrap(), Decimal::from_str("1.0").unwrap()),
                (Decimal::from_str("50002.00").unwrap(), Decimal::from_str("2.0").unwrap()),
            ],
        };
        orderbook.apply_snapshot(snapshot).await;
        
        let (best_bid, best_ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(best_bid.unwrap().0, Decimal::from_str("50000.00").unwrap());
        assert_eq!(best_ask.unwrap().0, Decimal::from_str("50001.00").unwrap());
        
        // Remove best bid
        let update = OrderBookUpdate {
            sequence: 1001,
            side: "buy",
            price: Decimal::from_str("50000.00").unwrap(),
            size: Decimal::ZERO,
        };
        orderbook.apply_update(update).await.unwrap();
        
        let (best_bid, best_ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(best_bid.unwrap().0, Decimal::from_str("49999.00").unwrap());
        assert_eq!(best_ask.unwrap().0, Decimal::from_str("50001.00").unwrap());
    }

    /// Test spread calculation
    #[tokio::test]
    async fn test_spread_calculation() {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        let snapshot = OrderBookSnapshot {
            product_id: "BTC-USD".to_string(),
            sequence: 1000,
            bids: vec![(Decimal::from_str("50000.00").unwrap(), Decimal::from_str("1.0").unwrap())],
            asks: vec![(Decimal::from_str("50001.00").unwrap(), Decimal::from_str("1.0").unwrap())],
        };
        orderbook.apply_snapshot(snapshot).await;
        
        let spread = orderbook.get_spread().await;
        assert_eq!(spread, Some(Decimal::from_str("1.00").unwrap()));
        
        let spread_bps = orderbook.get_spread_bps().await;
        assert!(spread_bps.is_some());
        assert!(spread_bps.unwrap() > Decimal::ZERO);
    }
}