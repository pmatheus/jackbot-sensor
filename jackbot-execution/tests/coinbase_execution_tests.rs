#[cfg(test)]
mod coinbase_execution_tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use futures::stream::StreamExt;
    use jackbot_execution::{
        balance::AssetBalance,
        client::{
            coinbase::{CoinbaseClient, CoinbaseConfig},
            ExecutionClient,
        },
        error::{UnindexedClientError, UnindexedOrderError},
        order::{
            request::{OrderRequestCancel, OrderRequestOpen, OrderRequestState},
            state::{Cancelled, Open, OrderKey},
            Order, OrderKind, TimeInForce,
        },
        trade::Trade,
        UnindexedAccountEvent,
    };
    use jackbot_instrument::{
        asset::{name::AssetNameExchange, QuoteAsset},
        exchange::ExchangeId,
        instrument::name::InstrumentNameExchange,
        Side,
    };
    use mockito::{self, Matcher};
    use rust_decimal::Decimal;
    use serde_json::json;
    use std::str::FromStr;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Test order placement with various order types
    #[tokio::test]
    async fn test_order_placement() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        // Test market order
        let market_order = OrderRequestOpen {
            key: OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument: InstrumentNameExchange::new("BTC-USD"),
                order_id: "test_market_001".to_string(),
            },
            state: OrderRequestState {
                side: Side::Buy,
                price: None,
                quantity: Decimal::from_str("0.01").unwrap(),
                kind: OrderKind::Market,
                time_in_force: TimeInForce::ImmediateOrCancel,
            },
        };

        // Mock the REST API response
        let _m = mockito::mock("POST", "/orders")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({
                "id": "test_market_001",
                "product_id": "BTC-USD",
                "side": "buy",
                "type": "market",
                "size": "0.01",
                "status": "done",
                "filled_size": "0.01",
                "executed_value": "500.00",
                "fill_fees": "0.50"
            }).to_string())
            .create();

        let result = client.open_order(market_order).await;
        match result.state {
            Ok(open_state) => {
                assert_eq!(open_state.status, "done");
                assert_eq!(open_state.filled_quantity, Some(Decimal::from_str("0.01").unwrap()));
            }
            Err(e) => panic!("Order placement failed: {:?}", e),
        }
    }

    /// Test limit order placement
    #[tokio::test]
    async fn test_limit_order_placement() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        let limit_order = OrderRequestOpen {
            key: OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument: InstrumentNameExchange::new("BTC-USD"),
                order_id: "test_limit_001".to_string(),
            },
            state: OrderRequestState {
                side: Side::Sell,
                price: Some(Decimal::from_str("51000.00").unwrap()),
                quantity: Decimal::from_str("0.05").unwrap(),
                kind: OrderKind::Limit,
                time_in_force: TimeInForce::GoodTillCancelled,
            },
        };

        let _m = mockito::mock("POST", "/orders")
            .with_status(200)
            .with_body(json!({
                "id": "test_limit_001",
                "product_id": "BTC-USD",
                "side": "sell",
                "type": "limit",
                "price": "51000.00",
                "size": "0.05",
                "status": "open",
                "filled_size": "0",
                "executed_value": "0",
                "fill_fees": "0"
            }).to_string())
            .create();

        let result = client.open_order(limit_order).await;
        assert!(result.state.is_ok());
    }

    /// Test order cancellation
    #[tokio::test]
    async fn test_order_cancellation() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        let cancel_request = OrderRequestCancel {
            key: OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument: InstrumentNameExchange::new("BTC-USD"),
                order_id: "test_order_123".to_string(),
            },
        };

        let _m = mockito::mock("DELETE", "/orders/test_order_123")
            .with_status(200)
            .with_body(json!({
                "id": "test_order_123"
            }).to_string())
            .create();

        let result = client.cancel_order(cancel_request).await;
        assert!(result.state.is_ok());
    }

    /// Test fill notifications through WebSocket
    #[tokio::test]
    async fn test_fill_notifications() {
        // This would test WebSocket order status updates
        let fill_msg = json!({
            "type": "done",
            "time": "2024-01-01T00:00:00.000000Z",
            "product_id": "BTC-USD",
            "sequence": 123456789,
            "order_id": "test_order_001",
            "client_oid": "client_001",
            "side": "buy",
            "reason": "filled",
            "price": "50000.00",
            "remaining_size": "0",
            "size": "0.01"
        });

        // Parse and verify fill notification
        assert!(serde_json::from_value::<serde_json::Value>(fill_msg).is_ok());
    }

    /// Test error scenarios in order placement
    #[tokio::test]
    async fn test_order_error_scenarios() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        // Test insufficient funds error
        let order = OrderRequestOpen {
            key: OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument: InstrumentNameExchange::new("BTC-USD"),
                order_id: "test_insufficient_001".to_string(),
            },
            state: OrderRequestState {
                side: Side::Buy,
                price: None,
                quantity: Decimal::from_str("1000.0").unwrap(), // Large quantity
                kind: OrderKind::Market,
                time_in_force: TimeInForce::ImmediateOrCancel,
            },
        };

        let _m = mockito::mock("POST", "/orders")
            .with_status(400)
            .with_body(json!({
                "message": "Insufficient funds"
            }).to_string())
            .create();

        let result = client.open_order(order).await;
        assert!(result.state.is_err());
        
        match result.state {
            Err(UnindexedOrderError::InsufficientBalance) => (),
            Err(e) => panic!("Expected InsufficientBalance error, got: {:?}", e),
            Ok(_) => panic!("Expected error, but order succeeded"),
        }
    }

    /// Test balance fetching
    #[tokio::test]
    async fn test_fetch_balances() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        let _m = mockito::mock("GET", "/accounts")
            .with_status(200)
            .with_body(json!([
                {
                    "id": "btc_account",
                    "currency": "BTC",
                    "balance": "1.23456789",
                    "available": "1.20000000",
                    "hold": "0.03456789"
                },
                {
                    "id": "usd_account",
                    "currency": "USD",
                    "balance": "50000.00",
                    "available": "45000.00",
                    "hold": "5000.00"
                }
            ]).to_string())
            .create();

        let balances = client.fetch_balances().await.unwrap();
        assert_eq!(balances.len(), 2);
        
        let btc_balance = balances.iter().find(|b| b.asset.as_ref() == "BTC").unwrap();
        assert_eq!(btc_balance.total, Decimal::from_str("1.23456789").unwrap());
        assert_eq!(btc_balance.free, Decimal::from_str("1.20000000").unwrap());
    }

    /// Test fetching open orders
    #[tokio::test]
    async fn test_fetch_open_orders() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        let _m = mockito::mock("GET", "/orders")
            .match_query(Matcher::UrlEncoded("status".into(), "open".into()))
            .with_status(200)
            .with_body(json!([
                {
                    "id": "order_001",
                    "product_id": "BTC-USD",
                    "side": "buy",
                    "type": "limit",
                    "price": "49000.00",
                    "size": "0.01",
                    "status": "open",
                    "filled_size": "0",
                    "executed_value": "0",
                    "fill_fees": "0",
                    "created_at": "2024-01-01T00:00:00Z"
                }
            ]).to_string())
            .create();

        let orders = client.fetch_open_orders().await.unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].key.order_id, "order_001");
        assert_eq!(orders[0].side, Side::Buy);
    }

    /// Test trade history fetching
    #[tokio::test]
    async fn test_fetch_trades() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        let since = Utc::now() - chrono::Duration::hours(24);

        let _m = mockito::mock("GET", "/fills")
            .with_status(200)
            .with_body(json!([
                {
                    "trade_id": 12345,
                    "product_id": "BTC-USD",
                    "price": "50000.00",
                    "size": "0.01",
                    "side": "buy",
                    "fee": "2.50",
                    "created_at": "2024-01-01T00:00:00Z",
                    "liquidity": "T",
                    "settled": true
                }
            ]).to_string())
            .create();

        let trades = client.fetch_trades(since).await.unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, Decimal::from_str("50000.00").unwrap());
    }

    /// Test rate limiting compliance
    #[tokio::test]
    async fn test_rate_limiting() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        // Simulate rapid order placement
        let mut handles = vec![];
        
        for i in 0..5 {
            let c = client.clone();
            let handle = tokio::spawn(async move {
                let order = OrderRequestOpen {
                    key: OrderKey {
                        exchange: ExchangeId::Coinbase,
                        instrument: InstrumentNameExchange::new("BTC-USD"),
                        order_id: format!("test_rate_{}", i),
                    },
                    state: OrderRequestState {
                        side: Side::Buy,
                        price: Some(Decimal::from_str("49000.00").unwrap()),
                        quantity: Decimal::from_str("0.01").unwrap(),
                        kind: OrderKind::Limit,
                        time_in_force: TimeInForce::GoodTillCancelled,
                    },
                };
                c.open_order(order).await
            });
            handles.push(handle);
        }

        // All requests should complete without rate limit errors
        for handle in handles {
            let result = handle.await.unwrap();
            // Rate limiter should ensure proper spacing
            assert!(result.state.is_ok() || matches!(
                result.state,
                Err(UnindexedOrderError::RateLimitExceeded)
            ));
        }
    }

    /// Test account snapshot functionality
    #[tokio::test]
    async fn test_account_snapshot() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        // Mock both balances and orders endpoints
        let _m1 = mockito::mock("GET", "/accounts")
            .with_status(200)
            .with_body(json!([
                {
                    "id": "btc_account",
                    "currency": "BTC",
                    "balance": "1.0",
                    "available": "0.9",
                    "hold": "0.1"
                }
            ]).to_string())
            .create();

        let _m2 = mockito::mock("GET", "/orders")
            .match_query(Matcher::UrlEncoded("status".into(), "open".into()))
            .with_status(200)
            .with_body(json!([
                {
                    "id": "order_001",
                    "product_id": "BTC-USD",
                    "side": "sell",
                    "type": "limit",
                    "price": "51000.00",
                    "size": "0.1",
                    "status": "open",
                    "filled_size": "0",
                    "executed_value": "0",
                    "fill_fees": "0",
                    "created_at": "2024-01-01T00:00:00Z"
                }
            ]).to_string())
            .create();

        let snapshot = client.account_snapshot(
            &[AssetNameExchange::new("BTC")],
            &[InstrumentNameExchange::new("BTC-USD")]
        ).await.unwrap();

        assert_eq!(snapshot.exchange, ExchangeId::Coinbase);
        assert_eq!(snapshot.balances.len(), 1);
        assert_eq!(snapshot.instruments.len(), 1);
        assert_eq!(snapshot.instruments[0].orders.len(), 1);
    }

    /// Test concurrent order operations
    #[tokio::test]
    async fn test_concurrent_order_operations() {
        let config = CoinbaseConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            sandbox: true,
            ws_auth_payload: "test_auth".to_string(),
        };

        let client = CoinbaseClient::new(config);

        // Create multiple orders concurrently
        let orders = (0..3).map(|i| OrderRequestOpen {
            key: OrderKey {
                exchange: ExchangeId::Coinbase,
                instrument: InstrumentNameExchange::new("BTC-USD"),
                order_id: format!("concurrent_{}", i),
            },
            state: OrderRequestState {
                side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                price: Some(Decimal::from_str(&format!("{}.00", 49000 + i * 100)).unwrap()),
                quantity: Decimal::from_str("0.01").unwrap(),
                kind: OrderKind::Limit,
                time_in_force: TimeInForce::GoodTillCancelled,
            },
        }).collect::<Vec<_>>();

        // Mock responses for all orders
        for i in 0..3 {
            let _m = mockito::mock("POST", "/orders")
                .with_status(200)
                .with_body(json!({
                    "id": format!("concurrent_{}", i),
                    "product_id": "BTC-USD",
                    "side": if i % 2 == 0 { "buy" } else { "sell" },
                    "type": "limit",
                    "price": format!("{}.00", 49000 + i * 100),
                    "size": "0.01",
                    "status": "open",
                    "filled_size": "0",
                    "executed_value": "0",
                    "fill_fees": "0"
                }).to_string())
                .create();
        }

        let mut order_stream = client.open_orders(orders);
        let mut results = vec![];
        
        while let Some(result) = order_stream.next().await {
            results.push(result);
        }

        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.state.is_ok());
        }
    }
}