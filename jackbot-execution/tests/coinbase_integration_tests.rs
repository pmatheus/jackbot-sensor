#[cfg(all(test, feature = "integration-tests"))]
mod coinbase_integration_tests {
    use super::*;
    use futures::stream::StreamExt;
    use jackbot_execution::{
        client::{
            coinbase::{CoinbaseClient, CoinbaseConfig},
            ExecutionClient,
        },
        error::UnindexedOrderError,
        order::{
            request::{OrderRequestCancel, OrderRequestOpen, OrderRequestState},
            state::OrderKey,
            OrderKind, TimeInForce,
        },
    };
    use jackbot_instrument::{
        asset::name::AssetNameExchange,
        exchange::ExchangeId,
        instrument::name::InstrumentNameExchange,
        Side,
    };
    use rust_decimal::Decimal;
    use std::env;
    use std::str::FromStr;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};
    use tracing::info;

    /// Get test configuration from environment variables
    fn get_test_config() -> Option<CoinbaseConfig> {
        let api_key = env::var("COINBASE_API_KEY").ok()?;
        let api_secret = env::var("COINBASE_API_SECRET").ok()?;
        let api_passphrase = env::var("COINBASE_API_PASSPHRASE").ok()?;
        
        Some(CoinbaseConfig {
            api_key,
            api_secret,
            api_passphrase,
            sandbox: true, // Always use sandbox for tests
            ws_auth_payload: String::new(), // Will be generated
        })
    }

    /// Test connection to Coinbase sandbox
    #[tokio::test]
    #[ignore] // Run with --ignored flag to execute
    async fn test_sandbox_connection() {
        let config = get_test_config().expect("Set COINBASE_* env vars");
        let client = CoinbaseClient::new(config);
        
        // Test fetching balances
        let result = timeout(Duration::from_secs(10), client.fetch_balances()).await;
        assert!(result.is_ok(), "Connection timeout");
        
        let balances = result.unwrap();
        assert!(balances.is_ok(), "Failed to fetch balances: {:?}", balances.err());
        
        info!("Successfully connected to Coinbase sandbox");
    }

    /// Test order placement and cancellation
    #[tokio::test]
    #[ignore]
    async fn test_order_lifecycle() {
        let config = get_test_config().expect("Set COINBASE_* env vars");
        let client = CoinbaseClient::new(config);
        
        // Place a limit order (far from market to avoid execution)
        let order_request = OrderRequestOpen {
            key: OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument: InstrumentNameExchange::new("BTC-USD"),
                order_id: format!("test_{}", chrono::Utc::now().timestamp()),
            },
            state: OrderRequestState {
                side: Side::Buy,
                price: Some(Decimal::from_str("10000.00").unwrap()), // Far below market
                quantity: Decimal::from_str("0.001").unwrap(),
                kind: OrderKind::Limit,
                time_in_force: TimeInForce::GoodTillCancelled,
            },
        };
        
        // Place order
        let order_result = client.open_order(order_request.clone()).await;
        assert!(order_result.state.is_ok(), "Failed to place order: {:?}", order_result.state);
        
        let order_id = match order_result.state {
            Ok(open_state) => open_state.id,
            Err(e) => panic!("Order placement failed: {:?}", e),
        };
        
        info!("Order placed successfully: {}", order_id);
        
        // Wait a bit for order to be fully processed
        sleep(Duration::from_secs(2)).await;
        
        // Cancel the order
        let cancel_request = OrderRequestCancel {
            key: order_request.key.clone(),
            state: crate::order::request::OrderRequestCancelState {
                id: Some(order_id.clone()),
            },
        };
        
        let cancel_result = client.cancel_order(cancel_request).await;
        assert!(cancel_result.state.is_ok(), "Failed to cancel order: {:?}", cancel_result.state);
        
        info!("Order cancelled successfully");
    }

    /// Test WebSocket market data streaming
    #[tokio::test]
    #[ignore]
    async fn test_market_data_streaming() {
        let config = get_test_config().expect("Set COINBASE_* env vars");
        let client = CoinbaseClient::new(config);
        
        // Subscribe to BTC-USD market data
        let mut stream = client.subscribe_market_data(
            vec!["BTC-USD".to_string()],
            true,  // Include trades
            true,  // Include depth
        ).await.expect("Failed to subscribe to market data");
        
        // Collect events for 10 seconds
        let start = std::time::Instant::now();
        let mut trade_count = 0;
        let mut orderbook_count = 0;
        
        while start.elapsed() < Duration::from_secs(10) {
            if let Ok(Some(event)) = timeout(Duration::from_secs(1), stream.next()).await {
                match event.kind {
                    jackbot_data::event::DataKind::Trade(_) => trade_count += 1,
                    jackbot_data::event::DataKind::OrderBook(_) => orderbook_count += 1,
                    _ => {}
                }
            }
        }
        
        info!("Received {} trades and {} orderbook updates", trade_count, orderbook_count);
        assert!(trade_count > 0 || orderbook_count > 0, "No market data received");
    }

    /// Test order book integrity
    #[tokio::test]
    #[ignore]
    async fn test_order_book_integrity() {
        use jackbot_execution::client::coinbase::orderbook::CoinbaseOrderBook;
        
        let config = get_test_config().expect("Set COINBASE_* env vars");
        let manager = jackbot_execution::client::coinbase::websocket::CoinbaseWsManager::new(true);
        
        // Subscribe to order book
        let mut stream = manager.subscribe_order_book(vec!["ETH-USD".to_string()])
            .await
            .expect("Failed to subscribe");
        
        let orderbook = CoinbaseOrderBook::new("ETH-USD");
        let mut last_checksum = 0u64;
        let mut update_count = 0;
        
        // Process updates for 20 seconds
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(20) && update_count < 50 {
            if let Ok(Some(event)) = timeout(Duration::from_secs(1), stream.next()).await {
                match event.kind {
                    jackbot_data::event::DataKind::OrderBook(book_event) => {
                        update_count += 1;
                        
                        // Verify checksum changes
                        let checksum = orderbook.calculate_checksum().await;
                        if update_count > 1 {
                            assert_ne!(checksum, last_checksum, "Checksum didn't change after update");
                        }
                        last_checksum = checksum;
                        
                        // Verify order book health
                        assert!(orderbook.is_healthy().await, "Order book unhealthy");
                        
                        // Verify spread is reasonable
                        if let Some(spread_bps) = orderbook.get_spread_bps().await {
                            assert!(spread_bps > Decimal::ZERO, "Invalid spread");
                            assert!(spread_bps < Decimal::from(1000), "Spread too wide (>10%)");
                        }
                    }
                    _ => {}
                }
            }
        }
        
        info!("Processed {} order book updates", update_count);
        assert!(update_count > 10, "Too few order book updates received");
    }

    /// Test latency requirements
    #[tokio::test]
    #[ignore]
    async fn test_latency_requirements() {
        use std::time::Instant;
        
        let config = get_test_config().expect("Set COINBASE_* env vars");
        let client = CoinbaseClient::new(config);
        
        // Measure balance fetch latency
        let mut latencies = Vec::new();
        
        for _ in 0..10 {
            let start = Instant::now();
            let _ = client.fetch_balances().await;
            let latency = start.elapsed();
            latencies.push(latency);
            
            sleep(Duration::from_millis(100)).await; // Avoid rate limits
        }
        
        // Calculate statistics
        let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        let max_latency = latencies.iter().max().unwrap();
        
        info!("Average latency: {:?}, Max latency: {:?}", avg_latency, max_latency);
        
        // Verify latency requirements (REST calls are slower, so we allow more time)
        assert!(avg_latency < Duration::from_millis(500), "Average latency too high");
        assert!(max_latency < Duration::from_secs(1), "Max latency too high");
    }

    /// Test error handling scenarios
    #[tokio::test]
    #[ignore]
    async fn test_error_handling() {
        let config = get_test_config().expect("Set COINBASE_* env vars");
        let client = CoinbaseClient::new(config);
        
        // Test invalid order (quantity too small)
        let invalid_order = OrderRequestOpen {
            key: OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument: InstrumentNameExchange::new("BTC-USD"),
                order_id: "invalid_test".to_string(),
            },
            state: OrderRequestState {
                side: Side::Buy,
                price: Some(Decimal::from_str("50000.00").unwrap()),
                quantity: Decimal::from_str("0.00001").unwrap(), // Below minimum
                kind: OrderKind::Limit,
                time_in_force: TimeInForce::GoodTillCancelled,
            },
        };
        
        let result = client.open_order(invalid_order).await;
        assert!(result.state.is_err(), "Expected error for invalid order");
        
        // Test cancelling non-existent order
        let cancel_request = OrderRequestCancel {
            key: OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument: InstrumentNameExchange::new("BTC-USD"),
                order_id: "non_existent".to_string(),
            },
            state: crate::order::request::OrderRequestCancelState {
                id: Some(crate::order::id::OrderId::new("fake_order_id")),
            },
        };
        
        let cancel_result = client.cancel_order(cancel_request).await;
        assert!(cancel_result.state.is_err(), "Expected error for non-existent order");
    }

    /// Test account snapshot functionality
    #[tokio::test]
    #[ignore]
    async fn test_account_snapshot() {
        let config = get_test_config().expect("Set COINBASE_* env vars");
        let client = CoinbaseClient::new(config);
        
        // Get full account snapshot
        let snapshot = client.account_snapshot(&[], &[]).await;
        assert!(snapshot.is_ok(), "Failed to get account snapshot");
        
        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.exchange, ExchangeId::Coinbase);
        
        // Get filtered snapshot
        let filtered_snapshot = client.account_snapshot(
            &[AssetNameExchange::new("USD")],
            &[InstrumentNameExchange::new("BTC-USD")],
        ).await;
        
        assert!(filtered_snapshot.is_ok(), "Failed to get filtered snapshot");
        
        let filtered = filtered_snapshot.unwrap();
        // Verify filtering worked
        for balance in &filtered.balances {
            assert_eq!(balance.asset.as_ref(), "USD");
        }
    }

    /// Test rate limiting compliance
    #[tokio::test]
    #[ignore]
    async fn test_rate_limiting() {
        let config = get_test_config().expect("Set COINBASE_* env vars");
        let client = CoinbaseClient::new(config);
        
        // Make rapid requests
        let mut futures = Vec::new();
        
        for i in 0..10 {
            let c = client.clone();
            let future = tokio::spawn(async move {
                let start = std::time::Instant::now();
                let result = c.fetch_balances().await;
                let elapsed = start.elapsed();
                (i, result, elapsed)
            });
            futures.push(future);
        }
        
        // Wait for all requests
        let mut success_count = 0;
        let mut total_time = Duration::ZERO;
        
        for future in futures {
            let (i, result, elapsed) = future.await.unwrap();
            if result.is_ok() {
                success_count += 1;
            }
            total_time += elapsed;
            info!("Request {} completed in {:?}", i, elapsed);
        }
        
        info!("Success rate: {}/10, Total time: {:?}", success_count, total_time);
        
        // Should handle rate limiting gracefully
        assert!(success_count >= 8, "Too many requests failed due to rate limiting");
    }

    /// Benchmark order book update performance
    #[tokio::test]
    #[ignore]
    async fn benchmark_orderbook_performance() {
        use criterion::{black_box, Criterion};
        use jackbot_execution::client::coinbase::orderbook::{CoinbaseOrderBook, OrderBookSnapshot, OrderBookUpdate};
        
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        
        // Create large snapshot
        let mut bids = Vec::new();
        let mut asks = Vec::new();
        
        for i in 0..500 {
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
        
        // Apply snapshot
        orderbook.apply_snapshot(snapshot).await;
        
        // Benchmark update application
        let start = std::time::Instant::now();
        let iterations = 1000;
        
        for i in 0..iterations {
            let update = OrderBookUpdate {
                sequence: 1001 + i,
                side: if i % 2 == 0 { "buy" } else { "sell" },
                price: Decimal::from_str(&format!("{}.50", 50000 + (i % 10))).unwrap(),
                size: Decimal::from_str("2.0").unwrap(),
            };
            
            let _ = orderbook.apply_update(update).await;
        }
        
        let elapsed = start.elapsed();
        let avg_latency = elapsed / iterations;
        
        info!("Applied {} updates in {:?}", iterations, elapsed);
        info!("Average update latency: {:?}", avg_latency);
        
        // Verify <10ms requirement
        assert!(avg_latency < Duration::from_millis(10), 
                "Update latency too high: {:?}", avg_latency);
    }
}