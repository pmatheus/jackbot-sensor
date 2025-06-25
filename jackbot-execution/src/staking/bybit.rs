//! Bybit staking implementation
//!
//! Supports Bybit staking products including:
//! - Savings products (flexible)
//! - Fixed deposits (locked)
//! - Liquid staking products

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

/// Bybit staking manager configuration
#[derive(Debug, Clone)]
pub struct BybitStakingConfig {
    pub api_key: String,
    pub secret_key: String,
    pub base_url: String,
    pub testnet: bool,
}

impl Default for BybitStakingConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            secret_key: String::new(),
            base_url: "https://api.bybit.com".to_string(),
            testnet: false,
        }
    }
}

/// Bybit staking manager
#[derive(Debug, Clone)]
pub struct BybitStakingManager {
    config: BybitStakingConfig,
    client: Client,
}

impl BybitStakingManager {
    /// Create a new Bybit staking manager
    pub fn new(config: BybitStakingConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Sign a request with Bybit API signature
    fn sign_request(&self, params: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(self.config.secret_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(params.as_bytes());

        hex::encode(mac.finalize().into_bytes())
    }

    /// Make authenticated API request
    async fn api_request<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        params: HashMap<String, String>,
    ) -> StakingResult<T> {
        let timestamp = Utc::now().timestamp_millis();
        let mut params = params;
        params.insert("timestamp".to_string(), timestamp.to_string());

        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        let signature = self.sign_request(&query_string);
        let url = format!("{}/v5/{}", self.config.base_url, endpoint);
        let signed_query = format!("{}&sign={}", query_string, signature);

        let response = self
            .client
            .get(&format!("{}?{}", url, signed_query))
            .header("X-BAPI-API-KEY", &self.config.api_key)
            .header("X-BAPI-TIMESTAMP", timestamp.to_string())
            .send()
            .await?;

        if !response.status().is_success() {
            let status_code = response.status().to_string();
            let error_text = response.text().await.unwrap_or_default();
            return Err(StakingError::ExchangeError {
                exchange: ExchangeId::BybitSpot,
                code: status_code,
                message: error_text,
            });
        }

        let result: BybitApiResponse<T> =
            response
                .json()
                .await
                .map_err(|e| StakingError::SerializationError {
                    message: e.to_string(),
                })?;

        if result.ret_code != 0 {
            return Err(StakingError::ExchangeError {
                exchange: ExchangeId::BybitSpot,
                code: result.ret_code.to_string(),
                message: result.ret_msg,
            });
        }

        Ok(result.result)
    }

    /// Convert Bybit product to StakingProduct
    fn convert_product(&self, product: BybitStakingProduct) -> StakingProduct {
        let product_type = if product.term > 0 {
            StakingType::Locked(Duration::days(product.term))
        } else {
            StakingType::Flexible
        };

        StakingProduct {
            id: product.product_id,
            asset: product.currency,
            exchange: ExchangeId::BybitSpot,
            product_type,
            apy: product.apy,
            minimum_amount: product.min_purchase_amount,
            maximum_amount: product.max_purchase_amount,
            lock_period: if product.term > 0 {
                Some(Duration::days(product.term))
            } else {
                None
            },
            auto_compound: product.auto_compound.unwrap_or(false),
            available_quota: product.available_quota,
            status: match product.status.as_str() {
                "1" => StakingProductStatus::Available,
                "2" => StakingProductStatus::SoldOut,
                _ => StakingProductStatus::Unavailable,
            },
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("risk_level".to_string(), product.risk_level.to_string());
                meta.insert("product_type".to_string(), product.product_type);
                meta
            },
        }
    }

    /// Convert Bybit position to StakingPosition
    fn convert_position(&self, position: BybitStakingPosition) -> StakingPosition {
        let status = match position.status.as_str() {
            "1" => StakingPositionStatus::Active,
            "2" => StakingPositionStatus::Unstaking,
            "3" => StakingPositionStatus::Completed,
            _ => StakingPositionStatus::Active,
        };

        StakingPosition {
            id: position.order_id,
            asset: position.currency.clone(),
            exchange: ExchangeId::BybitSpot,
            amount: position.amount,
            product: StakingProduct {
                id: position.product_id,
                asset: position.currency,
                exchange: ExchangeId::BybitSpot,
                product_type: if position.term > 0 {
                    StakingType::Locked(Duration::days(position.term))
                } else {
                    StakingType::Flexible
                },
                apy: position.apy,
                minimum_amount: Decimal::ZERO,
                maximum_amount: None,
                lock_period: if position.term > 0 {
                    Some(Duration::days(position.term))
                } else {
                    None
                },
                auto_compound: false,
                available_quota: None,
                status: StakingProductStatus::Available,
                metadata: HashMap::new(),
            },
            start_time: position.purchase_time,
            end_time: position.redeem_time,
            accumulated_rewards: position.yield_amount,
            status,
            last_updated: Utc::now(),
        }
    }
}

