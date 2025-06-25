//! Tests for Gate.io client implementation.

use jackbot_execution::{
    client::{
        gateio::{GateioClient, GateioConfig},
        ExecutionClient,
    },
    error::UnindexedOrderError,
};
use jackbot_instrument::{
    asset::name::AssetNameExchange, exchange::ExchangeId, instrument::name::InstrumentNameExchange,
};

#[tokio::test]
async fn test_gateio_client_creation() {
    let config = GateioConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        rest_url: "https://api.gateio.ws".to_string(),
        ws_url: "wss://api.gateio.ws".to_string(),
    };

    let client = GateioClient::new(config);
    assert_eq!(GateioClient::EXCHANGE, ExchangeId::Gateio);
}

#[tokio::test]
async fn test_gateio_account_snapshot() {
    let config = GateioConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        rest_url: "https://api.gateio.ws".to_string(),
        ws_url: "wss://api.gateio.ws".to_string(),
    };

    let client = GateioClient::new(config);
    let assets = vec![AssetNameExchange::new("BTC")];
    let instruments = vec![InstrumentNameExchange::new("BTC_USDT")];

    let snapshot = client.account_snapshot(&assets, &instruments).await;
    assert!(snapshot.is_ok());

    let snapshot = snapshot.unwrap();
    assert_eq!(snapshot.exchange, ExchangeId::Gateio);
    assert!(snapshot.balances.is_empty());
    assert!(snapshot.instruments.is_empty());
}

#[tokio::test]
async fn test_gateio_methods_return_not_implemented() {
    let config = GateioConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        rest_url: "https://api.gateio.ws".to_string(),
        ws_url: "wss://api.gateio.ws".to_string(),
    };

    let client = GateioClient::new(config);

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
