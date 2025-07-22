//! TDD Integration tests for OKX exchange connector
//!
//! This test suite follows TDD principles to drive the implementation
//! of the OKX exchange connector.

use anyhow::Result;
use futures::StreamExt;
use jackbot_sensor::connectors::okx::OKXConnector;
use jackbot_sensor::connector::{Exchange, Order, OrderSide, OrderType, TimeInForce, OrderStatus};
use std::time::{Duration, Instant};
use tokio;

#[tokio::test]
async fn test_okx_connector_creation() {
    // TDD: Test connector creation
    let connector = OKXConnector::new(None, None, true);
    assert!(connector.is_ok(), "OKX connector should be created successfully");
}

#[tokio::test]
async fn test_okx_connection() {
    // TDD: Test connection capability
    let connector = OKXConnector::new(None, None, true).unwrap();
    let connection_result = connector.connect().await;
    
    assert!(connection_result.is_ok(), "OKX connection should succeed");
}

#[tokio::test]
async fn test_okx_market_data_subscription() {
    // TDD: Test market data subscription
    let connector = OKXConnector::new(None, None, true).unwrap();
    
    // Connect first
    let _ = connector.connect().await.unwrap();
    
    // Test symbols for OKX
    let symbols = vec!["BTC-USDT".to_string(), "ETH-USDT".to_string()];
    let stream_result = connector.subscribe_market_data(symbols).await;
    
    assert!(stream_result.is_ok(), "OKX market data subscription should succeed");
    
    // Test that we can receive data from the stream
    let mut stream = stream_result.unwrap();
    let timeout = Duration::from_secs(3);
    let start = Instant::now();
    
    // Try to get at least one message
    let mut received_data = false;
    while start.elapsed() < timeout {
        if let Some(data) = stream.next().await {
            received_data = true;
            
            // Validate data structure
            match data {
                jackbot_sensor::connector::MarketData::Ticker(ticker) => {
                    assert_eq!(ticker.exchange, "okx");
                    assert!(!ticker.symbol.is_empty());
                    assert!(ticker.price > 0.0);
                }
                _ => {} // Other data types are also valid
            }
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    assert!(received_data, "Should receive market data from OKX stream");
}

#[tokio::test]
async fn test_okx_order_placement() {
    // TDD: Test order placement functionality
    let connector = OKXConnector::new(None, None, true).unwrap();
    
    // Connect first
    let _ = connector.connect().await.unwrap();
    
    // Create test order
    let order = Order {
        id: None,
        symbol: "BTC-USDT".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        price: Some(50000.0),
        quantity: 0.001,
        time_in_force: Some(TimeInForce::GTC),
        status: OrderStatus::New,
    };
    
    // In sandbox mode, this should work even without real credentials
    let order_result = connector.place_order(order).await;
    
    // Should either succeed or fail gracefully with proper error
    match order_result {
        Ok(result) => {
            assert!(!result.order_id.is_empty());
            assert_eq!(result.status, OrderStatus::New);
        }
        Err(e) => {
            // Should be a proper error message, not a panic
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("credentials") || error_msg.contains("API") || error_msg.contains("sandbox"),
                "Error should be informative: {}", error_msg
            );
        }
    }
}

#[tokio::test]
async fn test_okx_order_cancellation() {
    // TDD: Test order cancellation
    let connector = OKXConnector::new(None, None, true).unwrap();
    let _ = connector.connect().await.unwrap();
    
    let cancel_result = connector.cancel_order("test_order_id".to_string()).await;
    
    // Should handle cancellation gracefully
    match cancel_result {
        Ok(_) => {
            // Success is acceptable
        }
        Err(e) => {
            // Should be a proper error, not a panic
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("credentials") || error_msg.contains("order") || error_msg.contains("not found"),
                "Error should be informative: {}", error_msg
            );
        }
    }
}

#[tokio::test]
async fn test_okx_balance_retrieval() {
    // TDD: Test balance retrieval
    let connector = OKXConnector::new(None, None, true).unwrap();
    let _ = connector.connect().await.unwrap();
    
    let balance_result = connector.get_balance().await;
    
    // Should handle balance requests gracefully
    match balance_result {
        Ok(balances) => {
            // Validate balance structure if returned
            for balance in balances {
                assert!(!balance.asset.is_empty());
                assert!(balance.total >= 0.0);
                assert!(balance.free >= 0.0);
                assert!(balance.locked >= 0.0);
                assert_eq!(balance.total, balance.free + balance.locked);
            }
        }
        Err(e) => {
            // Should be a proper error message
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("credentials") || error_msg.contains("API") || error_msg.contains("balance"),
                "Error should be informative: {}", error_msg
            );
        }
    }
}

