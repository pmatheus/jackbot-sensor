//! OKX staking implementation
//!
//! Supports OKX staking products including:
//! - Savings products
//! - DeFi earn products
//! - Structured products

use crate::staking::{
    error::{StakingError, StakingResult},
    manager::StakingManager,
    *,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use jackbot_instrument::{asset::name::AssetNameExchange, exchange::ExchangeId};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;

/// OKX staking manager configuration
#[derive(Debug, Clone)]
pub struct OKXStakingConfig {
    pub api_key: String,
    pub secret_key: String,
    pub passphrase: String,
    pub base_url: String,
    pub sandbox: bool,
}

impl Default for OKXStakingConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            secret_key: String::new(),
            passphrase: String::new(),
            base_url: "https://www.okx.com".to_string(),
            sandbox: false,
        }
    }
}

/// OKX staking manager
#[derive(Debug, Clone)]
pub struct OKXStakingManager {
    config: OKXStakingConfig,
    client: Client,
}

impl OKXStakingManager {
    /// Create a new OKX staking manager
    pub fn new(config: OKXStakingConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Sign a request with OKX API signature
    fn sign_request(
        &self,
        timestamp: &str,
        method: &str,
        request_path: &str,
        body: &str,
    ) -> String {
        use base64::{engine::general_purpose, Engine};
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let prehash = format!("{}{}{}{}", timestamp, method, request_path, body);

        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(self.config.secret_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(prehash.as_bytes());

        general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    /// Make authenticated API request
    async fn api_request<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        params: Option<HashMap<String, String>>,
    ) -> StakingResult<T> {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let request_path = format!("/api/v5/{}", endpoint);

        let query_string = params
            .map(|p| {
                p.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("&")
            })
            .unwrap_or_default();

        let full_path = if query_string.is_empty() {
            request_path.clone()
        } else {
            format!("{}?{}", request_path, query_string)
        };

        let signature = self.sign_request(&timestamp, "GET", &request_path, "");
        let url = format!("{}{}", self.config.base_url, full_path);

        let response = self
            .client
            .get(&url)
            .header("OK-ACCESS-KEY", &self.config.api_key)
            .header("OK-ACCESS-SIGN", signature)
            .header("OK-ACCESS-TIMESTAMP", timestamp)
            .header("OK-ACCESS-PASSPHRASE", &self.config.passphrase)
            .send()
            .await?;

        if !response.status().is_success() {
            let status_code = response.status().to_string();
            let error_text = response.text().await.unwrap_or_default();
            return Err(StakingError::ExchangeError {
                exchange: ExchangeId::Okx,
                code: status_code,
                message: error_text,
            });
        }

        let result: OKXApiResponse<T> =
            response
                .json()
                .await
                .map_err(|e| StakingError::SerializationError {
                    message: e.to_string(),
                })?;

        if result.code != "0" {
            return Err(StakingError::ExchangeError {
                exchange: ExchangeId::Okx,
                code: result.code,
                message: result.msg,
            });
        }

        if result.data.is_empty() {
            return Err(StakingError::InternalError {
                message: "No data returned from OKX API".to_string(),
            });
        }

        let first_item = result.data.into_iter().next().unwrap();
        Ok(first_item)
    }

    /// Convert OKX product to StakingProduct
    fn convert_product(&self, product: OKXStakingProduct) -> StakingProduct {
        let product_type = if product.term.parse::<i64>().unwrap_or(0) > 0 {
            StakingType::Locked(Duration::days(product.term.parse().unwrap_or(0)))
        } else {
            StakingType::Flexible
        };

        StakingProduct {
            id: product.product_id,
            asset: product.ccy,
            exchange: ExchangeId::Okx,
            product_type,
            apy: product.rate,
            minimum_amount: product.min_investment,
            maximum_amount: product.max_investment,
            lock_period: if product.term.parse::<i64>().unwrap_or(0) > 0 {
                Some(Duration::days(product.term.parse().unwrap_or(0)))
            } else {
                None
            },
            auto_compound: product.auto_renew,
            available_quota: product.remaining_quota,
            status: match product.state.as_str() {
                "8" => StakingProductStatus::Available,
                "9" => StakingProductStatus::SoldOut,
                _ => StakingProductStatus::Unavailable,
            },
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("product_type".to_string(), product.product_type);
                meta.insert("protocol".to_string(), product.protocol.unwrap_or_default());
                meta
            },
        }
    }

    /// Convert OKX position to StakingPosition
    fn convert_position(&self, position: OKXStakingPosition) -> StakingPosition {
        let status = match position.state.as_str() {
            "8" => StakingPositionStatus::Active,
            "13" => StakingPositionStatus::Unstaking,
            "9" => StakingPositionStatus::Completed,
            _ => StakingPositionStatus::Active,
        };

        StakingPosition {
            id: position.ord_id,
            asset: position.ccy.clone(),
            exchange: ExchangeId::Okx,
            amount: position.inv_data,
            product: StakingProduct {
                id: position.product_id,
                asset: position.ccy.clone(),
                exchange: ExchangeId::Okx,
                product_type: if position.term.parse::<i64>().unwrap_or(0) > 0 {
                    StakingType::Locked(Duration::days(position.term.parse().unwrap_or(0)))
                } else {
                    StakingType::Flexible
                },
                apy: position.rate,
                minimum_amount: Decimal::ZERO,
                maximum_amount: None,
                lock_period: if position.term.parse::<i64>().unwrap_or(0) > 0 {
                    Some(Duration::days(position.term.parse().unwrap_or(0)))
                } else {
                    None
                },
                auto_compound: position.auto_renew,
                available_quota: None,
                status: StakingProductStatus::Available,
                metadata: HashMap::new(),
            },
            start_time: DateTime::from_timestamp_millis(
                position.purchase_time.parse().unwrap_or(0),
            )
            .unwrap_or_else(Utc::now),
            end_time: if position.redempt_date.is_empty() {
                None
            } else {
                DateTime::from_timestamp_millis(position.redempt_date.parse().unwrap_or(0))
            },
            accumulated_rewards: position.earnings,
            status,
            last_updated: Utc::now(),
        }
    }
}

#[async_trait]
impl StakingManager for OKXStakingManager {
    fn exchange_id(&self) -> ExchangeId {
        ExchangeId::Okx
    }

