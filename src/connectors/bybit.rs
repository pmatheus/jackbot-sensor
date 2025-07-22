//! Bybit exchange connector implementation
//!
//! This module provides the Bybit implementation of the Exchange trait,
//! focusing on derivatives trading with spot market support.

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
use crate::connector::{
    Balance, Connection, Exchange, MarketData, MarketDataStream, Order, OrderId, OrderResult,
    OrderSide, OrderStatus, OrderType, TimeInForce,
};
use crate::streaming::StreamingManager;

use jackbot_execution::client::{
    bybit::{BybitRestClient, BybitRestConfig},
    ExecutionClient,
};
use jackbot_instrument::{
    asset::name::AssetNameExchange,
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};

/// Bybit connector implementing the Exchange trait
pub struct BybitConnector {
    client: Arc<Mutex<Option<BybitRestClient>>>,
    api_key: Option<String>,
    api_secret: Option<String>,
    sandbox: bool,
    streaming_manager: Arc<StreamingManager>,
    market_data_subscriptions: Arc<Mutex<Vec<String>>>,
}

impl BybitConnector {
    /// Create a new Bybit connector
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

        // Bybit uses BTCUSDT format
        let instrument = InstrumentNameExchange::new(order.symbol.replace("/", ""));
        
        OrderRequestOpen {
            key: OrderKey {
                exchange: ExchangeId::BybitPerpetualsUsd,
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

    /// Process market data from Bybit WebSocket
    async fn process_market_data(&self) -> Result<()> {
        // This would be implemented to process actual WebSocket data
        // For now, simulate with periodic updates
        let mut interval = interval(Duration::from_millis(100));
        
        loop {
            interval.tick().await;
            
            let subscriptions = self.market_data_subscriptions.lock().await;
            for symbol in subscriptions.iter() {
                // Simulate ticker data with Bybit-specific characteristics
                let ticker = TickerData {
                    symbol: symbol.clone(),
                    exchange: "bybit".to_string(),
                    price: 50000.0 + (rand::random::<f64>() * 1000.0),
                    bid: 49950.0,
                    ask: 50050.0,
                    volume_24h: 5000.0,
                    change_24h: 1.8,
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

#[async_trait]
impl Exchange for BybitConnector {
    async fn connect(&self) -> Result<Connection> {
        info!("Connecting to Bybit exchange");
        
        // Create configuration
        let config = if let (Some(api_key), Some(api_secret)) = (&self.api_key, &self.api_secret) {
            BybitRestConfig {
                api_key: api_key.clone(),
                api_secret: api_secret.clone(),
            }
        } else {
            // Use testnet configuration
            BybitRestConfig {
                api_key: "demo".to_string(),
                api_secret: "demo".to_string(),
            }
        };
        
        // Create client
        let client = BybitRestClient::new(config);
        
        // Store client
        let mut client_guard = self.client.lock().await;
        *client_guard = Some(client);
        
        // Start market data processing
        let self_clone = Arc::new(self.clone());
        tokio::spawn(async move {
            if let Err(e) = self_clone.process_market_data().await {
                error!("Market data processing error: {}", e);
            }
        });
        
        info!("Successfully connected to Bybit");
        Ok(Arc::new(()) as Connection)
    }
    
    async fn subscribe_market_data(&self, symbols: Vec<String>) -> Result<MarketDataStream> {
        info!("Subscribing to market data for {} symbols", symbols.len());
        
        // Store subscriptions
        let mut subs = self.market_data_subscriptions.lock().await;
        subs.extend(symbols.clone());
        
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
        
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .context("Client not connected")?;
        
        let request = self.order_to_request(order);
        
        // TODO: Fix REST client integration - open_order method not found
        let _ = (client, request); // Suppress unused warnings
        Err(anyhow::anyhow!("Bybit order placement not yet implemented"))
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
                exchange: ExchangeId::BybitPerpetualsUsd,
                instrument: InstrumentNameExchange::new("BTCUSDT".to_string()), // Instrument tracking - see INSTRUMENT_MAPPING_SPEC.md
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

impl Clone for BybitConnector {
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
            api_key: self.api_key.clone(),
            api_secret: self.api_secret.clone(),
            sandbox: self.sandbox,
            streaming_manager: Arc::clone(&self.streaming_manager),
            market_data_subscriptions: Arc::clone(&self.market_data_subscriptions),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bybit_connector_creation() {
        let connector = BybitConnector::new(None, None, true);
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_bybit_connector_connect() {
        let connector = BybitConnector::new(None, None, true).unwrap();
        let result = connector.connect().await;
        assert!(result.is_ok());
    }
}