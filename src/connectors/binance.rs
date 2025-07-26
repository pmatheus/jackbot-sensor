//! Binance exchange connector implementation
//!
//! This module provides the Binance implementation of the Exchange trait,
//! wrapping the jackbot-execution Binance client to provide a unified interface.

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::api::{BalanceData, KlineData, OrderBookData, TickerData, TradeData};
use crate::binance_websocket::BinanceWebSocketClient;
use crate::connector::{
    Balance, Connection, Exchange, MarketData, MarketDataStream, Order, OrderId, OrderResult,
    OrderSide, OrderStatus, OrderType, TimeInForce,
};
use crate::streaming::StreamingManager;

use jackbot_execution::client::{
    binance::futures::{BinanceFuturesUsd, BinanceFuturesUsdConfig},
    ExecutionClient,
};
use jackbot_instrument::{
    asset::name::AssetNameExchange,
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};

/// Binance connector implementing the Exchange trait
pub struct BinanceConnector {
    client: Arc<Mutex<Option<BinanceFuturesUsd>>>,
    api_key: Option<String>,
    api_secret: Option<String>,
    sandbox: bool,
    streaming_manager: Arc<StreamingManager>,
    market_data_subscriptions: Arc<Mutex<Vec<String>>>,
    base_url: Option<String>,
    rest_client: Arc<Mutex<Option<BinanceRestClient>>>,
    websocket_client: Arc<Mutex<Option<BinanceWebSocketClient>>>,
}

// Import REST client
use jackbot_execution::client::binance::rest::{BinanceRestClient, BinanceRestConfig};

