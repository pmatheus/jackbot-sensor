//! Hyperliquid exchange client implementation.
//!
//! This module provides an on-chain DEX client for Hyperliquid,
//! supporting perpetual futures trading with EVM-compatible Web3 integration.

pub mod rest;
pub mod types;
pub mod web3;
pub mod websocket;

// Re-export the main client types
pub use types::HyperliquidConfig;

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
use rest::HyperliquidRestClient;
use tokio_stream::wrappers::UnboundedReceiverStream;
use web3::HyperliquidWeb3Client;

/// Hyperliquid client implementation supporting REST, WebSocket, and Web3 APIs.
#[derive(Clone, Debug)]
pub struct HyperliquidClient {
    config: HyperliquidConfig,
    rest_client: HyperliquidRestClient,
    web3_client: HyperliquidWeb3Client,
}

impl ExecutionClient for HyperliquidClient {
    const EXCHANGE: ExchangeId = ExchangeId::Hyperliquid;
    type Config = HyperliquidConfig;
    type AccountStream = UnboundedReceiverStream<UnindexedAccountEvent>;

    fn new(config: Self::Config) -> Self {
        let rest_client = HyperliquidRestClient::new(config.clone());
        let web3_client = HyperliquidWeb3Client::new(config.clone());
        Self {
            config,
            rest_client,
            web3_client,
        }
    }

    async fn account_snapshot(
        &self,
        assets: &[AssetNameExchange],
        instruments: &[InstrumentNameExchange],
    ) -> Result<UnindexedAccountSnapshot, UnindexedClientError> {
        // Fetch on-chain balances
        let balances = if assets.is_empty() {
            self.web3_client.fetch_all_balances().await?
        } else {
            self.web3_client.fetch_specific_balances(assets).await?
        };

        // Fetch positions for specified instruments
        let positions = if instruments.is_empty() {
            self.rest_client.fetch_all_positions().await?
        } else {
            self.rest_client.fetch_positions(instruments).await?
        };

        let instrument_snapshots = instruments
            .iter()
            .map(|instrument| crate::InstrumentAccountSnapshot {
                instrument: instrument.clone(),
                orders: Vec::new(), // Order fetching implementation - see HYPERLIQUID_ORDERS_SPEC.md
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
        match self.web3_client.cancel_order(&request).await {
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
        match self.web3_client.place_order(&request).await {
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
        self.web3_client.fetch_all_balances().await
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

/// Hyperliquid liquidation monitoring client.
#[derive(Clone, Debug)]
pub struct HyperliquidLiquidationClient {
    config: HyperliquidConfig,
    rest_client: HyperliquidRestClient,
}

impl HyperliquidLiquidationClient {
    /// Create a new liquidation monitoring client.
    pub fn new(config: HyperliquidConfig) -> Self {
        let rest_client = HyperliquidRestClient::new(config.clone());
        Self {
            config,
            rest_client,
        }
    }

    /// Get recent liquidations.
    pub async fn get_liquidations(
        &self,
        instrument: Option<&InstrumentNameExchange>,
        limit: Option<usize>,
    ) -> Result<Vec<Liquidation>, UnindexedClientError> {
        self.rest_client.get_liquidations(instrument, limit).await
    }

    /// Subscribe to liquidation events via WebSocket.
    pub async fn liquidation_stream(
        &self,
    ) -> Result<UnboundedReceiverStream<Liquidation>, UnindexedClientError> {
        websocket::create_liquidation_stream(&self.config).await
    }
}

/// Liquidation event data.
#[derive(Debug, Clone)]
pub struct Liquidation {
    pub liquidation_id: String,
    pub account: String,
    pub instrument: InstrumentNameExchange,
    pub side: jackbot_instrument::Side,
    pub price: rust_decimal::Decimal,
    pub quantity: rust_decimal::Decimal,
    pub time: DateTime<Utc>,
    pub liquidation_type: LiquidationType,
}

/// Type of liquidation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LiquidationType {
    /// Partial liquidation to reduce position risk.
    Partial,
    /// Full liquidation of entire position.
    Full,
    /// Auto-deleverage event.
    AutoDeleverage,
}

/// Gas estimation result for on-chain transactions.
#[derive(Debug, Clone)]
pub struct GasEstimate {
    pub gas_limit: u64,
    pub gas_price: u64,
    pub total_cost_wei: u128,
    pub total_cost_eth: rust_decimal::Decimal,
}