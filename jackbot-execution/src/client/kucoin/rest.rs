//! KuCoin REST API v3 client implementation.

use super::types::*;
use crate::{
    balance::{AssetBalance, Balance},
    error::UnindexedClientError,
    order::{
        id::{ClientOrderId, OrderId},
        request::{OrderRequestCancel, OrderRequestOpen},
        state::{Cancelled, Open},
        Order, OrderKey, OrderKind, TimeInForce,
    },
    trade::Trade,
};
use chrono::{DateTime, TimeZone, Utc};
use hmac::{Hmac, Mac};
use jackbot_data::exchange::kucoin::rate_limit::KucoinRateLimit;
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};
use jackbot_integration::rate_limit::Priority;
use reqwest::{Client, Method};
use rust_decimal::Decimal;
use sha2::Sha256;
use std::str::FromStr;
use tracing::{debug, error, warn};
use base64::{Engine as _, engine::general_purpose};

type HmacSha256 = Hmac<Sha256>;

/// KuCoin REST API client.
#[derive(Clone, Debug)]
pub struct KuCoinRestClient {
    config: KuCoinConfig,
    client: Client,
    rate_limiter: KucoinRateLimit,
}

impl KuCoinRestClient {
    /// Create a new REST API client.
    pub fn new(config: KuCoinConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            rate_limiter: KucoinRateLimit::new(),
        }
    }

    /// Get WebSocket connection info.
    pub async fn get_ws_connection_info(&self) -> Result<KuCoinWsConnection, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v1/bullet-private";
        let response = self.signed_request(Method::POST, endpoint, None).await?;
        
        let result: KuCoinResponse<KuCoinWsConnection> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin API error: {} - {:?}",
                result.code, result.msg
            )));
        }

        result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in WebSocket connection response".to_string())
        })
    }

    /// Fetch all account balances.
    pub async fn fetch_all_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v1/accounts";
        let response = self.signed_request(Method::GET, endpoint, None).await?;
        
        let result: KuCoinResponse<Vec<KuCoinBalance>> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin API error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in balance response".to_string())
        })?;

        data.into_iter()
            .filter(|b| b.r#type == "trade") // Only trade accounts for now
            .filter_map(|b| self.parse_balance(b))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Fetch specific asset balances.
    pub async fn fetch_specific_balances(
        &self,
        assets: &[AssetNameExchange],
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        let all_balances = self.fetch_all_balances().await?;
        Ok(all_balances
            .into_iter()
            .filter(|b| assets.contains(&b.asset))
            .collect())
    }

    /// Place a new order.
    pub async fn place_order(
        &self,
        request: &OrderRequestOpen<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Open, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::High).await;

        let endpoint = "/api/v1/orders";

        let order_type = match request.state.kind {
            OrderKind::Market => "market",
            OrderKind::Limit => "limit",
            OrderKind::Stop => "market", // KuCoin doesn't support stop orders directly
            OrderKind::StopLimit => "limit",
            OrderKind::Jackpot | OrderKind::Prophetic | OrderKind::EventTriggered => "market",
        };

        let side = match request.state.side {
            Side::Buy => "buy",
            Side::Sell => "sell",
        };

        let mut order_req = KuCoinOrderRequest {
            client_oid: request.key.cid.as_ref().to_string(),
            side: side.to_string(),
            symbol: request.key.instrument.as_ref().to_string(),
            r#type: Some(order_type.to_string()),
            price: if order_type == "limit" {
                Some(request.state.price.to_string())
            } else {
                None
            },
            size: Some(request.state.quantity.to_string()),
            funds: None,
            time_in_force: match request.state.time_in_force {
                TimeInForce::GoodUntilCancelled { post_only } => {
                    if post_only {
                        Some("PO".to_string())
                    } else {
                        Some("GTC".to_string())
                    }
                }
                TimeInForce::ImmediateOrCancel => Some("IOC".to_string()),
                TimeInForce::FillOrKill => Some("FOK".to_string()),
                TimeInForce::GoodUntilEndOfDay => Some("GTC".to_string()),
            },
            post_only: match request.state.time_in_force {
                TimeInForce::GoodUntilCancelled { post_only } => Some(post_only),
                _ => None,
            },
            hidden: None,
            iceberg: None,
            visible_size: None,
        };

        let body = serde_json::to_string(&order_req)
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        let response = self
            .signed_request(Method::POST, endpoint, Some(body))
            .await?;

        let result: KuCoinResponse<KuCoinOrderResponse> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin order placement error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in order response".to_string())
        })?;

        Ok(Open {
            id: OrderId::new(&data.order_id),
            time_exchange: Utc::now(),
            filled_quantity: Decimal::ZERO,
        })
    }

    /// Cancel an order.
    pub async fn cancel_order(
        &self,
        request: &OrderRequestCancel<ExchangeId, InstrumentNameExchange>,
    ) -> Result<Cancelled, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::High).await;

        let order_id = request.state.id.as_ref()
            .map(|id| id.as_ref())
            .ok_or_else(|| UnindexedClientError::Other("Order ID required for cancellation".to_string()))?;

        let endpoint = format!("/api/v1/orders/{}", order_id);

        let response = self
            .signed_request(Method::DELETE, &endpoint, None)
            .await?;

        let result: KuCoinResponse<serde_json::Value> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin order cancellation error: {} - {:?}",
                result.code, result.msg
            )));
        }

        Ok(Cancelled {
            id: request.state.id.clone().unwrap_or_else(|| OrderId::new("")),
            time_exchange: Utc::now(),
        })
    }

    /// Fetch open orders.
    pub async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v1/orders?status=active";
        let response = self.signed_request(Method::GET, endpoint, None).await?;
        
        let result: KuCoinResponse<KuCoinPaginatedResponse<KuCoinOrderInfo>> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin fetch orders error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in orders response".to_string())
        })?;

        data.items
            .into_iter()
            .filter_map(|order| self.parse_order_info(order))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Fetch trade history.
    pub async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = format!("/api/v1/fills?startAt={}", time_since.timestamp_millis());
        let response = self.signed_request(Method::GET, &endpoint, None).await?;
        
        let result: KuCoinResponse<KuCoinPaginatedResponse<KuCoinFill>> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin fetch trades error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in trades response".to_string())
        })?;

        data.items
            .into_iter()
            .filter_map(|fill| self.parse_fill(fill))
            .collect::<Result<Vec<_>, _>>()
    }

    // Sub-account methods

    /// List all sub-accounts.
    pub async fn list_sub_accounts(&self) -> Result<Vec<super::SubAccount>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v1/sub/user";
        let response = self.signed_request(Method::GET, endpoint, None).await?;
        
        let result: KuCoinResponse<Vec<KuCoinSubAccount>> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin list sub-accounts error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in sub-accounts response".to_string())
        })?;

        Ok(data.into_iter().map(|sa| self.parse_sub_account(sa)).collect())
    }

    /// Create a new sub-account.
    pub async fn create_sub_account(
        &self,
        name: &str,
        password: &str,
    ) -> Result<super::SubAccount, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v1/sub/user";
        let req = KuCoinSubAccountCreate {
            sub_name: name.to_string(),
            password: password.to_string(),
            remarks: None,
        };

        let body = serde_json::to_string(&req)
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        let response = self
            .signed_request(Method::POST, endpoint, Some(body))
            .await?;

        let result: KuCoinResponse<KuCoinSubAccount> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin create sub-account error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in create sub-account response".to_string())
        })?;

        Ok(self.parse_sub_account(data))
    }

    /// Get sub-account balance.
    pub async fn get_sub_account_balance(
        &self,
        sub_account_id: &str,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = format!("/api/v1/sub-accounts/{}", sub_account_id);
        let response = self.signed_request(Method::GET, &endpoint, None).await?;
        
        let result: KuCoinResponse<Vec<KuCoinBalance>> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin sub-account balance error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in sub-account balance response".to_string())
        })?;

        data.into_iter()
            .filter(|b| b.r#type == "trade") // Only trade accounts for now
            .filter_map(|b| self.parse_balance(b))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Transfer between accounts.
    pub async fn transfer_between_accounts(
        &self,
        transfer: &super::AccountTransfer,
    ) -> Result<super::TransferResult, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v2/accounts/inner-transfer";
        
        let from = match transfer.from_account_type {
            super::AccountType::Main => "main",
            super::AccountType::Trade => "trade",
            super::AccountType::Margin => "margin",
            super::AccountType::Contract => "contract",
        };

        let to = match transfer.to_account_type {
            super::AccountType::Main => "main",
            super::AccountType::Trade => "trade",
            super::AccountType::Margin => "margin",
            super::AccountType::Contract => "contract",
        };

        let req = KuCoinTransferRequest {
            currency: transfer.currency.clone(),
            amount: transfer.amount.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            from_user_id: transfer.from_user_id.clone(),
            to_user_id: transfer.to_user_id.clone(),
        };

        let body = serde_json::to_string(&req)
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        let response = self
            .signed_request(Method::POST, endpoint, Some(body))
            .await?;

        let result: KuCoinResponse<KuCoinTransferResponse> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin transfer error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in transfer response".to_string())
        })?;

        Ok(super::TransferResult {
            order_id: data.order_id,
            status: super::TransferStatus::Success,
        })
    }

    // Margin trading methods

    /// Get margin account info.
    pub async fn get_margin_account(&self) -> Result<super::MarginAccount, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v1/margin/account";
        let response = self.signed_request(Method::GET, endpoint, None).await?;
        
        let result: KuCoinResponse<KuCoinMarginAccount> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin margin account error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in margin account response".to_string())
        })?;

        self.parse_margin_account(data)
    }

    /// Borrow assets.
    pub async fn borrow(
        &self,
        currency: &str,
        amount: Decimal,
    ) -> Result<super::BorrowResult, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::High).await;

        let endpoint = "/api/v1/margin/borrow";
        let req = KuCoinBorrowRequest {
            currency: currency.to_string(),
            size: amount.to_string(),
            r#type: "IOC".to_string(),
        };

        let body = serde_json::to_string(&req)
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        let response = self
            .signed_request(Method::POST, endpoint, Some(body))
            .await?;

        let result: KuCoinResponse<KuCoinBorrowResponse> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin borrow error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in borrow response".to_string())
        })?;

        Ok(super::BorrowResult {
            order_id: data.order_id,
            currency: data.currency,
            amount: Decimal::from_str(&data.actual_size)
                .map_err(|e| UnindexedClientError::Other(e.to_string()))?,
        })
    }

    /// Repay borrowed assets.
    pub async fn repay(
        &self,
        currency: &str,
        amount: Decimal,
    ) -> Result<super::RepayResult, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::High).await;

        let endpoint = "/api/v1/margin/repay";
        let req = KuCoinRepayRequest {
            currency: currency.to_string(),
            size: amount.to_string(),
        };

        let body = serde_json::to_string(&req)
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        let response = self
            .signed_request(Method::POST, endpoint, Some(body))
            .await?;

        let result: KuCoinResponse<KuCoinRepayResponse> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin repay error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in repay response".to_string())
        })?;

        Ok(super::RepayResult {
            order_id: data.order_id,
            currency: data.currency,
            amount: Decimal::from_str(&data.actual_size)
                .map_err(|e| UnindexedClientError::Other(e.to_string()))?,
        })
    }

    /// Get borrow history.
    pub async fn get_borrow_history(
        &self,
        currency: Option<&str>,
    ) -> Result<Vec<super::BorrowRecord>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let mut endpoint = "/api/v1/margin/borrow/outstanding".to_string();
        if let Some(curr) = currency {
            endpoint.push_str(&format!("?currency={}", curr));
        }

        let response = self.signed_request(Method::GET, &endpoint, None).await?;
        
        let result: KuCoinResponse<KuCoinPaginatedResponse<KuCoinBorrowRecord>> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "200000" {
            return Err(UnindexedClientError::Other(format!(
                "KuCoin borrow history error: {} - {:?}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in borrow history response".to_string())
        })?;

        data.items
            .into_iter()
            .filter_map(|record| self.parse_borrow_record(record))
            .collect::<Result<Vec<_>, _>>()
    }

    // Helper methods

    /// Create a signed request.
    async fn signed_request(
        &self,
        method: Method,
        endpoint: &str,
        body: Option<String>,
    ) -> Result<reqwest::Response, UnindexedClientError> {
        let timestamp = Utc::now().timestamp_millis().to_string();
        let url = format!("{}{}", self.config.rest_url, endpoint);

        let str_to_sign = format!(
            "{}{}{}{}",
            timestamp,
            method.as_str(),
            endpoint,
            body.as_deref().unwrap_or("")
        );

        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(str_to_sign.as_bytes());
        let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        let passphrase_payload = format!("{}:{}", self.config.api_key, self.config.passphrase);
        let mut passphrase_mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        passphrase_mac.update(passphrase_payload.as_bytes());
        let passphrase_sign = general_purpose::STANDARD.encode(passphrase_mac.finalize().into_bytes());

        let mut request = self.client.request(method, &url)
            .header("KC-API-KEY", &self.config.api_key)
            .header("KC-API-SIGN", signature)
            .header("KC-API-TIMESTAMP", timestamp)
            .header("KC-API-PASSPHRASE", passphrase_sign)
            .header("KC-API-KEY-VERSION", &self.config.api_version)
            .header("Content-Type", "application/json");

        if let Some(body_str) = body {
            request = request.body(body_str);
        }

        request
            .send()
            .await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))
    }

    /// Parse balance.
    fn parse_balance(
        &self,
        balance: KuCoinBalance,
    ) -> Option<Result<AssetBalance<AssetNameExchange>, UnindexedClientError>> {
        let available = Decimal::from_str(&balance.available).ok()?;
        let holds = Decimal::from_str(&balance.holds).ok()?;
        let total = Decimal::from_str(&balance.balance).ok()?;

        Some(Ok(AssetBalance {
            asset: AssetNameExchange::new(balance.currency),
            balance: Balance {
                total,
                free: available,
            },
            time_exchange: Utc::now(),
        }))
    }

    /// Parse order info into Order struct.
    fn parse_order_info(
        &self,
        info: KuCoinOrderInfo,
    ) -> Option<Result<Order<ExchangeId, InstrumentNameExchange, Open>, UnindexedClientError>> {
        let side = match info.side.as_str() {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => return None,
        };

        let kind = match info.r#type.as_str() {
            "limit" => OrderKind::Limit,
            "market" => OrderKind::Market,
            _ => return None,
        };

        let price = Decimal::from_str(&info.price).ok()?;
        let quantity = Decimal::from_str(&info.size).ok()?;
        let filled_quantity = Decimal::from_str(&info.deal_size).ok()?;
        let time = Utc.timestamp_millis_opt(info.created_at).single()?;

        Some(Ok(Order {
            key: OrderKey {
                exchange: ExchangeId::Kucoin,
                instrument: InstrumentNameExchange::new(info.symbol),
                strategy: Default::default(),
                cid: ClientOrderId::new(info.client_oid),
            },
            side,
            price,
            quantity,
            kind,
            time_in_force: match info.time_in_force.as_str() {
                "GTC" => TimeInForce::GoodUntilCancelled { post_only: info.post_only },
                "IOC" => TimeInForce::ImmediateOrCancel,
                "FOK" => TimeInForce::FillOrKill,
                "PO" => TimeInForce::GoodUntilCancelled { post_only: true },
                _ => TimeInForce::GoodUntilCancelled { post_only: false },
            },
            state: Open {
                id: OrderId::new(info.id),
                time_exchange: time,
                filled_quantity,
            },
        }))
    }

    /// Parse fill into Trade struct.
    fn parse_fill(
        &self,
        fill: KuCoinFill,
    ) -> Option<Result<Trade<QuoteAsset, InstrumentNameExchange>, UnindexedClientError>> {
        let side = match fill.side.as_str() {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => return None,
        };

        let price = Decimal::from_str(&fill.price).ok()?;
        let quantity = Decimal::from_str(&fill.size).ok()?;
        let fee = Decimal::from_str(&fill.fee).ok()?;
        let time = Utc.timestamp_millis_opt(fill.created_at).single()
            .unwrap_or_else(Utc::now);

        Some(Ok(Trade {
            id: crate::trade::TradeId::new(&fill.id),
            order_id: OrderId::new(&fill.order_id),
            instrument: InstrumentNameExchange::new(fill.symbol),
            strategy: crate::order::id::StrategyId::unknown(),
            time_exchange: time,
            side,
            price,
            quantity,
            fees: crate::trade::AssetFees::quote_fees(fee),
        }))
    }

    /// Parse sub-account.
    fn parse_sub_account(&self, sa: KuCoinSubAccount) -> super::SubAccount {
        super::SubAccount {
            user_id: sa.user_id,
            sub_name: sa.sub_name,
            sub_type: match sa.r#type {
                0 => super::SubAccountType::Normal,
                1 => super::SubAccountType::Trade,
                2 => super::SubAccountType::Margin,
                _ => super::SubAccountType::Normal,
            },
            created_at: Utc.timestamp_millis_opt(sa.created_at).single()
                .unwrap_or_else(Utc::now),
            remarks: sa.remarks,
        }
    }

    /// Parse margin account.
    fn parse_margin_account(
        &self,
        account: KuCoinMarginAccount,
    ) -> Result<super::MarginAccount, UnindexedClientError> {
        let total_asset = Decimal::from_str(&account.total_asset_of_quote)
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;
        let total_liability = Decimal::from_str(&account.total_liability_of_quote)
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;
        let debt_ratio = Decimal::from_str(&account.debt_ratio)
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        let accounts = account.accounts
            .into_iter()
            .filter_map(|asset| self.parse_margin_asset(asset).ok())
            .collect();

        Ok(super::MarginAccount {
            total_asset_of_quote: total_asset,
            total_liability_of_quote: total_liability,
            debt_ratio,
            accounts,
        })
    }

    /// Parse margin asset.
    fn parse_margin_asset(
        &self,
        asset: KuCoinMarginAsset,
    ) -> Result<super::MarginAsset, UnindexedClientError> {
        Ok(super::MarginAsset {
            currency: asset.currency,
            total_balance: Decimal::from_str(&asset.total_balance)
                .map_err(|e| UnindexedClientError::Other(e.to_string()))?,
            available_balance: Decimal::from_str(&asset.available)
                .map_err(|e| UnindexedClientError::Other(e.to_string()))?,
            hold_balance: Decimal::from_str(&asset.hold)
                .map_err(|e| UnindexedClientError::Other(e.to_string()))?,
            liability: Decimal::from_str(&asset.borrowed)
                .map_err(|e| UnindexedClientError::Other(e.to_string()))?,
            max_borrow_size: Decimal::from_str(&asset.max_borrow_size)
                .map_err(|e| UnindexedClientError::Other(e.to_string()))?,
        })
    }

    /// Parse borrow record.
    fn parse_borrow_record(
        &self,
        record: KuCoinBorrowRecord,
    ) -> Option<Result<super::BorrowRecord, UnindexedClientError>> {
        let amount = Decimal::from_str(&record.size).ok()?;
        let created_at = Utc.timestamp_millis_opt(record.created_at).single()?;

        let status = match record.status.as_str() {
            "Active" => super::BorrowStatus::Active,
            "Repaid" => super::BorrowStatus::Repaid,
            "PartiallyRepaid" => super::BorrowStatus::PartiallyRepaid,
            _ => return None,
        };

        Some(Ok(super::BorrowRecord {
            order_id: record.order_id,
            currency: record.currency,
            amount,
            interest: Decimal::ZERO, // Would need additional API call
            repaid: Decimal::ZERO,    // Would need additional API call
            created_at,
            status,
        }))
    }
}