#[tokio::test]
async fn test_okx_performance_requirements() {
    // TDD: Test performance requirements
    let connector = OKXConnector::new(None, None, true).unwrap();
    
    // Test connection latency
    let start = Instant::now();
    let _ = connector.connect().await.unwrap();
    let connection_latency = start.elapsed();
    
    // Connection should be reasonably fast
    assert!(
        connection_latency < Duration::from_millis(5000),
        "Connection latency {:?} should be < 5s", connection_latency
    );
    
    // Test subscription latency
    let start = Instant::now();
    let symbols = vec!["BTC-USDT".to_string()];
    let mut stream = connector.subscribe_market_data(symbols).await.unwrap();
    let subscription_latency = start.elapsed();
    
    assert!(
        subscription_latency < Duration::from_millis(1000),
        "Subscription latency {:?} should be < 1s", subscription_latency
    );
    
    // Test message processing latency
    let mut message_latencies = Vec::new();
    let timeout = Duration::from_secs(5);
    let start_time = Instant::now();
    
    while message_latencies.len() < 10 && start_time.elapsed() < timeout {
        let msg_start = Instant::now();
        if let Some(_data) = stream.next().await {
            let latency = msg_start.elapsed();
            message_latencies.push(latency);
        }
    }
    
    if !message_latencies.is_empty() {
        message_latencies.sort();
        let median_latency = message_latencies[message_latencies.len() / 2];
        
        // For now, accept higher latency during development
        // This will be tightened as we optimize
        assert!(
            median_latency < Duration::from_millis(100),
            "Median message latency {:?} should be < 100ms", median_latency
        );
    }
}

#[tokio::test]
async fn test_okx_real_market_data_format() {
    // TDD: Test that OKX returns realistic market data
    let connector = OKXConnector::new(None, None, true).unwrap();
    let _ = connector.connect().await.unwrap();
    
    let symbols = vec!["BTC-USDT".to_string(), "ETH-USDT".to_string()];
    let mut stream = connector.subscribe_market_data(symbols).await.unwrap();
    
    // Get some market data and validate it's realistic
    let timeout = Duration::from_secs(5);
    let start_time = Instant::now();
    let mut ticker_count = 0;
    
    while ticker_count < 5 && start_time.elapsed() < timeout {
        if let Some(data) = stream.next().await {
            match data {
                jackbot_sensor::connector::MarketData::Ticker(ticker) => {
                    // Validate realistic market data
                    assert_eq!(ticker.exchange, "okx");
                    assert!(ticker.symbol.contains("BTC") || ticker.symbol.contains("ETH"));
                    
                    // Basic sanity checks for crypto prices
                    if ticker.symbol.contains("BTC") {
                        assert!(ticker.price > 1000.0 && ticker.price < 200000.0, 
                               "BTC price {} should be realistic", ticker.price);
                    }
                    if ticker.symbol.contains("ETH") {
                        assert!(ticker.price > 100.0 && ticker.price < 20000.0,
                               "ETH price {} should be realistic", ticker.price);
                    }
                    
                    assert!(ticker.bid > 0.0);
                    assert!(ticker.ask > 0.0);
                    assert!(ticker.ask >= ticker.bid, "Ask {} should be >= bid {}", ticker.ask, ticker.bid);
                    assert!(ticker.volume_24h >= 0.0);
                    assert!(ticker.high_24h >= ticker.low_24h);
                    assert!(ticker.timestamp > 0);
                    
                    ticker_count += 1;
                }
                _ => {} // Other data types are also valid
            }
        }
    }
    
    assert!(ticker_count > 0, "Should receive at least one ticker message from OKX");
}

#[tokio::test]
async fn test_okx_error_handling() {
    // TDD: Test proper error handling
    let connector = OKXConnector::new(None, None, true).unwrap();
    
    // Test with invalid symbols
    let invalid_symbols = vec!["INVALID-SYMBOL".to_string()];
    let stream_result = connector.subscribe_market_data(invalid_symbols).await;
    
    // Should handle gracefully - either filter out invalid symbols or return proper error
    match stream_result {
        Ok(_) => {
            // If it succeeds, it should filter out invalid symbols
        }
        Err(e) => {
            // Should be a proper error message
            let error_msg = e.to_string();
            assert!(!error_msg.is_empty(), "Error message should not be empty");
        }
    }
}

#[tokio::test]
async fn test_okx_concurrent_operations() {
    // TDD: Test concurrent operations capability
    let connector = std::sync::Arc::new(OKXConnector::new(None, None, true).unwrap());
    let _ = connector.connect().await.unwrap();
    
    // Test multiple concurrent subscriptions
    let mut handles = Vec::new();
    
    for i in 0..3 {
        let connector = connector.clone();
        let handle = tokio::spawn(async move {
            let symbols = vec![format!("BTC-USDT-{}", i)]; // Different symbols to avoid conflicts
            let result = connector.subscribe_market_data(symbols).await;
            result.is_ok()
        });
        handles.push(handle);
    }
    
    // Wait for all operations
    let mut success_count = 0;
    for handle in handles {
        if let Ok(success) = handle.await {
            if success {
                success_count += 1;
            }
        }
    }
    
    // At least some operations should succeed
    assert!(success_count > 0, "At least some concurrent operations should succeed");
}