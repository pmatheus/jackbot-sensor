//! Bitget REST API v2 client implementation.

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
use jackbot_data::exchange::bitget::rate_limit::BitgetRateLimit;
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};
use jackbot_integration::rate_limit::Priority;
use reqwest::{Client, Method, RequestBuilder};
use rust_decimal::Decimal;
use sha2::Sha256;
use std::str::FromStr;
use tracing::{debug, error, warn};

type HmacSha256 = Hmac<Sha256>;

/// Bitget REST API client.
#[derive(Clone, Debug)]
pub struct BitgetRestClient {
    config: BitgetConfig,
    client: Client,
    rate_limiter: BitgetRateLimit,
}

impl BitgetRestClient {
    /// Create a new REST API client.
    pub fn new(config: BitgetConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            rate_limiter: BitgetRateLimit::new(),
        }
    }

    /// Fetch all account balances.
    pub async fn fetch_all_balances(
        &self,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = match self.config.trading_mode {
            TradingMode::Spot => "/api/v2/spot/account/assets",
            TradingMode::Futures => "/api/v2/mix/account/accounts",
        };

        let response = self.signed_request(Method::GET, endpoint, None).await?;
        let result: BitgetResponse<Vec<serde_json::Value>> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "00000" {
            return Err(UnindexedClientError::Other(format!(
                "Bitget API error: {} - {}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in balance response".to_string())
        })?;

        match self.config.trading_mode {
            TradingMode::Spot => self.parse_spot_balances(data),
            TradingMode::Futures => self.parse_futures_balances(data),
        }
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

        let endpoint = match self.config.trading_mode {
            TradingMode::Spot => "/api/v2/spot/trade/place-order",
            TradingMode::Futures => "/api/v2/mix/order/place-order",
        };

        let order_type = match request.state.kind {
            OrderKind::Market => "market",
            OrderKind::Limit => "limit",
            OrderKind::Stop => "market", // Bitget doesn't support stop orders directly
            OrderKind::StopLimit => "limit",
            OrderKind::Jackpot | OrderKind::Prophetic | OrderKind::EventTriggered => "market",
        };

        let side = match request.state.side {
            Side::Buy => "buy",
            Side::Sell => "sell",
        };

        let mut order_req = BitgetOrderRequest {
            symbol: request.key.instrument.as_ref().to_string(),
            side: side.to_string(),
            order_type: order_type.to_string(),
            size: request.state.quantity.to_string(),
            price: if order_type == "limit" {
                Some(request.state.price.to_string())
            } else {
                None
            },
            client_oid: request.key.cid.as_ref().to_string(),
            time_in_force: match request.state.time_in_force {
                TimeInForce::GoodUntilCancelled { post_only } => {
                    if post_only {
                        Some("post_only".to_string())
                    } else {
                        Some("gtc".to_string())
                    }
                }
                TimeInForce::ImmediateOrCancel => Some("ioc".to_string()),
                TimeInForce::FillOrKill => Some("fok".to_string()),
                TimeInForce::GoodUntilEndOfDay => Some("gtc".to_string()),
            },
        };

        let body = serde_json::to_string(&order_req)
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        let response = self
            .signed_request(Method::POST, endpoint, Some(body))
            .await?;

        let result: BitgetResponse<BitgetOrderResponse> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "00000" {
            return Err(UnindexedClientError::Other(format!(
                "Bitget order placement error: {} - {}",
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

        let endpoint = match self.config.trading_mode {
            TradingMode::Spot => "/api/v2/spot/trade/cancel-order",
            TradingMode::Futures => "/api/v2/mix/order/cancel-order",
        };

        let body = serde_json::json!({
            "symbol": request.key.instrument.as_ref(),
            "orderId": request.state.id.as_ref().map(|id| id.as_ref()),
            "clientOid": request.key.cid.as_ref()
        });

        let response = self
            .signed_request(Method::POST, endpoint, Some(body.to_string()))
            .await?;

        let result: BitgetResponse<serde_json::Value> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "00000" {
            return Err(UnindexedClientError::Other(format!(
                "Bitget order cancellation error: {} - {}",
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

        let endpoint = match self.config.trading_mode {
            TradingMode::Spot => "/api/v2/spot/trade/unfilled-orders",
            TradingMode::Futures => "/api/v2/mix/order/orders-pending",
        };

        let response = self.signed_request(Method::GET, endpoint, None).await?;
        let result: BitgetResponse<Vec<BitgetOrderInfo>> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "00000" {
            return Err(UnindexedClientError::Other(format!(
                "Bitget fetch orders error: {} - {}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in orders response".to_string())
        })?;

        data.into_iter()
            .filter_map(|order| self.parse_order_info(order))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Fetch trade history.
    pub async fn fetch_trades(
        &self,
        time_since: DateTime<Utc>,
    ) -> Result<Vec<Trade<QuoteAsset, InstrumentNameExchange>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = match self.config.trading_mode {
            TradingMode::Spot => "/api/v2/spot/trade/fills",
            TradingMode::Futures => "/api/v2/mix/order/fills",
        };

        let params = format!("?startTime={}", time_since.timestamp_millis());
        let url = format!("{}{}", endpoint, params);

        let response = self.signed_request(Method::GET, &url, None).await?;
        let result: BitgetResponse<Vec<BitgetTradeHistory>> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        if result.code != "00000" {
            return Err(UnindexedClientError::Other(format!(
                "Bitget fetch trades error: {} - {}",
                result.code, result.msg
            )));
        }

        let data = result.data.ok_or_else(|| {
            UnindexedClientError::Other("No data in trades response".to_string())
        })?;

        data.into_iter()
            .filter_map(|trade| self.parse_trade_history(trade))
            .collect::<Result<Vec<_>, _>>()
    }

    // Copy trading methods

    /// List available master traders.
    pub async fn list_master_traders(
        &self,
    ) -> Result<Vec<super::MasterTrader>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v2/copy/spot-trader/config";
        let response = self.signed_request(Method::GET, endpoint, None).await?;
        
        // Parse response and convert to MasterTrader structs
        // Implementation depends on actual API response format
        todo!("Implement master trader listing")
    }

    /// Follow a master trader.
    pub async fn follow_trader(&self, trader_id: &str) -> Result<(), UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v2/copy/spot-trader/follow";
        let body = serde_json::json!({
            "traderId": trader_id
        });

        let response = self
            .signed_request(Method::POST, endpoint, Some(body.to_string()))
            .await?;

        // Check response
        todo!("Implement trader following")
    }

    /// Unfollow a master trader.
    pub async fn unfollow_trader(&self, trader_id: &str) -> Result<(), UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v2/copy/spot-trader/unfollow";
        let body = serde_json::json!({
            "traderId": trader_id
        });

        let response = self
            .signed_request(Method::POST, endpoint, Some(body.to_string()))
            .await?;

        // Check response
        todo!("Implement trader unfollowing")
    }

    /// Get copy trading settings.
    pub async fn get_copy_settings(&self) -> Result<super::CopySettings, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v2/copy/spot-trader/settings";
        let response = self.signed_request(Method::GET, endpoint, None).await?;

        // Parse response
        todo!("Implement copy settings retrieval")
    }

    /// Update copy trading settings.
    pub async fn update_copy_settings(
        &self,
        settings: &super::CopySettings,
    ) -> Result<(), UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/api/v2/copy/spot-trader/settings";
        let body = serde_json::json!({
            "maxPositionSize": settings.max_position_size,
            "maxDailyLoss": settings.max_daily_loss,
            "copyRatio": settings.copy_ratio,
            "stopLossPercentage": settings.stop_loss_percentage,
            "takeProfitPercentage": settings.take_profit_percentage,
        });

        let response = self
            .signed_request(Method::POST, endpoint, Some(body.to_string()))
            .await?;

        // Check response
        todo!("Implement copy settings update")
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

        let signature = self.generate_signature(&timestamp, &method, endpoint, body.as_deref());

        let mut request = self.client.request(method, &url)
            .header("ACCESS-KEY", &self.config.api_key)
            .header("ACCESS-SIGN", signature)
            .header("ACCESS-TIMESTAMP", timestamp)
            .header("ACCESS-PASSPHRASE", &self.config.passphrase)
            .header("Content-Type", "application/json");

        if let Some(body_str) = body {
            request = request.body(body_str);
        }

        request
            .send()
            .await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))
    }

    /// Generate HMAC signature for request.
    fn generate_signature(
        &self,
        timestamp: &str,
        method: &Method,
        endpoint: &str,
        body: Option<&str>,
    ) -> String {
        let body_str = body.unwrap_or("");
        let sign_string = format!("{}{}{}{}", timestamp, method.as_str(), endpoint, body_str);

        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(sign_string.as_bytes());

        base64::encode(mac.finalize().into_bytes())
    }

    /// Parse spot balances.
    fn parse_spot_balances(
        &self,
        data: Vec<serde_json::Value>,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        data.into_iter()
            .filter_map(|v| {
                let balance: BitgetSpotBalance = serde_json::from_value(v).ok()?;
                let available = Decimal::from_str(&balance.available).ok()?;
                let frozen = Decimal::from_str(&balance.frozen).ok()?;
                let locked = Decimal::from_str(&balance.lock).ok()?;

                Some(AssetBalance {
                    asset: AssetNameExchange::new(balance.coin_name),
                    balance: Balance {
                        total: available + frozen + locked,
                        free: available,
                    },
                    time_exchange: Utc::now(),
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(Ok)
            .collect()
    }

    /// Parse futures balances.
    fn parse_futures_balances(
        &self,
        data: Vec<serde_json::Value>,
    ) -> Result<Vec<AssetBalance<AssetNameExchange>>, UnindexedClientError> {
        data.into_iter()
            .filter_map(|v| {
                let balance: BitgetFuturesBalance = serde_json::from_value(v).ok()?;
                let available = Decimal::from_str(&balance.available).ok()?;
                let locked = Decimal::from_str(&balance.locked).ok()?;

                Some(AssetBalance {
                    asset: AssetNameExchange::new(balance.margin_coin),
                    balance: Balance {
                        total: available + locked,
                        free: available,
                    },
                    time_exchange: Utc::now(),
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(Ok)
            .collect()
    }

    /// Parse order info into Order struct.
    fn parse_order_info(
        &self,
        info: BitgetOrderInfo,
    ) -> Option<Result<Order<ExchangeId, InstrumentNameExchange, Open>, UnindexedClientError>> {
        let side = match info.side.as_str() {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => return None,
        };

        let kind = match info.order_type.as_str() {
            "limit" => OrderKind::Limit,
            "market" => OrderKind::Market,
            _ => return None,
        };

        let price = Decimal::from_str(&info.price).ok()?;
        let quantity = Decimal::from_str(&info.size).ok()?;
        let filled_quantity = Decimal::from_str(&info.filled_qty).ok()?;
        let time = info.create_time.parse::<i64>().ok()
            .and_then(|ts| Utc.timestamp_millis_opt(ts).single())?;

        Some(Ok(Order {
            key: OrderKey {
                exchange: ExchangeId::Bitget,
                instrument: InstrumentNameExchange::new(info.symbol),
                strategy: Default::default(),
                cid: ClientOrderId::new(info.client_oid),
            },
            side,
            price,
            quantity,
            kind,
            time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
            state: Open {
                id: OrderId::new(info.order_id),
                time_exchange: time,
                filled_quantity,
            },
        }))
    }

    /// Parse trade history into Trade struct.
    fn parse_trade_history(
        &self,
        trade: BitgetTradeHistory,
    ) -> Option<Result<Trade<QuoteAsset, InstrumentNameExchange>, UnindexedClientError>> {
        // Implementation depends on Trade struct definition
        todo!("Implement trade history parsing")
    }
}