//! Hyperliquid REST API client implementation.

use super::types::*;
use crate::{
    error::UnindexedClientError,
    order::{
        id::{ClientOrderId, OrderId},
        request::OrderRequestOpen,
        state::Open,
        Order, OrderKey, OrderKind, TimeInForce,
    },
    trade::Trade,
};
use chrono::{DateTime, TimeZone, Utc};
use jackbot_data::exchange::hyperliquid::rate_limit::HyperliquidRateLimit;
use jackbot_instrument::{
    asset::{name::AssetNameExchange, QuoteAsset},
    exchange::ExchangeId,
    instrument::name::InstrumentNameExchange,
    Side,
};
use jackbot_integration::rate_limit::Priority;
use reqwest::{Client, Method};
use rust_decimal::Decimal;
use std::str::FromStr;
use tracing::{debug, error, warn};

/// Hyperliquid REST API client.
#[derive(Clone, Debug)]
pub struct HyperliquidRestClient {
    config: HyperliquidConfig,
    client: Client,
    rate_limiter: HyperliquidRateLimit,
}

impl HyperliquidRestClient {
    /// Create a new REST API client.
    pub fn new(config: HyperliquidConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            rate_limiter: HyperliquidRateLimit::new(),
        }
    }

    /// Fetch all positions.
    pub async fn fetch_all_positions(
        &self,
    ) -> Result<Vec<Position>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/info";
        let body = serde_json::json!({
            "type": "clearinghouseState",
            "user": self.config.account_address,
        });

        let response = self.post_request(endpoint, &body).await?;
        let result: HyperliquidAccountState = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        Ok(result
            .asset_positions
            .into_iter()
            .map(|ap| ap.position)
            .collect())
    }

    /// Fetch positions for specific instruments.
    pub async fn fetch_positions(
        &self,
        instruments: &[InstrumentNameExchange],
    ) -> Result<Vec<Position>, UnindexedClientError> {
        let all_positions = self.fetch_all_positions().await?;
        let instrument_names: Vec<String> = instruments
            .iter()
            .map(|i| i.as_ref().to_string())
            .collect();

        Ok(all_positions
            .into_iter()
            .filter(|p| instrument_names.contains(&p.coin))
            .collect())
    }

    /// Fetch open orders.
    pub async fn fetch_open_orders(
        &self,
    ) -> Result<Vec<Order<ExchangeId, InstrumentNameExchange, Open>>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/info";
        let body = serde_json::json!({
            "type": "openOrders",
            "user": self.config.account_address,
        });

        let response = self.post_request(endpoint, &body).await?;
        let orders: Vec<HyperliquidOrderInfo> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        orders
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

        let endpoint = "/info";
        let body = serde_json::json!({
            "type": "userFills",
            "user": self.config.account_address,
            "startTime": time_since.timestamp_millis(),
        });

        let response = self.post_request(endpoint, &body).await?;
        let fills: Vec<HyperliquidFill> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        fills
            .into_iter()
            .filter_map(|fill| self.parse_fill(fill))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Get recent liquidations.
    pub async fn get_liquidations(
        &self,
        instrument: Option<&InstrumentNameExchange>,
        limit: Option<usize>,
    ) -> Result<Vec<super::Liquidation>, UnindexedClientError> {
        self.rate_limiter.acquire_rest(Priority::Normal).await;

        let endpoint = "/info";
        let mut body = serde_json::json!({
            "type": "liquidations",
        });

        if let Some(inst) = instrument {
            body["coin"] = serde_json::Value::String(inst.as_ref().to_string());
        }

        if let Some(lim) = limit {
            body["limit"] = serde_json::Value::Number(lim.into());
        }

        let response = self.post_request(endpoint, &body).await?;
        let liquidations: Vec<HyperliquidLiquidation> = response.json().await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))?;

        liquidations
            .into_iter()
            .filter_map(|liq| self.parse_liquidation(liq))
            .collect::<Result<Vec<_>, _>>()
    }

    // Helper methods

    /// Make a POST request to the API.
    async fn post_request(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, UnindexedClientError> {
        let url = format!("{}{}", self.config.rest_url, endpoint);

        let mut request = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(body);

        // Add API key if configured
        if let Some(api_key) = &self.config.api_key {
            request = request.header("X-API-KEY", api_key);
        }

        request
            .send()
            .await
            .map_err(|e| UnindexedClientError::Other(e.to_string()))
    }

    /// Parse order info into Order struct.
    fn parse_order_info(
        &self,
        info: HyperliquidOrderInfo,
    ) -> Option<Result<Order<ExchangeId, InstrumentNameExchange, Open>, UnindexedClientError>> {
        let side = match info.side.as_str() {
            "B" => Side::Buy,
            "A" => Side::Sell,
            _ => return None,
        };

        let kind = match info.order_type.as_str() {
            "Limit" => OrderKind::Limit,
            "Market" => OrderKind::Market,
            _ => return None,
        };

        let price = Decimal::from_str(&info.limit_px).ok()?;
        let quantity = Decimal::from_str(&info.sz).ok()?;
        let filled_quantity = Decimal::from_str(&info.sz).ok()? 
            - Decimal::from_str(&info.orig_sz).ok()?;
        let time = Utc.timestamp_millis_opt(info.timestamp).single()?;

        let cid = info.cloid
            .and_then(|c| c.parse::<u64>().ok())
            .map(|id| ClientOrderId::new(id.to_string()))
            .unwrap_or_default();

        Some(Ok(Order {
            key: OrderKey {
                exchange: ExchangeId::Hyperliquid,
                instrument: InstrumentNameExchange::new(info.coin),
                strategy: Default::default(),
                cid,
            },
            side,
            price,
            quantity,
            kind,
            time_in_force: TimeInForce::GoodUntilCancelled { post_only: false },
            state: Open {
                id: OrderId::new(info.oid),
                time_exchange: time,
                filled_quantity,
            },
        }))
    }

    /// Parse fill into Trade struct.
    fn parse_fill(
        &self,
        fill: HyperliquidFill,
    ) -> Option<Result<Trade<QuoteAsset, InstrumentNameExchange>, UnindexedClientError>> {
        // Implementation depends on Trade struct definition
        // This is a placeholder
        todo!("Implement fill parsing")
    }

    /// Parse liquidation event.
    fn parse_liquidation(
        &self,
        liq: HyperliquidLiquidation,
    ) -> Option<Result<super::Liquidation, UnindexedClientError>> {
        let side = match liq.side.as_str() {
            "B" => Side::Buy,
            "A" => Side::Sell,
            _ => return None,
        };

        let liq_type = match liq.liquidation_type.as_str() {
            "partial" => super::LiquidationType::Partial,
            "full" => super::LiquidationType::Full,
            "adl" => super::LiquidationType::AutoDeleverage,
            _ => return None,
        };

        let price = Decimal::from_str(&liq.px).ok()?;
        let quantity = Decimal::from_str(&liq.sz).ok()?;
        let time = Utc.timestamp_millis_opt(liq.time).single()?;

        Some(Ok(super::Liquidation {
            liquidation_id: liq.liquidation_id,
            account: liq.account,
            instrument: InstrumentNameExchange::new(liq.coin),
            side,
            price,
            quantity,
            time,
            liquidation_type: liq_type,
        }))
    }
}