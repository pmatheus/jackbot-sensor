//! Tests for Crypto.com client implementation.

use jackbot_execution::{
    client::{
        cryptocom::{CryptocomClient, CryptocomConfig},
        ExecutionClient,
    },
    error::UnindexedOrderError,
};
use jackbot_instrument::{
    asset::name::AssetNameExchange, exchange::ExchangeId, instrument::name::InstrumentNameExchange,
};

#[tokio::test]
async fn test_cryptocom_client_creation() {
    let config = CryptocomConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        rest_url: "https://api.crypto.com".to_string(),
        ws_url: "wss://stream.crypto.com".to_string(),
    };

    let client = CryptocomClient::new(config);
    assert_eq!(CryptocomClient::EXCHANGE, ExchangeId::Cryptocom);
}

#[tokio::test]
async fn test_cryptocom_account_snapshot() {
    let config = CryptocomConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        rest_url: "https://api.crypto.com".to_string(),
        ws_url: "wss://stream.crypto.com".to_string(),
    };

    let client = CryptocomClient::new(config);
    let assets = vec![AssetNameExchange::new("BTC")];
    let instruments = vec![InstrumentNameExchange::new("BTC_USDT")];

    let snapshot = client.account_snapshot(&assets, &instruments).await;
    assert!(snapshot.is_ok());

    let snapshot = snapshot.unwrap();
    assert_eq!(snapshot.exchange, ExchangeId::Cryptocom);
    assert!(snapshot.balances.is_empty());
    assert!(snapshot.instruments.is_empty());
}

#[tokio::test]
async fn test_cryptocom_methods_return_not_implemented() {
    let config = CryptocomConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        rest_url: "https://api.crypto.com".to_string(),
        ws_url: "wss://stream.crypto.com".to_string(),
    };

    let client = CryptocomClient::new(config);

    // Test that fetch methods return empty results
    let balances = client.fetch_balances().await;
    assert!(balances.is_ok());
    assert!(balances.unwrap().is_empty());

    let orders = client.fetch_open_orders().await;
    assert!(orders.is_ok());
    assert!(orders.unwrap().is_empty());

    let trades = client.fetch_trades(chrono::Utc::now()).await;
    assert!(trades.is_ok());
    assert!(trades.unwrap().is_empty());
}