#[async_trait]
impl StakingManager for BybitStakingManager {
    fn exchange_id(&self) -> ExchangeId {
        ExchangeId::BybitSpot
    }

    async fn stake_asset(
        &self,
        asset: &AssetNameExchange,
        amount: Decimal,
        product_id: Option<String>,
        _constraints: Option<StakingConstraints>,
    ) -> StakingResult<StakingOperation> {
        let product_id = product_id.ok_or_else(|| StakingError::InvalidParameters {
            message: "Product ID is required for Bybit staking".to_string(),
        })?;

        let mut params = HashMap::new();
        params.insert("productId".to_string(), product_id.clone());
        params.insert("amount".to_string(), amount.to_string());

        let response: BybitStakeResponse = self.api_request("asset/earn/purchase", params).await?;

        Ok(StakingOperation {
            id: response.order_id,
            operation_type: StakingOperationType::Stake,
            exchange: ExchangeId::BybitSpot,
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
        params.insert("orderId".to_string(), position_id.to_string());

        if let Some(amt) = amount {
            params.insert("amount".to_string(), amt.to_string());
        }

        let response: BybitUnstakeResponse = self.api_request("asset/earn/redeem", params).await?;

        Ok(StakingOperation {
            id: response.redeem_id,
            operation_type: StakingOperationType::Unstake,
            exchange: ExchangeId::BybitSpot,
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
        params.insert("currency".to_string(), asset.to_string());

        let response: BybitProductsResponse =
            self.api_request("asset/earn/product/list", params).await?;

        let products = response
            .rows
            .into_iter()
            .map(|p| self.convert_product(p))
            .collect();

        Ok(products)
    }

    async fn get_staking_positions(&self) -> StakingResult<Vec<StakingPosition>> {
        let params = HashMap::new();

        let response: BybitPositionsResponse =
            self.api_request("asset/earn/order/list", params).await?;

        let positions = response
            .rows
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
        params.insert("currency".to_string(), asset.to_string());

        let response: BybitPositionsResponse =
            self.api_request("asset/earn/order/list", params).await?;

        let positions = response
            .rows
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
            params.insert("currency".to_string(), asset.to_string());
        }

        let response: BybitRewardsResponse =
            self.api_request("asset/earn/yield/list", params).await?;

        let rewards = response
            .rows
            .into_iter()
            .map(|r| StakingReward {
                id: format!("{}_{}", r.currency, r.yield_time.timestamp()),
                asset: r.currency,
                exchange: ExchangeId::BybitSpot,
                position_id: r.order_id,
                amount: r.yield_amount,
                earned_time: r.yield_time,
                claimed_time: Some(r.yield_time), // Bybit auto-distributes
                status: StakingRewardStatus::Claimed,
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
            message: "Bybit automatically distributes rewards - manual claiming not supported"
                .to_string(),
        })
    }

    async fn get_operation_status(&self, operation_id: &str) -> StakingResult<StakingOperation> {
        let mut params = HashMap::new();
        params.insert("orderId".to_string(), operation_id.to_string());

        let response: BybitStakingPosition =
            self.api_request("asset/earn/order/detail", params).await?;

        let status = match response.status.as_str() {
            "1" => StakingOperationStatus::Success,
            "2" => StakingOperationStatus::InProgress,
            "3" => StakingOperationStatus::Success,
            _ => StakingOperationStatus::Failed,
        };

        Ok(StakingOperation {
            id: operation_id.to_string(),
            operation_type: StakingOperationType::Stake,
            exchange: ExchangeId::BybitSpot,
            asset: response.currency,
            amount: response.amount,
            timestamp: response.purchase_time,
            status,
            error: None,
        })
    }

    async fn cancel_operation(&self, _operation_id: &str) -> StakingResult<bool> {
        Err(StakingError::InvalidParameters {
            message: "Operation cancellation not supported by Bybit".to_string(),
        })
    }

    async fn get_available_balance(&self, asset: &AssetNameExchange) -> StakingResult<Decimal> {
        let mut params = HashMap::new();
        params.insert("coin".to_string(), asset.to_string());

        let response: BybitBalanceResponse =
            self.api_request("account/wallet-balance", params).await?;

        let balance = response
            .list
            .into_iter()
            .flat_map(|wallet| wallet.coin)
            .find(|coin| coin.coin == asset.to_string())
            .map(|coin| coin.wallet_balance)
            .unwrap_or(Decimal::ZERO);

        Ok(balance)
    }

    async fn set_auto_compound(&self, _position_id: &str, _enabled: bool) -> StakingResult<bool> {
        Err(StakingError::AutoCompoundNotSupported {
            exchange: ExchangeId::BybitSpot,
            product_id: _position_id.to_string(),
        })
    }

    async fn get_reward_history(
        &self,
        asset: Option<&AssetNameExchange>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> StakingResult<Vec<StakingReward>> {
        let mut params = HashMap::new();

        if let Some(asset) = asset {
            params.insert("currency".to_string(), asset.to_string());
        }

        if let Some(start) = start_time {
            params.insert(
                "startTime".to_string(),
                start.timestamp_millis().to_string(),
            );
        }

        if let Some(end) = end_time {
            params.insert("endTime".to_string(), end.timestamp_millis().to_string());
        }

        self.get_staking_rewards(asset).await
    }

    async fn get_estimated_apy(
        &self,
        product_id: &str,
        _amount: Decimal,
    ) -> StakingResult<Decimal> {
        let mut params = HashMap::new();
        params.insert("productId".to_string(), product_id.to_string());

        let response: BybitStakingProduct = self
            .api_request("asset/earn/product/detail", params)
            .await?;

        Ok(response.apy)
    }
}

// Bybit API response structures
#[derive(Debug, Deserialize)]
struct BybitApiResponse<T> {
    #[serde(rename = "retCode")]
    ret_code: i32,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    result: T,
}

#[derive(Debug, Deserialize)]
struct BybitStakingProduct {
    #[serde(rename = "productId")]
    product_id: String,
    currency: String,
    apy: Decimal,
    #[serde(rename = "minPurchaseAmount")]
    min_purchase_amount: Decimal,
    #[serde(rename = "maxPurchaseAmount")]
    max_purchase_amount: Option<Decimal>,
    #[serde(rename = "availableQuota")]
    available_quota: Option<Decimal>,
    term: i64,
    status: String,
    #[serde(rename = "riskLevel")]
    risk_level: i32,
    #[serde(rename = "productType")]
    product_type: String,
    #[serde(rename = "autoCompound")]
    auto_compound: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct BybitProductsResponse {
    rows: Vec<BybitStakingProduct>,
}

#[derive(Debug, Deserialize)]
struct BybitStakingPosition {
    #[serde(rename = "orderId")]
    order_id: String,
    #[serde(rename = "productId")]
    product_id: String,
    currency: String,
    amount: Decimal,
    #[serde(rename = "purchaseTime")]
    purchase_time: DateTime<Utc>,
    #[serde(rename = "redeemTime")]
    redeem_time: Option<DateTime<Utc>>,
    term: i64,
    status: String,
    apy: Decimal,
    #[serde(rename = "yieldAmount")]
    yield_amount: Decimal,
}

#[derive(Debug, Deserialize)]
struct BybitPositionsResponse {
    rows: Vec<BybitStakingPosition>,
}

#[derive(Debug, Deserialize)]
struct BybitStakeResponse {
    #[serde(rename = "orderId")]
    order_id: String,
}

#[derive(Debug, Deserialize)]
struct BybitUnstakeResponse {
    #[serde(rename = "redeemId")]
    redeem_id: String,
}

#[derive(Debug, Deserialize)]
struct BybitRewardRecord {
    currency: String,
    #[serde(rename = "yieldAmount")]
    yield_amount: Decimal,
    #[serde(rename = "orderId")]
    order_id: String,
    #[serde(rename = "yieldTime")]
    yield_time: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct BybitRewardsResponse {
    rows: Vec<BybitRewardRecord>,
}

#[derive(Debug, Deserialize)]
struct BybitCoinBalance {
    coin: String,
    #[serde(rename = "walletBalance")]
    wallet_balance: Decimal,
}

#[derive(Debug, Deserialize)]
struct BybitWalletBalance {
    coin: Vec<BybitCoinBalance>,
}

#[derive(Debug, Deserialize)]
struct BybitBalanceResponse {
    list: Vec<BybitWalletBalance>,
}
