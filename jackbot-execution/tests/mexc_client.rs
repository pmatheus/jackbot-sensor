//! Tests for MEXC client implementation.

use jackbot_execution::{
    client::{
        mexc::{MexcClient, MexcConfig},
        ExecutionClient,
    },
    error::UnindexedOrderError,
};
use jackbot_instrument::{
    asset::name::AssetNameExchange, exchange::ExchangeId, instrument::name::InstrumentNameExchange,
};

#[tokio::test]
async fn test_mexc_client_creation() {
    let config = MexcConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        rest_url: "https://api.mexc.com".to_string(),
        ws_url: "wss://wbs.mexc.com".to_string(),
    };

    let client = MexcClient::new(config);
    assert_eq!(MexcClient::EXCHANGE, ExchangeId::Mexc);
}

#[tokio::test]
async fn test_mexc_account_snapshot() {
    let config = MexcConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        rest_url: "https://api.mexc.com".to_string(),
        ws_url: "wss://wbs.mexc.com".to_string(),
    };

    let client = MexcClient::new(config);
    let assets = vec![AssetNameExchange::new("BTC")];
    let instruments = vec![InstrumentNameExchange::new("BTCUSDT")];

    let snapshot = client.account_snapshot(&assets, &instruments).await;
    assert!(snapshot.is_ok());

    let snapshot = snapshot.unwrap();
    assert_eq!(snapshot.exchange, ExchangeId::Mexc);
    assert!(snapshot.balances.is_empty());
    assert!(snapshot.instruments.is_empty());
}

#[tokio::test]
async fn test_mexc_methods_return_not_implemented() {
    let config = MexcConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        rest_url: "https://api.mexc.com".to_string(),
        ws_url: "wss://wbs.mexc.com".to_string(),
    };

    let client = MexcClient::new(config);

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