    async fn stake_asset(
        &self,
        asset: &AssetNameExchange,
        amount: Decimal,
        product_id: Option<String>,
        _constraints: Option<StakingConstraints>,
    ) -> StakingResult<StakingOperation> {
        let product_id = product_id.ok_or_else(|| StakingError::InvalidParameters {
            message: "Product ID is required for OKX staking".to_string(),
        })?;

        let mut params = HashMap::new();
        params.insert("productId".to_string(), product_id.clone());
        params.insert("invData".to_string(), amount.to_string());
        params.insert("ccy".to_string(), asset.to_string());

        let response: OKXStakeResponse = self
            .api_request("finance/staking-defi/purchase", Some(params))
            .await?;

        Ok(StakingOperation {
            id: response.ord_id,
            operation_type: StakingOperationType::Stake,
            exchange: ExchangeId::Okx,
            asset: asset.to_string(),
            amount,
            timestamp: Utc::now(),
            status: StakingOperationStatus::Success,
            error: None,
        })
    }

    async fn unstake_asset(
        &self,
        position_id: &str,
        amount: Option<Decimal>,
    ) -> StakingResult<StakingOperation> {
        let mut params = HashMap::new();
        params.insert("ordId".to_string(), position_id.to_string());

        if let Some(_amt) = amount {
            params.insert("protocolType".to_string(), "staking".to_string());
            params.insert("allowEarlyRedemption".to_string(), "true".to_string());
        }

        let response: OKXUnstakeResponse = self
            .api_request("finance/staking-defi/redeem", Some(params))
            .await?;

        Ok(StakingOperation {
            id: response.ord_id,
            operation_type: StakingOperationType::Unstake,
            exchange: ExchangeId::Okx,
            asset: "".to_string(),
            amount: amount.unwrap_or(Decimal::ZERO),
            timestamp: Utc::now(),
            status: StakingOperationStatus::Success,
            error: None,
        })
    }

