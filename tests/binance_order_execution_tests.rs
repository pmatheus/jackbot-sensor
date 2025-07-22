//! Unit tests for Binance order execution flow
//! Following TDD methodology - write tests first, then implementation

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use mockito::{mock, Mock, Server};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use jackbot_sensor::connector::{
    Balance, Exchange, MarketData, Order, OrderId, OrderResult, OrderSide, OrderStatus, OrderType,
    TimeInForce,
};
use jackbot_sensor::connectors::binance::BinanceConnector;

/// Mock server for testing Binance REST API
async fn setup_mock_server() -> (Server, Vec<Mock>) {
    let mut server = Server::new_async().await;
    let mut mocks = Vec::new();

    // Mock order placement endpoint
    let order_mock = server
        .mock("POST", "/api/v3/order")
        .match_header("X-MBX-APIKEY", mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "symbol": "BTCUSDT",
                "orderId": 123456789,
                "orderListId": -1,
                "clientOrderId": "test-order-id",
                "transactTime": 1507725176595,
                "price": "50000.00000000",
                "origQty": "1.00000000",
                "executedQty": "0.00000000",
                "cummulativeQuoteQty": "0.00000000",
                "status": "NEW",
                "timeInForce": "GTC",
                "type": "LIMIT",
                "side": "BUY",
                "fills": []
            })
            .to_string(),
        )
        .create_async()
        .await;
    mocks.push(order_mock);

    // Mock order cancellation endpoint
    let cancel_mock = server
        .mock("DELETE", "/api/v3/order")
        .match_header("X-MBX-APIKEY", mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "symbol": "BTCUSDT",
                "origClientOrderId": "test-order-id",
                "orderId": 123456789,
                "orderListId": -1,
                "clientOrderId": "cancelMyOrder1",
                "price": "50000.00000000",
                "origQty": "1.00000000",
                "executedQty": "0.00000000",
                "cummulativeQuoteQty": "0.00000000",
                "status": "CANCELED",
                "timeInForce": "GTC",
                "type": "LIMIT",
                "side": "BUY"
            })
            .to_string(),
        )
        .create_async()
        .await;
    mocks.push(cancel_mock);

    // Mock account balance endpoint
    let balance_mock = server
        .mock("GET", "/api/v3/account")
        .match_header("X-MBX-APIKEY", mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "makerCommission": 15,
                "takerCommission": 15,
                "buyerCommission": 0,
                "sellerCommission": 0,
                "canTrade": true,
                "canWithdraw": true,
                "canDeposit": true,
                "updateTime": 123456789,
                "accountType": "SPOT",
                "balances": [
                    {
                        "asset": "BTC",
                        "free": "1.00000000",
                        "locked": "0.00000000"
                    },
                    {
                        "asset": "USDT",
                        "free": "10000.00000000",
                        "locked": "500.00000000"
                    }
                ],
                "permissions": ["SPOT"]
            })
            .to_string(),
        )
        .create_async()
        .await;
    mocks.push(balance_mock);

    (server, mocks)
}

#[cfg(test)]
mod order_execution_tests {
    use super::*;

    #[tokio::test]
    async fn test_place_limit_order_success() {
        // Arrange
        let (server, _mocks) = setup_mock_server().await;
        let connector = BinanceConnector::new_with_url(
            Some("test_api_key".to_string()),
            Some("test_api_secret".to_string()),
            true,
            server.url(),
        )
        .expect("Failed to create connector");

        // Connect first
        let _connection = connector.connect().await.expect("Failed to connect");

        let order = Order {
            id: Some("test-order-id".to_string()),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(50000.0),
            quantity: 1.0,
            time_in_force: Some(TimeInForce::GTC),
            status: OrderStatus::New,
        };

        // Act
        let result = connector.place_order(order).await;

        // Assert
        assert!(result.is_ok());
        let order_result = result.unwrap();
        assert_eq!(order_result.order_id, "123456789");
        assert_eq!(order_result.status, OrderStatus::New);
        assert_eq!(order_result.filled_quantity, 0.0);
        assert_eq!(order_result.remaining_quantity, 1.0);
    }

