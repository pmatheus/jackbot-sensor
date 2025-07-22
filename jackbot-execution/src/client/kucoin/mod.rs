//! KuCoin exchange client implementation.
//!
//! This module provides a REST API v3 client for KuCoin exchange,
//! supporting spot and margin trading, sub-accounts, and order management.

pub mod rest;
pub mod types;
pub mod websocket;
pub mod orderbook;

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
use rest::KuCoinRestClient;
use tokio_stream::wrappers::UnboundedReceiverStream;
pub use types::KuCoinConfig;

/// KuCoin client implementation supporting both REST and WebSocket APIs.
#[derive(Clone, Debug)]
pub struct KuCoinClient {
    config: KuCoinConfig,
    rest_client: KuCoinRestClient,
}

impl ExecutionClient for KuCoinClient {
    const EXCHANGE: ExchangeId = ExchangeId::Kucoin;
    type Config = KuCoinConfig;
    type AccountStream = UnboundedReceiverStream<UnindexedAccountEvent>;

    fn new(config: Self::Config) -> Self {
        let rest_client = KuCoinRestClient::new(config.clone());
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

        // Margin positions fetching - see KUCOIN_MARGIN_SPEC.md for margin support

        let instrument_snapshots = instruments
            .iter()
            .map(|instrument| crate::InstrumentAccountSnapshot {
                instrument: instrument.clone(),
                orders: Vec::new(), // Order fetching implementation - see KUCOIN_ORDERS_SPEC.md
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

/// KuCoin sub-account management client.
#[derive(Clone, Debug)]
pub struct KuCoinSubAccountClient {
    config: KuCoinConfig,
    rest_client: KuCoinRestClient,
}

impl KuCoinSubAccountClient {
    /// Create a new sub-account management client.
    pub fn new(config: KuCoinConfig) -> Self {
        let rest_client = KuCoinRestClient::new(config.clone());
        Self {
            config,
            rest_client,
        }
    }

    /// List all sub-accounts.
    pub async fn list_sub_accounts(&self) -> Result<Vec<SubAccount>, UnindexedClientError> {
        self.rest_client.list_sub_accounts().await
    }

    /// Create a new sub-account.
    pub async fn create_sub_account(
        &self,
        name: &str,
        password: &str,
    ) -> Result<SubAccount, UnindexedClientError> {
        self.rest_client.create_sub_account(name, password).await
    }

    /// Get sub-account balance.
    pub async fn get_sub_account_balance(
        &self,
        sub_account_id: &str,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        self.rest_client.get_sub_account_balance(sub_account_id).await
    }

    /// Transfer between accounts.
    pub async fn transfer_between_accounts(
        &self,
        transfer: &AccountTransfer,
    ) -> Result<TransferResult, UnindexedClientError> {
        self.rest_client.transfer_between_accounts(transfer).await
    }
}

/// Sub-account information.
#[derive(Debug, Clone)]
pub struct SubAccount {
    pub user_id: String,
    pub sub_name: String,
    pub sub_type: SubAccountType,
    pub created_at: DateTime<Utc>,
    pub remarks: Option<String>,
}

/// Sub-account type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SubAccountType {
    /// Normal sub-account.
    Normal,
    /// Trade sub-account.
    Trade,
    /// Margin sub-account.
    Margin,
}

/// Transfer between accounts.
#[derive(Debug, Clone)]
pub struct AccountTransfer {
    pub from_account_type: AccountType,
    pub to_account_type: AccountType,
    pub from_user_id: Option<String>, // None for main account
    pub to_user_id: Option<String>,   // None for main account
    pub currency: String,
    pub amount: rust_decimal::Decimal,
}

/// Account type for transfers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccountType {
    Main,
    Trade,
    Margin,
    Contract,
}

/// Transfer result.
#[derive(Debug, Clone)]
pub struct TransferResult {
    pub order_id: String,
    pub status: TransferStatus,
}

/// Transfer status.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransferStatus {
    Success,
    Pending,
    Failed,
}

/// KuCoin margin trading client.
#[derive(Clone, Debug)]
pub struct KuCoinMarginClient {
    config: KuCoinConfig,
    rest_client: KuCoinRestClient,
}

impl KuCoinMarginClient {
    /// Create a new margin trading client.
    pub fn new(config: KuCoinConfig) -> Self {
        let rest_client = KuCoinRestClient::new(config.clone());
        Self {
            config,
            rest_client,
        }
    }

    /// Get margin account info.
    pub async fn get_margin_account(&self) -> Result<MarginAccount, UnindexedClientError> {
        self.rest_client.get_margin_account().await
    }

    /// Borrow assets.
    pub async fn borrow(
        &self,
        currency: &str,
        amount: rust_decimal::Decimal,
    ) -> Result<BorrowResult, UnindexedClientError> {
        self.rest_client.borrow(currency, amount).await
    }

    /// Repay borrowed assets.
    pub async fn repay(
        &self,
        currency: &str,
        amount: rust_decimal::Decimal,
    ) -> Result<RepayResult, UnindexedClientError> {
        self.rest_client.repay(currency, amount).await
    }

    /// Get borrow history.
    pub async fn get_borrow_history(
        &self,
        currency: Option<&str>,
    ) -> Result<Vec<BorrowRecord>, UnindexedClientError> {
        self.rest_client.get_borrow_history(currency).await
    }
}

/// Margin account information.
#[derive(Debug, Clone)]
pub struct MarginAccount {
    pub total_asset_of_quote: rust_decimal::Decimal,
    pub total_liability_of_quote: rust_decimal::Decimal,
    pub debt_ratio: rust_decimal::Decimal,
    pub accounts: Vec<MarginAsset>,
}

/// Margin asset details.
#[derive(Debug, Clone)]
pub struct MarginAsset {
    pub currency: String,
    pub total_balance: rust_decimal::Decimal,
    pub available_balance: rust_decimal::Decimal,
    pub hold_balance: rust_decimal::Decimal,
    pub liability: rust_decimal::Decimal,
    pub max_borrow_size: rust_decimal::Decimal,
}

/// Borrow result.
#[derive(Debug, Clone)]
pub struct BorrowResult {
    pub order_id: String,
    pub currency: String,
    pub amount: rust_decimal::Decimal,
}

/// Repay result.
#[derive(Debug, Clone)]
pub struct RepayResult {
    pub order_id: String,
    pub currency: String,
    pub amount: rust_decimal::Decimal,
}

/// Borrow record.
#[derive(Debug, Clone)]
pub struct BorrowRecord {
    pub order_id: String,
    pub currency: String,
    pub amount: rust_decimal::Decimal,
    pub interest: rust_decimal::Decimal,
    pub repaid: rust_decimal::Decimal,
    pub created_at: DateTime<Utc>,
    pub status: BorrowStatus,
}

/// Borrow status.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorrowStatus {
    Active,
    Repaid,
    PartiallyRepaid,
}