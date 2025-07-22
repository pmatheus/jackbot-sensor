//! Bitget exchange client implementation.
//!
//! This module provides a REST API v2 client for Bitget exchange,
//! supporting spot and futures trading, order management, and account operations.

pub mod rest;
pub mod types;
pub mod websocket;

// Re-export the main client types
pub use types::BitgetConfig;

use crate::{
    balance::{AssetBalance, Balance},
    client::ExecutionClient,
    error::{UnindexedClientError, UnindexedOrderError, ConnectivityError},
    order::{
        id::{ClientOrderId, OrderId, StrategyId},
        request::{OrderRequestCancel, OrderRequestOpen, UnindexedOrderResponseCancel},
        state::{Cancelled, Open},
        Order, OrderKey,
    },
    trade::Trade,
    UnindexedAccountEvent, UnindexedAccountSnapshot,
};
use chrono::{DateTime, Utc};
use futures::Stream;
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
};
use rest::BitgetRestClient;
use std::pin::Pin;
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Bitget client implementation supporting both REST and WebSocket APIs.
#[derive(Clone, Debug)]
pub struct BitgetClient {
    config: BitgetConfig,
    rest_client: BitgetRestClient,
}

impl ExecutionClient for BitgetClient {
    const EXCHANGE: ExchangeId = ExchangeId::Bitget;
    type Config = BitgetConfig;
    type AccountStream = UnboundedReceiverStream<UnindexedAccountEvent>;

    fn new(config: Self::Config) -> Self {
        let rest_client = BitgetRestClient::new(config.clone());
        Self {
            config,
            rest_client,
        }
    }

    async fn account_snapshot(
        &self,
        assets: &[AssetNameExchange],
        instruments: &[InstrumentNameExchange],
    ) -> Result<UnindexedAccountSnapshot, UnindexedClientError> {
        // Fetch balances for specified assets
        let balances = if assets.is_empty() {
            self.rest_client.fetch_all_balances().await?
        } else {
            self.rest_client.fetch_specific_balances(assets).await?
        };

        // Futures positions fetching - see BITGET_FUTURES_SPEC.md for futures support

        let instrument_snapshots = instruments
            .iter()
            .map(|instrument| crate::InstrumentAccountSnapshot {
                instrument: instrument.clone(),
                orders: Vec::new(), // Order fetching implementation - see BITGET_ORDERS_SPEC.md
            })
            .collect();

        Ok(UnindexedAccountSnapshot {
            exchange: Self::EXCHANGE,
            balances,
            instruments: instrument_snapshots,
        })
    }

    async fn account_stream(
        &self,
        assets: &[AssetNameExchange],
        instruments: &[InstrumentNameExchange],
    ) -> Result<Self::AccountStream, UnindexedClientError> {
        websocket::create_account_stream(&self.config, assets, instruments).await
    }

    async fn cancel_order(
        &self,
        request: OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> UnindexedOrderResponseCancel {
        match self.rest_client.cancel_order(&request).await {
            Ok(cancelled_state) => UnindexedOrderResponseCancel {
                key: request.key,
                state: Ok(cancelled_state),
            },
            Err(err) => UnindexedOrderResponseCancel {
                key: request.key,
                state: Err(UnindexedOrderError::Connectivity(ConnectivityError::Socket(err.to_string()))),
            },
        }
    }

    async fn open_order(
        &self,
        request: OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>> {
        match self.rest_client.place_order(&request).await {
            Ok(open_state) => Order {
                key: request.key,
                side: request.state.side,
                price: request.state.price,
                quantity: request.state.quantity,
                kind: request.state.kind,
                time_in_force: request.state.time_in_force,
                state: Ok(open_state),
            },
            Err(err) => Order {
                key: request.key,
                side: request.state.side,
                price: request.state.price,
                quantity: request.state.quantity,
                kind: request.state.kind,
                time_in_force: request.state.time_in_force,
                state: Err(match err {
                    UnindexedClientError::Connectivity(conn_err) => UnindexedOrderError::Connectivity(conn_err),
                    UnindexedClientError::Api(api_err) => UnindexedOrderError::Rejected(api_err),
                    UnindexedClientError::AccountSnapshot(_) |
                    UnindexedClientError::AccountStream(_) |
                    UnindexedClientError::Other(_) => 
                        UnindexedOrderError::Connectivity(ConnectivityError::Socket(err.to_string())),
                }),
            },
        }
    }

    async fn fetch_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        self.rest_client.fetch_all_balances().await
    }

    async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        self.rest_client.fetch_open_orders().await
    }

    async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        self.rest_client.fetch_trades(time_since).await
    }
}

/// Bitget copy trading client for following master traders.
#[derive(Clone, Debug)]
pub struct BitgetCopyTradingClient {
    config: BitgetConfig,
    rest_client: BitgetRestClient,
}

impl BitgetCopyTradingClient {
    /// Create a new copy trading client.
    pub fn new(config: BitgetConfig) -> Self {
        let rest_client = BitgetRestClient::new(config.clone());
        Self {
            config,
            rest_client,
        }
    }

    /// Get list of available master traders.
    pub async fn list_master_traders(&self) -> Result<Vec<MasterTrader>, UnindexedClientError> {
        self.rest_client.list_master_traders().await
    }

    /// Follow a master trader.
    pub async fn follow_trader(&self, trader_id: &str) -> Result<(), UnindexedClientError> {
        self.rest_client.follow_trader(trader_id).await
    }

    /// Unfollow a master trader.
    pub async fn unfollow_trader(&self, trader_id: &str) -> Result<(), UnindexedClientError> {
        self.rest_client.unfollow_trader(trader_id).await
    }

    /// Get copy trading settings.
    pub async fn get_copy_settings(&self) -> Result<CopySettings, UnindexedClientError> {
        self.rest_client.get_copy_settings().await
    }

    /// Update copy trading settings.
    pub async fn update_copy_settings(
        &self,
        settings: &CopySettings,
    ) -> Result<(), UnindexedClientError> {
        self.rest_client.update_copy_settings(settings).await
    }
}

/// Master trader information.
#[derive(Debug, Clone)]
pub struct MasterTrader {
    pub trader_id: String,
    pub nickname: String,
    pub profit_rate: f64,
    pub win_rate: f64,
    pub followers: u32,
    pub total_assets: f64,
}

/// Copy trading settings.
#[derive(Debug, Clone)]
pub struct CopySettings {
    pub max_position_size: f64,
    pub max_daily_loss: f64,
    pub copy_ratio: f64,
    pub stop_loss_percentage: Option<f64>,
    pub take_profit_percentage: Option<f64>,
}