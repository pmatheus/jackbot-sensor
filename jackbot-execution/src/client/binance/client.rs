use crate::{
    balance::AssetBalance,
    client::ExecutionClient,
    error::{UnindexedClientError, UnindexedOrderError},
    order::{
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::Open,
        Order,
    },
    trade::Trade,
    UnindexedAccountEvent, UnindexedAccountSnapshot,
};
use chrono::{DateTime, Utc};
use jackbot_data::{
    exchange::binance::rate_limit::{BinanceRateLimit},
};
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};
use jackbot_integration::{
    rate_limit::Priority,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{
    rest::{BinanceRestClient, BinanceRestConfig},
    websocket::BinanceWsManager,
    BinanceWsClient, BinanceWsConfig,
};

/// Configuration for the Binance client combining REST and WebSocket
#[derive(Clone)]
pub struct BinanceConfig {
    pub api_key: String,
    pub api_secret: String,
    pub testnet: bool,
    pub ws_auth_payload: String,
}

/// Combined Binance client with REST API and WebSocket support
#[derive(Clone)]
pub struct BinanceClient {
    rest_client: Arc<BinanceRestClient>,
    ws_client: Arc<BinanceWsClient>,
    ws_manager: Arc<BinanceWsManager>,
    rate_limiter: Arc<BinanceRateLimit>,
    exchange_id: ExchangeId,
}

impl BinanceClient {
    pub fn spot(config: BinanceConfig) -> Self {
        let rest_config = BinanceRestConfig {
            api_key: config.api_key.clone(),
            api_secret: config.api_secret.clone(),
            testnet: config.testnet,
        };

        let ws_config = BinanceWsConfig {
            url: if config.testnet {
                "wss://testnet.binance.vision/ws".parse().expect("Valid URL")
            } else {
                "wss://stream.binance.com:9443/ws".parse().expect("Valid URL")
            },
            auth_payload: config.ws_auth_payload,
        };

        Self {
            rest_client: Arc::new(BinanceRestClient::new(rest_config)),
            ws_client: Arc::new(BinanceWsClient::new(ws_config)),
            ws_manager: Arc::new(BinanceWsManager::new()),
            rate_limiter: Arc::new(BinanceRateLimit::new()),
            exchange_id: ExchangeId::BinanceSpot,
        }
    }

    pub fn futures(config: BinanceConfig) -> Self {
        let rest_config = BinanceRestConfig {
            api_key: config.api_key.clone(),
            api_secret: config.api_secret.clone(),
            testnet: config.testnet,
        };

        let ws_config = BinanceWsConfig {
            url: if config.testnet {
                "wss://stream.binancefuture.com/ws".parse().expect("Valid URL")
            } else {
                "wss://fstream.binance.com/ws".parse().expect("Valid URL")
            },
            auth_payload: config.ws_auth_payload,
        };

        Self {
            rest_client: Arc::new(BinanceRestClient::new(rest_config)),
            ws_client: Arc::new(BinanceWsClient::new(ws_config)),
            ws_manager: Arc::new(BinanceWsManager::new()),
            rate_limiter: Arc::new(BinanceRateLimit::new()),
            exchange_id: ExchangeId::BinanceFuturesUsd,
        }
    }

    /// Subscribe to market data streams
    pub async fn subscribe_market_data(
        &self,
        symbol: &str,
        include_trades: bool,
        include_depth: bool,
        depth_levels: u32,
    ) -> Result<mpsc::UnboundedReceiver<jackbot_data::event::MarketEvent<InstrumentNameExchange, jackbot_data::event::DataKind>>, UnindexedClientError> {
        let is_futures = matches!(self.exchange_id, ExchangeId::BinanceFuturesUsd);
        let mut streams = Vec::new();

        if include_trades {
            streams.push(format!("{}@trade", symbol.to_lowercase()));
        }

        if include_depth {
            streams.push(format!("{}@depth{}@100ms", symbol.to_lowercase(), depth_levels));
        }

        if streams.is_empty() {
            return Err(UnindexedClientError::AccountStream(
                "No streams specified".to_string(),
            ));
        }

        self.ws_manager.subscribe_combined(streams, is_futures).await
    }

    /// Get current order book snapshot via REST
    pub async fn get_order_book(
        &self,
        symbol: &str,
        limit: u32,
    ) -> Result<jackbot_data::books::canonical::CanonicalOrderBook, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;
        
        // This would need to be implemented in the REST client
        // For now, return an error
        Err(UnindexedClientError::AccountSnapshot(
            "Order book snapshot not yet implemented".to_string(),
        ))
    }
}