    #[tokio::test]
    async fn test_place_market_order_success() {
        // Arrange
        let (server, _mocks) = setup_mock_server().await;
        let connector = BinanceConnector::new_with_url(
            Some("test_api_key".to_string()),
            Some("test_api_secret".to_string()),
            true,
            server.url(),
        )
        .expect("Failed to create connector");

        let _connection = connector.connect().await.expect("Failed to connect");

        let order = Order {
            id: Some("test-market-order".to_string()),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Sell,
            order_type: OrderType::Market,
            price: None,
            quantity: 0.5,
            time_in_force: None,
            status: OrderStatus::New,
        };

        // Act
        let result = connector.place_order(order).await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cancel_order_success() {
        // Arrange
        let (server, _mocks) = setup_mock_server().await;
        let connector = BinanceConnector::new_with_url(
            Some("test_api_key".to_string()),
            Some("test_api_secret".to_string()),
            true,
            server.url(),
        )
        .expect("Failed to create connector");

        let _connection = connector.connect().await.expect("Failed to connect");

        // Act
        let result = connector.cancel_order("test-order-id".to_string()).await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_balance_success() {
        // Arrange
        let (server, _mocks) = setup_mock_server().await;
        let connector = BinanceConnector::new_with_url(
            Some("test_api_key".to_string()),
            Some("test_api_secret".to_string()),
            true,
            server.url(),
        )
        .expect("Failed to create connector");

        let _connection = connector.connect().await.expect("Failed to connect");

        // Act
        let result = connector.get_balance().await;

        // Assert
        assert!(result.is_ok());
        let balances = result.unwrap();
        assert_eq!(balances.len(), 2);

        let btc_balance = balances.iter().find(|b| b.asset == "BTC").unwrap();
        assert_eq!(btc_balance.free, 1.0);
        assert_eq!(btc_balance.locked, 0.0);
        assert_eq!(btc_balance.total, 1.0);

        let usdt_balance = balances.iter().find(|b| b.asset == "USDT").unwrap();
        assert_eq!(usdt_balance.free, 10000.0);
        assert_eq!(usdt_balance.locked, 500.0);
        assert_eq!(usdt_balance.total, 10500.0);
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        // Test that rate limiting is properly enforced
        let (server, _mocks) = setup_mock_server().await;
        let connector = BinanceConnector::new_with_url(
            Some("test_api_key".to_string()),
            Some("test_api_secret".to_string()),
            true,
            server.url(),
        )
        .expect("Failed to create connector");

        let _connection = connector.connect().await.expect("Failed to connect");

        // Place multiple orders rapidly
        let mut futures = Vec::new();
        for i in 0..10 {
            let order = Order {
                id: Some(format!("test-order-{}", i)),
                symbol: "BTC/USDT".to_string(),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                price: Some(50000.0 + i as f64),
                quantity: 0.1,
                time_in_force: Some(TimeInForce::GTC),
                status: OrderStatus::New,
            };
            futures.push(connector.place_order(order));
        }

        // All should complete without rate limit errors
        let results = futures::future::join_all(futures).await;
        for result in results {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_error_handling_invalid_symbol() {
        // Test error handling for invalid trading pair
        let (server, _mocks) = setup_mock_server().await;
        let connector = BinanceConnector::new_with_url(
            Some("test_api_key".to_string()),
            Some("test_api_secret".to_string()),
            true,
            server.url(),
        )
        .expect("Failed to create connector");

        let _connection = connector.connect().await.expect("Failed to connect");

        let order = Order {
            id: Some("test-invalid-order".to_string()),
            symbol: "INVALID/PAIR".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(100.0),
            quantity: 1.0,
            time_in_force: Some(TimeInForce::GTC),
            status: OrderStatus::New,
        };

        // Should return an error
        let result = connector.place_order(order).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_order_validation() {
        // Test client-side order validation
        let (server, _mocks) = setup_mock_server().await;
        let connector = BinanceConnector::new_with_url(
            Some("test_api_key".to_string()),
            Some("test_api_secret".to_string()),
            true,
            server.url(),
        )
        .expect("Failed to create connector");

        let _connection = connector.connect().await.expect("Failed to connect");

        // Test invalid quantity
        let order = Order {
            id: Some("test-invalid-qty".to_string()),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(50000.0),
            quantity: -1.0, // Invalid negative quantity
            time_in_force: Some(TimeInForce::GTC),
            status: OrderStatus::New,
        };

        let result = connector.place_order(order).await;
        assert!(result.is_err());

        // Test limit order without price
        let order = Order {
            id: Some("test-no-price".to_string()),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: None, // Missing required price for limit order
            quantity: 1.0,
            time_in_force: Some(TimeInForce::GTC),
            status: OrderStatus::New,
        };

        let result = connector.place_order(order).await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_order_latency_under_50ms() {
        // Test that order round trip is under 50ms
        let (server, _mocks) = setup_mock_server().await;
        let connector = BinanceConnector::new_with_url(
            Some("test_api_key".to_string()),
            Some("test_api_secret".to_string()),
            true,
            server.url(),
        )
        .expect("Failed to create connector");

        let _connection = connector.connect().await.expect("Failed to connect");

        let order = Order {
            id: Some("perf-test-order".to_string()),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(50000.0),
            quantity: 1.0,
            time_in_force: Some(TimeInForce::GTC),
            status: OrderStatus::New,
        };

        // Measure latency
        let start = Instant::now();
        let _result = connector.place_order(order).await.unwrap();
        let duration = start.elapsed();

        // Assert under 50ms
        assert!(
            duration.as_millis() < 50,
            "Order latency {} ms exceeds 50ms target",
            duration.as_millis()
        );
    }

    #[tokio::test]
    async fn test_market_data_processing_under_10ms() {
        // Test that market data processing is under 10ms
        let (server, _mocks) = setup_mock_server().await;
        let connector = BinanceConnector::new_with_url(
            Some("test_api_key".to_string()),
            Some("test_api_secret".to_string()),
            true,
            server.url(),
        )
        .expect("Failed to create connector");

        let _connection = connector.connect().await.expect("Failed to connect");
        let mut stream = connector
            .subscribe_market_data(vec!["BTC/USDT".to_string()])
            .await
            .unwrap();

        // Process multiple market data updates
        let start = Instant::now();
        let mut count = 0;

        while let Some(data) = stream.next().await {
            match data {
                MarketData::Ticker(_) => {
                    count += 1;
                    if count >= 100 {
                        break;
                    }
                }
                _ => {}
            }
        }

        let duration = start.elapsed();
        let avg_processing_time = duration.as_micros() / count;

        // Assert under 10ms (10000 microseconds)
        assert!(
            avg_processing_time < 10000,
            "Average market data processing time {} μs exceeds 10ms target",
            avg_processing_time
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Run with --ignored flag to test against real testnet
    async fn test_binance_testnet_integration() {
        // This test requires valid testnet credentials
        let api_key = std::env::var("BINANCE_TESTNET_API_KEY")
            .expect("BINANCE_TESTNET_API_KEY env var required");
        let api_secret = std::env::var("BINANCE_TESTNET_API_SECRET")
            .expect("BINANCE_TESTNET_API_SECRET env var required");

        let connector = BinanceConnector::new(Some(api_key), Some(api_secret), true)
            .expect("Failed to create connector");

        let _connection = connector.connect().await.expect("Failed to connect");

        // Test real balance fetch
        let balances = connector.get_balance().await.expect("Failed to get balance");
        assert!(!balances.is_empty());

        // Test real order placement (small amount on testnet)
        let order = Order {
            id: Some(Uuid::new_v4().to_string()),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(30000.0), // Low price to avoid fill
            quantity: 0.001,
            time_in_force: Some(TimeInForce::GTC),
            status: OrderStatus::New,
        };

        let result = connector
            .place_order(order)
            .await
            .expect("Failed to place order");

        // Cancel the order
        connector
            .cancel_order(result.order_id.clone())
            .await
            .expect("Failed to cancel order");
    }
}