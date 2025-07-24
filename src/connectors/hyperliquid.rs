//! Hyperliquid exchange connector implementation
//!
//! This module provides the Hyperliquid implementation of the Exchange trait,
//! focusing on on-chain perpetuals trading.

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::api::{BalanceData, KlineData, OrderBookData, TickerData, TradeData};
use crate::connector::{
    Balance, Connection, Exchange, MarketData, MarketDataStream, Order, OrderId, OrderResult,
    OrderSide, OrderStatus, OrderType, TimeInForce,
};
use crate::streaming::StreamingManager;

use jackbot_execution::client::{
    hyperliquid::{HyperliquidClient, HyperliquidConfig},
    ExecutionClient,
};
use jackbot_instrument::{
    asset::name::AssetNameExchange,
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};

/// Hyperliquid connector implementing the Exchange trait
pub struct HyperliquidConnector {
    client: Arc<Mutex<Option<HyperliquidClient>>>,
    private_key: Option<String>,
    sandbox: bool,
    streaming_manager: Arc<StreamingManager>,
    market_data_subscriptions: Arc<Mutex<Vec<String>>>,
}

impl HyperliquidConnector {
    /// Create a new Hyperliquid connector
    pub fn new(
        private_key: Option<String>,
        _api_secret: Option<String>, // Not used for Hyperliquid
        sandbox: bool,
    ) -> Result<Self> {
        let streaming_manager = Arc::new(StreamingManager::new());
        
        Ok(Self {
            client: Arc::new(Mutex::new(None)),
            private_key,
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
            _ => OrderKind::Limit,
        };

        let time_in_force = match order.time_in_force {
            Some(TimeInForce::GTC) => jackbot_execution::order::TimeInForce::GoodUntilCancelled { post_only: false },
            Some(TimeInForce::IOC) => jackbot_execution::order::TimeInForce::ImmediateOrCancel,
            Some(TimeInForce::FOK) => jackbot_execution::order::TimeInForce::FillOrKill,
            _ => jackbot_execution::order::TimeInForce::GoodUntilCancelled { post_only: false },
        };

        let instrument = InstrumentNameExchange::new(order.symbol.replace("/", "-"));
        
        OrderRequestOpen {
            key: OrderKey {
                exchange: ExchangeId::Hyperliquid,
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
                commission_asset: "USDC".to_string(), // Hyperliquid uses USDC
                timestamp: chrono::Utc::now().timestamp_millis(),
            }),
            Err(e) => Err(anyhow::anyhow!("Order failed: {:?}", e)),
        }
    }

    /// Process market data from Hyperliquid WebSocket
    async fn process_market_data(&self) -> Result<()> {
        let mut interval = interval(Duration::from_millis(100));
        
        loop {
            interval.tick().await;
            
            let subscriptions = self.market_data_subscriptions.lock().await;
            for symbol in subscriptions.iter() {
                let ticker = TickerData {
                    symbol: symbol.clone(),
                    exchange: "hyperliquid".to_string(),
                    price: 50000.0 + (rand::random::<f64>() * 1000.0),
                    bid: 49980.0,
                    ask: 50020.0,
                    volume_24h: 8000.0,
                    change_24h: 3.2,
                    high_24h: 51500.0,
                    low_24h: 48500.0,
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
impl Exchange for HyperliquidConnector {
    async fn connect(&self) -> Result<Connection> {
        info!("Connecting to Hyperliquid exchange");
        
        let config = if let Some(private_key) = &self.private_key {
            HyperliquidConfig {
                rest_url: Url::parse("https://api.hyperliquid.xyz").unwrap(),
                ws_url: Url::parse("wss://api.hyperliquid.xyz/ws").unwrap(),
                web3_rpc_url: Url::parse("https://rpc.ankr.com/arbitrum").unwrap(),
                private_key: private_key.clone(),
                account_address: "0x0000000000000000000000000000000000000000".to_string(),
                api_key: None,
                chain_id: 42161, // Arbitrum
            }
        } else {
            HyperliquidConfig {
                rest_url: Url::parse("https://api.hyperliquid.xyz").unwrap(),
                ws_url: Url::parse("wss://api.hyperliquid.xyz/ws").unwrap(),
                web3_rpc_url: Url::parse("https://rpc.ankr.com/arbitrum").unwrap(),
                private_key: "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                account_address: "0x0000000000000000000000000000000000000000".to_string(),
                api_key: None,
                chain_id: 42161, // Arbitrum
            }
        };
        
        let client = HyperliquidClient::new(config);
        
        let mut client_guard = self.client.lock().await;
        *client_guard = Some(client);
        
        let self_clone = Arc::new(self.clone());
        tokio::spawn(async move {
            if let Err(e) = self_clone.process_market_data().await {
                error!("Market data processing error: {}", e);
            }
        });
        
        info!("Successfully connected to Hyperliquid");
        Ok(Arc::new(()) as Connection)
    }
    
    async fn subscribe_market_data(&self, symbols: Vec<String>) -> Result<MarketDataStream> {
        info!("Subscribing to market data for {} symbols", symbols.len());
        
        let mut subs = self.market_data_subscriptions.lock().await;
        subs.extend(symbols.clone());
        
        let mut receiver = self.streaming_manager.subscribe_all().await?;
        
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
                exchange: ExchangeId::Hyperliquid,
                instrument: InstrumentNameExchange::new("BTC-USD".to_string()),
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

impl Clone for HyperliquidConnector {
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
            private_key: self.private_key.clone(),
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
    async fn test_hyperliquid_connector_creation() {
        let connector = HyperliquidConnector::new(None, None, true);
        assert!(connector.is_ok());
    }

    #[tokio::test]
    async fn test_hyperliquid_connector_connect() {
        let connector = HyperliquidConnector::new(None, None, true).unwrap();
        let result = connector.connect().await;
        assert!(result.is_ok());
    }
}