impl ExecutionClient for BinanceClient {
    const EXCHANGE: ExchangeId = ExchangeId::BinanceSpot; // Default to spot, actual value is in self.exchange_id

    type Config = BinanceConfig;
    type AccountStream = UnboundedReceiverStream<UnindexedAccountEvent>;

    fn new(config: Self::Config) -> Self {
        Self::spot(config)
    }

    async fn account_snapshot(
        &self,
        assets: &[AssetNameExchange],
        instruments: &[InstrumentNameExchange],
    ) -> Result<UnindexedAccountSnapshot, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::High).await;
        
        let balances = self.rest_client.fetch_balances().await?;
        let orders = self.rest_client.fetch_open_orders().await?;

        // Filter balances if specific assets were requested
        let filtered_balances = if assets.is_empty() {
            balances
        } else {
            balances
                .into_iter()
                .filter(|b| assets.contains(&b.asset))
                .collect()
        };

        // Filter orders if specific instruments were requested
        let filtered_orders = if instruments.is_empty() {
            orders
        } else {
            orders
                .into_iter()
                .filter(|o| instruments.contains(&o.key.instrument))
                .collect()
        };

        // Group orders by instrument
        let mut instruments_map = std::collections::HashMap::new();
        for order in filtered_orders {
            // Convert Order<_, _, Open> to Order<_, _, OrderState<_, _>>
            let order_snapshot = crate::order::Order {
                key: order.key.clone(),
                side: order.side,
                price: order.price,
                quantity: order.quantity,
                kind: order.kind,
                time_in_force: order.time_in_force,
                state: crate::order::state::OrderState::Active(
                    crate::order::state::ActiveOrderState::Open(order.state)
                ),
            };
            
            instruments_map
                .entry(order.key.instrument.clone())
                .or_insert_with(Vec::new)
                .push(order_snapshot);
        }
        
        let instruments = instruments_map
            .into_iter()
            .map(|(instrument, orders)| crate::InstrumentAccountSnapshot {
                instrument,
                orders,
            })
            .collect();

        Ok(UnindexedAccountSnapshot {
            exchange: self.exchange_id,
            balances: filtered_balances,
            instruments,
        })
    }

    async fn account_stream(
        &self,
        assets: &[AssetNameExchange],
        instruments: &[InstrumentNameExchange],
    ) -> Result<Self::AccountStream, UnindexedClientError> {
        // Use the WebSocket client for account updates
        self.ws_client.account_stream(assets, instruments).await
    }

    async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> UnindexedOrderResponseCancel {
        self.rate_limiter.acquire_rest(Priority::High).await;
        
        match self.rest_client.cancel_order(request.clone()).await {
            Ok(cancelled) => UnindexedOrderResponseCancel {
                key: request.key,
                state: Ok(cancelled),
            },
            Err(e) => UnindexedOrderResponseCancel {
                key: request.key,
                state: Err(e),
            },
        }
    }

    async fn open_order(
        &self,
        request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>> {
        self.rate_limiter.acquire_rest(Priority::High).await;
        
        match self.rest_client.place_order(request.clone()).await {
            Ok(order) => Order {
                key: order.key,
                side: order.side,
                price: order.price,
                quantity: order.quantity,
                kind: order.kind,
                time_in_force: order.time_in_force,
                state: Ok(order.state),
            },
            Err(e) => Order {
                key: request.key,
                side: request.state.side,
                price: request.state.price,
                quantity: request.state.quantity,
                kind: request.state.kind,
                time_in_force: request.state.time_in_force,
                state: Err(e),
            },
        }
    }

    async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;
        self.rest_client.fetch_balances().await
    }

    async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;
        self.rest_client.fetch_open_orders().await
    }

    async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;
        self.rest_client.fetch_trades(time_since).await
    }
}