impl BinanceConnector {
    /// Create a new Binance connector
    pub fn new(
        api_key: Option<String>,
        api_secret: Option<String>,
        sandbox: bool,
    ) -> Result<Self> {
        let streaming_manager = Arc::new(StreamingManager::new());
        
        Ok(Self {
            client: Arc::new(Mutex::new(None)),
            api_key,
            api_secret,
            sandbox,
            streaming_manager,
            market_data_subscriptions: Arc::new(Mutex::new(Vec::new())),
            base_url: None,
            rest_client: Arc::new(Mutex::new(None)),
            websocket_client: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a new Binance connector with custom URL (for testing)
    pub fn new_with_url(
        api_key: Option<String>,
        api_secret: Option<String>,
        sandbox: bool,
        base_url: String,
    ) -> Result<Self> {
        let streaming_manager = Arc::new(StreamingManager::new());
        
        Ok(Self {
            client: Arc::new(Mutex::new(None)),
            api_key,
            api_secret,
            sandbox,
            streaming_manager,
            market_data_subscriptions: Arc::new(Mutex::new(Vec::new())),
            base_url: Some(base_url),
            rest_client: Arc::new(Mutex::new(None)),
            websocket_client: Arc::new(Mutex::new(None)),
        })
    }

    /// Convert internal Order to jackbot OrderRequestOpen
    fn order_to_request(
        &self,
        order: Order,
    ) -> jackbot_execution::order::request::OrderRequestOpen<ExchangeId, InstrumentNameExchange> {
        use jackbot_execution::order::{
            id::{ClientOrderId, StrategyId},
            request::{OrderRequestOpen, RequestOpen},
            OrderKey, OrderKind,
        };

        let side = match order.side {
            OrderSide::Buy => Side::Buy,
            OrderSide::Sell => Side::Sell,
        };

        let kind = match order.order_type {
            OrderType::Market => OrderKind::Market,
            OrderType::Limit => OrderKind::Limit,
            _ => OrderKind::Limit, // Default to limit for unsupported types
        };

        let time_in_force = match order.time_in_force {
            Some(TimeInForce::GTC) => jackbot_execution::order::TimeInForce::GoodUntilCancelled { post_only: false },
            Some(TimeInForce::IOC) => jackbot_execution::order::TimeInForce::ImmediateOrCancel,
            Some(TimeInForce::FOK) => jackbot_execution::order::TimeInForce::FillOrKill,
            _ => jackbot_execution::order::TimeInForce::GoodUntilCancelled { post_only: false },
        };

        let instrument = InstrumentNameExchange::new(order.symbol.replace("/", ""));
        
        OrderRequestOpen {
            key: OrderKey {
                exchange: ExchangeId::BinanceFuturesUsd,
                instrument: instrument.clone(),
                strategy: StrategyId::new("sensor".to_string()),
                cid: ClientOrderId::new(
                    order.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
                ),
            },
            state: RequestOpen {
                side,
                price: rust_decimal::Decimal::try_from(order.price.unwrap_or(0.0))
                    .unwrap_or_default(),
                quantity: rust_decimal::Decimal::try_from(order.quantity).unwrap_or_default(),
                kind,
                time_in_force,
            },
        }
    }

    /// Convert jackbot Order to internal OrderResult
    fn convert_order_result(
        &self,
        order: jackbot_execution::order::Order<
            ExchangeId,
            InstrumentNameExchange,
            Result<jackbot_execution::order::state::Open, jackbot_execution::error::UnindexedOrderError>,
        >,
    ) -> Result<OrderResult> {
        match order.state {
            Ok(open_state) => Ok(OrderResult {
                order_id: order.key.cid.to_string(),
                status: OrderStatus::New,
                filled_quantity: 0.0,
                remaining_quantity: order.quantity.try_into().unwrap_or(0.0),
                average_price: 0.0,
                commission: 0.0,
                commission_asset: "USDT".to_string(),
                timestamp: chrono::Utc::now().timestamp_millis(),
            }),
            Err(e) => Err(anyhow::anyhow!("Order failed: {:?}", e)),
        }
    }

    /// Process market data from Binance WebSocket
    async fn process_market_data(&self) -> Result<()> {
        // Use the real WebSocket client if available
        let ws_client_guard = self.websocket_client.lock().await;
        if let Some(ws_client) = ws_client_guard.as_ref() {
            // Real WebSocket data is handled by BinanceWebSocketClient
            // which publishes directly to the streaming manager
            info!("Using real Binance WebSocket connection for market data");
            drop(ws_client_guard);
            
            // Just wait, as the WebSocket client handles data publishing
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        } else {
            // Fallback to simulation if WebSocket client not initialized
            warn!("WebSocket client not initialized, falling back to simulation");
            let mut interval = interval(Duration::from_millis(100));
            
            loop {
                interval.tick().await;
                
                let subscriptions = self.market_data_subscriptions.lock().await;
                for symbol in subscriptions.iter() {
                    // Simulate ticker data
                    let ticker = TickerData {
                        symbol: symbol.clone(),
                        exchange: "binance".to_string(),
                        price: 50000.0 + (rand::random::<f64>() * 1000.0),
                        bid: 49900.0,
                        ask: 50100.0,
                        volume_24h: 10000.0,
                        change_24h: 2.5,
                        high_24h: 51000.0,
                        low_24h: 49000.0,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    };
                    
                    if let Err(e) = self.streaming_manager.publish_ticker(ticker).await {
                        warn!("Failed to publish ticker data: {}", e);
                    }
                }
            }
        }
    }
    
    /// Validate order parameters before submission
    fn validate_order(&self, order: &Order) -> Result<()> {
        // Validate quantity
        if order.quantity <= 0.0 {
            return Err(anyhow::anyhow!("Order quantity must be positive"));
        }
        
        // Validate price for limit orders
        if order.order_type == OrderType::Limit && order.price.is_none() {
            return Err(anyhow::anyhow!("Limit orders require a price"));
        }
        
        if let Some(price) = order.price {
            if price <= 0.0 {
                return Err(anyhow::anyhow!("Order price must be positive"));
            }
        }
        
        // Validate symbol format
        if !order.symbol.contains('/') {
            return Err(anyhow::anyhow!("Invalid symbol format. Expected format: BTC/USDT"));
        }
        
        Ok(())
    }
    
    /// Place order using REST API
    async fn place_order_rest(&self, client: &BinanceRestClient, order: Order) -> Result<OrderResult> {
        use jackbot_execution::order::{
            id::{ClientOrderId, StrategyId},
            request::{OrderRequestOpen, RequestOpen},
            OrderKey, OrderKind,
        };
        
        let request = self.order_to_request(order.clone());
        
        // Convert to REST API format and send
        // Fix REST client integration - open_order method not found, see EXCHANGE_CLIENT_SPEC.md#binance-rest-api
        let _ = (client, request); // Suppress unused warnings
        return Err(anyhow::anyhow!("REST API integration not yet implemented"));
        
        // Remove unreachable code - commented out for compilation, see EXCHANGE_CLIENT_SPEC.md#binance-cleanup
        /*
        match response.state {
            Ok(open_state) => {
                // Extract order ID from response
                let order_id = match &open_state.exchange_order_id {
                    Some(id) => id.to_string(),
                    None => response.key.order_id.to_string(),
                };
                
                Ok(OrderResult {
                    order_id,
                    status: OrderStatus::New,
                    filled_quantity: 0.0,
                    remaining_quantity: order.quantity,
                    average_price: 0.0,
                    commission: 0.0,
                    commission_asset: "USDT".to_string(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
            Err(e) => Err(anyhow::anyhow!("Order failed: {:?}", e)),
        }
        */
    }
}

#[async_trait]
impl Exchange for BinanceConnector {
    async fn connect(&self) -> Result<Connection> {
        info!("Connecting to Binance exchange");
        
        // Initialize REST client if we have credentials
        if let (Some(api_key), Some(api_secret)) = (&self.api_key, &self.api_secret) {
            let rest_config = BinanceRestConfig {
                api_key: api_key.clone(),
                api_secret: api_secret.clone(),
                testnet: self.sandbox,
            };
            
            let rest_client = if let Some(base_url) = &self.base_url {
                // Use custom URL for testing
                let mut client = BinanceRestClient::new(rest_config);
                // Would need to modify REST client to support custom URLs
                client
            } else {
                BinanceRestClient::new(rest_config)
            };
            
            let mut rest_guard = self.rest_client.lock().await;
            *rest_guard = Some(rest_client);
        }
        
        // Create WebSocket configuration
        let config = BinanceFuturesUsdConfig::default();
        
        // Create WebSocket client
        let client = BinanceFuturesUsd::new(config);
        
        // Store WebSocket client
        let mut client_guard = self.client.lock().await;
        *client_guard = Some(client);
        
        // Initialize real WebSocket client for market data
        let ws_client = BinanceWebSocketClient::new(
            self.streaming_manager.clone(),
            None, // No direct Kafka producer
            self.sandbox,
        )?;
        
        let mut ws_guard = self.websocket_client.lock().await;
        *ws_guard = Some(ws_client);
        drop(ws_guard);
        
        // Start market data processing
        let self_clone = Arc::new(self.clone());
        tokio::spawn(async move {
            if let Err(e) = self_clone.process_market_data().await {
                error!("Market data processing error: {}", e);
            }
        });
        
        info!("Successfully connected to Binance with real WebSocket support");
        Ok(Arc::new(()) as Connection)
    }
    
    async fn subscribe_market_data(&self, symbols: Vec<String>) -> Result<MarketDataStream> {
        info!("Subscribing to market data for {} symbols", symbols.len());
        
        // Store subscriptions
        let mut subs = self.market_data_subscriptions.lock().await;
        subs.extend(symbols.clone());
        drop(subs);
        
        // Subscribe via real WebSocket client if available
        let ws_client_guard = self.websocket_client.lock().await;
        if let Some(ws_client) = ws_client_guard.as_ref() {
            info!("Subscribing to real Binance WebSocket streams");
            for symbol in &symbols {
                // Subscribe to multiple data streams for each symbol
                ws_client.subscribe_ticker(symbol).await?;
                ws_client.subscribe_orderbook(symbol).await?;
                ws_client.subscribe_trades(symbol).await?;
            }
        }
        drop(ws_client_guard);
        
        // Create stream from streaming manager
        let mut receiver = self.streaming_manager.subscribe_all().await?;
        
        // Filter for subscribed symbols
        let stream = async_stream::stream! {
            while let Ok(event) = receiver.recv().await {
                match event {
                    crate::streaming::StreamEvent::Ticker(ticker) => {
                        if symbols.contains(&ticker.symbol) {
                            yield MarketData::Ticker(ticker);
                        }
                    }
                    crate::streaming::StreamEvent::OrderBook(book) => {
                        if symbols.contains(&book.symbol) {
                            yield MarketData::OrderBook(book);
                        }
                    }
                    crate::streaming::StreamEvent::Trade(trade) => {
                        if symbols.contains(&trade.symbol) {
                            yield MarketData::Trade(trade);
                        }
                    }
                    crate::streaming::StreamEvent::Kline(kline) => {
                        if symbols.contains(&kline.symbol) {
                            yield MarketData::Kline(kline);
                        }
                    }
                    _ => {}
                }
            }
        };
        
        Ok(Box::pin(stream) as MarketDataStream)
    }
    
    async fn place_order(&self, order: Order) -> Result<OrderResult> {
        debug!("Placing order: {:?}", order);
        
        // Validate order parameters
        self.validate_order(&order)?;
        
        // Note: REST client functionality temporarily disabled due to API changes
        // Implement proper REST client integration - see EXCHANGE_CLIENT_SPEC.md#binance-rest-integration
        
        // Otherwise use WebSocket client
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .context("Client not connected")?;
        
        let request = self.order_to_request(order);
        let result = client.open_order(request).await;
        
        self.convert_order_result(result)
    }
    
    async fn cancel_order(&self, id: OrderId) -> Result<()> {
        debug!("Cancelling order: {}", id);
        
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .context("Client not connected")?;
        
        use jackbot_execution::order::{
            id::{ClientOrderId, OrderId as JackbotOrderId, StrategyId},
            request::{OrderRequestCancel, RequestCancel},
            OrderKey,
        };
        
        let request = OrderRequestCancel {
            key: OrderKey {
                exchange: ExchangeId::BinanceFuturesUsd,
                instrument: InstrumentNameExchange::new("BTCUSDT".to_string()), // Track instrument - see EXCHANGE_CLIENT_SPEC.md#instrument-tracking
                strategy: StrategyId::new("sensor".to_string()),
                cid: ClientOrderId::new(id.clone()),
            },
            state: RequestCancel {
                id: Some(JackbotOrderId::new(id)),
            },
        };
        
        let response = client.cancel_order(request).await;
        
        match response.state {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Failed to cancel order: {:?}", e)),
        }
    }
    
    async fn get_balance(&self) -> Result<Vec<Balance>> {
        debug!("Getting account balance");
        
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .context("Client not connected")?;
        
        let balances = client.fetch_balances().await
            .context("Failed to fetch balances")?;
        
        Ok(balances
            .into_iter()
            .map(|b| Balance {
                asset: b.asset.to_string(),
                free: b.balance.free.try_into().unwrap_or(0.0),
                locked: b.balance.used().try_into().unwrap_or(0.0),
                total: b.balance.total.try_into().unwrap_or(0.0),
            })
            .collect())
    }
}

impl Clone for BinanceConnector {
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
            api_key: self.api_key.clone(),
            api_secret: self.api_secret.clone(),
            sandbox: self.sandbox,
            streaming_manager: Arc::clone(&self.streaming_manager),
            market_data_subscriptions: Arc::clone(&self.market_data_subscriptions),
            base_url: self.base_url.clone(),
            rest_client: Arc::clone(&self.rest_client),
            websocket_client: Arc::clone(&self.websocket_client),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_binance_connector_creation() {
        let connector = BinanceConnector::new(None, None, true);
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_binance_connector_connect() {
        let connector = BinanceConnector::new(None, None, true).unwrap();
        let result = connector.connect().await;
        assert!(result.is_ok());
    }
}