    async fn get_staking_products(
        &self,
        asset: &AssetNameExchange,
    ) -> StakingResult<Vec<StakingProduct>> {
        let mut params = HashMap::new();
        params.insert("ccy".to_string(), asset.to_string());
        params.insert("productType".to_string(), "staking".to_string());

        let response: Vec<OKXStakingProduct> = self
            .api_request("finance/staking-defi/offers", Some(params))
            .await?;

        let products = response
            .into_iter()
            .map(|p| self.convert_product(p))
            .collect();

        Ok(products)
    }

    async fn get_staking_positions(&self) -> StakingResult<Vec<StakingPosition>> {
        let params = Some(HashMap::new());

        let response: Vec<OKXStakingPosition> = self
            .api_request("finance/staking-defi/orders-active", params)
            .await?;

        let positions = response
            .into_iter()
            .map(|p| self.convert_position(p))
            .collect();

        Ok(positions)
    }

    async fn get_staking_positions_for_asset(
        &self,
        asset: &AssetNameExchange,
    ) -> StakingResult<Vec<StakingPosition>> {
        let mut params = HashMap::new();
        params.insert("ccy".to_string(), asset.to_string());

        let response: Vec<OKXStakingPosition> = self
            .api_request("finance/staking-defi/orders-active", Some(params))
            .await?;

        let positions = response
            .into_iter()
            .map(|p| self.convert_position(p))
            .collect();

        Ok(positions)
    }

    async fn get_staking_rewards(
        &self,
        asset: Option<&AssetNameExchange>,
    ) -> StakingResult<Vec<StakingReward>> {
        let mut params = HashMap::new();
        if let Some(asset) = asset {
            params.insert("ccy".to_string(), asset.to_string());
        }

        // Get reward history from position history
        let response: Vec<OKXStakingPosition> = self
            .api_request("finance/staking-defi/orders-history", Some(params))
            .await?;

        let rewards = response
            .into_iter()
            .filter(|p| p.earnings > Decimal::ZERO)
            .map(|p| StakingReward {
                id: format!("{}_{}", p.ord_id, p.purchase_time),
                asset: p.ccy.clone(),
                exchange: ExchangeId::Okx,
                position_id: p.ord_id,
                amount: p.earnings,
                earned_time: DateTime::from_timestamp_millis(p.purchase_time.parse().unwrap_or(0))
                    .unwrap_or_else(Utc::now),
                claimed_time: if p.redempt_date.is_empty() {
                    None
                } else {
                    DateTime::from_timestamp_millis(p.redempt_date.parse().unwrap_or(0))
                },
                status: if p.redempt_date.is_empty() {
                    StakingRewardStatus::Pending
                } else {
                    StakingRewardStatus::Claimed
                },
            })
            .collect();

        Ok(rewards)
    }

    async fn claim_staking_rewards(
        &self,
        _asset: &AssetNameExchange,
        _reward_ids: Option<Vec<String>>,
    ) -> StakingResult<StakingOperation> {
        Err(StakingError::InvalidParameters {
            message: "OKX automatically distributes rewards - manual claiming not supported"
                .to_string(),
        })
    }

    async fn get_operation_status(&self, operation_id: &str) -> StakingResult<StakingOperation> {
        let mut params = HashMap::new();
        params.insert("ordId".to_string(), operation_id.to_string());

        let response: OKXStakingPosition = self
            .api_request("finance/staking-defi/orders-active", Some(params))
            .await?;

        let status = match response.state.as_str() {
            "8" => StakingOperationStatus::Success,
            "1" => StakingOperationStatus::InProgress,
            "9" => StakingOperationStatus::Success,
            _ => StakingOperationStatus::Failed,
        };

        Ok(StakingOperation {
            id: operation_id.to_string(),
            operation_type: StakingOperationType::Stake,
            exchange: ExchangeId::Okx,
            asset: response.ccy,
            amount: response.inv_data,
            timestamp: DateTime::from_timestamp_millis(response.purchase_time.parse().unwrap_or(0))
                .unwrap_or_else(Utc::now),
            status,
            error: None,
        })
    }

