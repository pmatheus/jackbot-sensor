//! KuCoin integration tests
//!
//! Comprehensive tests for KuCoin connector implementation including:
//! - REST API functionality
//! - WebSocket connections
//! - Order book management
//! - Authentication
//! - Rate limiting

use jackbot_execution::{
    client::{ExecutionClient, kucoin::{KuCoinClient, KuCoinConfig, orderbook::OrderBook}},
    error::UnindexedClientError,
};
use jackbot_instrument::{
    asset::name::AssetNameExchange,
    instrument::name::InstrumentNameExchange,
};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::time::{timeout, Duration};

/// Test KuCoin configuration creation
#[test]
fn test_kucoin_config_creation() {
    let config = KuCoinConfig::new(
        "test-api-key".to_string(),
        "test-api-secret".to_string(),
        "test-passphrase".to_string(),
    );
    
    assert_eq!(config.api_key, "test-api-key");
    assert_eq!(config.api_secret, "test-api-secret");
    assert_eq!(config.passphrase, "test-passphrase");
    assert_eq!(config.api_version, "2");
    assert!(config.rest_url.to_string().contains("api.kucoin.com"));
}

/// Test KuCoin sandbox configuration
#[test]
fn test_kucoin_sandbox_config() {
    let config = KuCoinConfig::new_sandbox(
        "test-api-key".to_string(),
        "test-api-secret".to_string(),
        "test-passphrase".to_string(),
    );
    
    assert!(config.rest_url.to_string().contains("openapi-sandbox.kucoin.com"));
}

/// Test KuCoin client creation
#[test]
fn test_kucoin_client_creation() {
    let config = KuCoinConfig::new_sandbox(
        "test-api-key".to_string(),
        "test-api-secret".to_string(),
        "test-passphrase".to_string(),
    );
    
    let client = KuCoinClient::new(config);
    assert_eq!(KuCoinClient::EXCHANGE, jackbot_instrument::exchange::ExchangeId::Kucoin);
}

/// Test order book operations
#[test]
fn test_order_book_operations() {
    let mut ob = OrderBook::new("BTC-USDT".to_string());
    
    // Test initial state
    assert_eq!(ob.symbol, "BTC-USDT");
    assert!(ob.best_bid().is_none());
    assert!(ob.best_ask().is_none());
    assert!(ob.spread().is_none());
    
    // Test levels
    let (bids, asks) = ob.levels(10);
    assert_eq!(bids.len(), 0);
    assert_eq!(asks.len(), 0);
}

/// Mock test for account snapshot (requires no network)
#[tokio::test]
async fn test_kucoin_account_snapshot_mock() {
    let config = KuCoinConfig::new_sandbox(
        "test-api-key".to_string(),
        "test-api-secret".to_string(),
        "test-passphrase".to_string(),
    );
    
    let client = KuCoinClient::new(config);
    
    // This will fail with network error since it's a mock, but we test the structure
    let assets = vec![AssetNameExchange::new("BTC"), AssetNameExchange::new("USDT")];
    let instruments = vec![InstrumentNameExchange::new("BTC-USDT")];
    
    let result = timeout(
        Duration::from_secs(1),
        client.account_snapshot(&assets, &instruments)
    ).await;
    
    // Should timeout or return connection error
    assert!(result.is_err() || result.unwrap().is_err());
}

/// Test rate limiting behavior
#[tokio::test]
async fn test_rate_limiting() {
    use jackbot_data::exchange::kucoin::rate_limit::KucoinRateLimit;
    use jackbot_integration::rate_limit::Priority;
    use tokio::time::Instant;
    
    let rate_limiter = KucoinRateLimit::with_params(
        1,
        Duration::from_millis(100),
        1,
        Duration::from_millis(100),
        Duration::from_millis(0),
    );
    
    // First request should be immediate
    let start = Instant::now();
    rate_limiter.acquire_rest(Priority::Normal).await;
    let first_duration = start.elapsed();
    assert!(first_duration < Duration::from_millis(10));
    
    // Second request should be delayed
    let start = Instant::now();
    rate_limiter.acquire_rest(Priority::Normal).await;
    let second_duration = start.elapsed();
    assert!(second_duration >= Duration::from_millis(90));
}