    async fn cancel_operation(&self, _operation_id: &str) -> StakingResult<bool> {
        Err(StakingError::InvalidParameters {
            message: "Operation cancellation not supported by OKX".to_string(),
        })
    }

    async fn get_available_balance(&self, asset: &AssetNameExchange) -> StakingResult<Decimal> {
        let mut params = HashMap::new();
        params.insert("ccy".to_string(), asset.to_string());

        let response: Vec<OKXBalance> = self.api_request("account/balance", Some(params)).await?;

        let balance = response
            .into_iter()
            .flat_map(|acc| acc.details)
            .find(|detail| detail.ccy == asset.to_string())
            .map(|detail| detail.available_bal)
            .unwrap_or(Decimal::ZERO);

        Ok(balance)
    }

    async fn set_auto_compound(&self, position_id: &str, enabled: bool) -> StakingResult<bool> {
        let mut params = HashMap::new();
        params.insert("ordId".to_string(), position_id.to_string());
        params.insert("autoRenew".to_string(), enabled.to_string());

        let _response: OKXAutoRenewResponse = self
            .api_request("finance/staking-defi/auto-renew", Some(params))
            .await?;

        Ok(true)
    }

    async fn get_reward_history(
        &self,
        asset: Option<&AssetNameExchange>,
        _start_time: Option<DateTime<Utc>>,
        _end_time: Option<DateTime<Utc>>,
    ) -> StakingResult<Vec<StakingReward>> {
        self.get_staking_rewards(asset).await
    }

    async fn get_estimated_apy(
        &self,
        product_id: &str,
        _amount: Decimal,
    ) -> StakingResult<Decimal> {
        let mut params = HashMap::new();
        params.insert("productId".to_string(), product_id.to_string());

        let response: OKXStakingProduct = self
            .api_request("finance/staking-defi/offers", Some(params))
            .await?;

        Ok(response.rate)
    }
}

// OKX API response structures
#[derive(Debug, Deserialize)]
struct OKXApiResponse<T> {
    code: String,
    msg: String,
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct OKXStakingProduct {
    #[serde(rename = "productId")]
    product_id: String,
    ccy: String,
    rate: Decimal,
    #[serde(rename = "minInvestment")]
    min_investment: Decimal,
    #[serde(rename = "maxInvestment")]
    max_investment: Option<Decimal>,
    #[serde(rename = "remainingQuota")]
    remaining_quota: Option<Decimal>,
    term: String,
    state: String,
    #[serde(rename = "productType")]
    product_type: String,
    #[serde(rename = "autoRenew")]
    auto_renew: bool,
    protocol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OKXStakingPosition {
    #[serde(rename = "ordId")]
    ord_id: String,
    #[serde(rename = "productId")]
    product_id: String,
    ccy: String,
    #[serde(rename = "invData")]
    inv_data: Decimal,
    #[serde(rename = "purchaseTime")]
    purchase_time: String,
    #[serde(rename = "redemptDate")]
    redempt_date: String,
    term: String,
    state: String,
    rate: Decimal,
    earnings: Decimal,
    #[serde(rename = "autoRenew")]
    auto_renew: bool,
}

#[derive(Debug, Deserialize)]
struct OKXStakeResponse {
    #[serde(rename = "ordId")]
    ord_id: String,
}

#[derive(Debug, Deserialize)]
struct OKXUnstakeResponse {
    #[serde(rename = "ordId")]
    ord_id: String,
}

#[derive(Debug, Deserialize)]
struct OKXBalanceDetail {
    ccy: String,
    #[serde(rename = "availBal")]
    available_bal: Decimal,
}

#[derive(Debug, Deserialize)]
struct OKXBalance {
    details: Vec<OKXBalanceDetail>,
}

#[derive(Debug, Deserialize)]
struct OKXAutoRenewResponse {
    #[serde(rename = "ordId")]
    ord_id: String,
}