/// Performance test for order book updates
#[test]
fn test_order_book_performance() {
    use std::time::Instant;
    
    let mut ob = OrderBook::new("BTC-USDT".to_string());
    
    // Simulate order book update performance
    let start = Instant::now();
    
    // This would normally use real KuCoinL2Update data
    // For now, we test the structure is performant
    let update_count = 1000;
    for i in 0..update_count {
        // Simulate price level update
        let price = Decimal::from_str(&format!("50000.{:03}", i)).unwrap();
        let level = jackbot_execution::client::kucoin::orderbook::PriceLevel {
            price,
            quantity: Decimal::from_str("1.0").unwrap(),
            sequence: i as i64,
        };
        ob.bids.insert(price, level);
    }
    
    let duration = start.elapsed();
    println!("Updated {} levels in {:?}", update_count, duration);
    
    // Should be well under 10ms for 1000 updates
    assert!(duration < Duration::from_millis(10));
    
    // Test best bid/ask performance
    let start = Instant::now();
    for _ in 0..1000 {
        let _ = ob.best_bid();
        let _ = ob.best_ask();
        let _ = ob.spread();
    }
    let query_duration = start.elapsed();
    println!("1000 queries in {:?}", query_duration);
    assert!(query_duration < Duration::from_millis(1));
}

/// Test WebSocket message parsing
#[test]
fn test_websocket_message_parsing() {
    use jackbot_execution::client::kucoin::types::{KuCoinWsMessage, KuCoinL2Update};
    
    // Test parsing a typical KuCoin WebSocket message
    let json_msg = r#"{
        "type": "message",
        "topic": "/market/level2:BTC-USDT",
        "subject": "trade.l2update",
        "data": {
            "symbol": "BTC-USDT",
            "changes": {
                "asks": [["50100.0", "0.5", "123"]],
                "bids": [["50000.0", "1.0", "124"]]
            },
            "sequenceStart": 123,
            "sequenceEnd": 124
        }
    }"#;
    
    let parsed: Result<KuCoinWsMessage, _> = serde_json::from_str(json_msg);
    assert!(parsed.is_ok());
    
    let ws_msg = parsed.unwrap();
    assert_eq!(ws_msg.r#type, "message");
    assert_eq!(ws_msg.topic, Some("/market/level2:BTC-USDT".to_string()));
    
    // Test parsing the data as L2Update
    if let Some(data) = ws_msg.data {
        let l2_update: Result<KuCoinL2Update, _> = serde_json::from_value(data);
        assert!(l2_update.is_ok());
        
        let update = l2_update.unwrap();
        assert_eq!(update.symbol, "BTC-USDT");
        assert_eq!(update.sequence_start, 123);
        assert_eq!(update.sequence_end, 124);
        assert_eq!(update.changes.asks.len(), 1);
        assert_eq!(update.changes.bids.len(), 1);
    }
}

/// Test authentication signature generation
#[test]
fn test_signature_generation() {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use base64::{Engine as _, engine::general_purpose};
    
    type HmacSha256 = Hmac<Sha256>;
    
    let api_secret = "test-secret";
    let timestamp = "1640995200000";
    let method = "GET";
    let endpoint = "/api/v1/accounts";
    let body = "";
    
    let str_to_sign = format!("{}{}{}{}", timestamp, method, endpoint, body);
    
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).unwrap();
    mac.update(str_to_sign.as_bytes());
    let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    
    // Should generate a valid base64 signature
    assert!(!signature.is_empty());
    assert!(signature.len() > 20);
    
    // Should be consistent
    let mut mac2 = HmacSha256::new_from_slice(api_secret.as_bytes()).unwrap();
    mac2.update(str_to_sign.as_bytes());
    let signature2 = general_purpose::STANDARD.encode(mac2.finalize().into_bytes());
    assert_eq!(signature, signature2);
}

/// Integration test for error handling
#[test]
fn test_error_handling() {
    use jackbot_execution::error::{UnindexedClientError, ConnectivityError};
    
    // Test error type conversion
    let connectivity_error = ConnectivityError::Socket("Connection failed".to_string());
    let client_error = UnindexedClientError::Connectivity(connectivity_error);
    
    match client_error {
        UnindexedClientError::Connectivity(ConnectivityError::Socket(msg)) => {
            assert_eq!(msg, "Connection failed");
        }
        _ => panic!("Unexpected error type"),
    }
}

/// Benchmark test for market data processing
#[tokio::test]
async fn test_market_data_latency() {
    use tokio::time::Instant;
    
    // Simulate market data processing latency
    let start = Instant::now();
    
    // Simulate parsing a market data message
    let json_data = r#"{"symbol":"BTC-USDT","price":"50000.0","quantity":"1.0"}"#;
    let parsed: serde_json::Value = serde_json::from_str(json_data).unwrap();
    
    // Simulate decimal conversion (common bottleneck)
    let price = Decimal::from_str("50000.0").unwrap();
    let quantity = Decimal::from_str("1.0").unwrap();
    let value = price * quantity;
    
    let latency = start.elapsed();
    
    // Should process market data in well under 1ms
    assert!(latency < Duration::from_micros(100));
    assert_eq!(value, Decimal::from_str("50000.0").unwrap());
}