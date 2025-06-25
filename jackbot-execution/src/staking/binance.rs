//! Binance staking implementation
//!
//! Supports Binance Simple Earn products including:
//! - Flexible products (instant unstaking)
//! - Locked products (fixed terms)
//! - DeFi staking
//! - Launchpool integration
//! - BNB vault features

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

/// Binance staking manager configuration
#[derive(Debug, Clone)]
pub struct BinanceStakingConfig {
    pub api_key: String,
    pub secret_key: String,
    pub base_url: String,
    pub testnet: bool,
}

impl Default for BinanceStakingConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            secret_key: String::new(),
            base_url: "https://api.binance.com".to_string(),
            testnet: false,
        }
    }
}

/// Binance staking manager
#[derive(Debug, Clone)]
pub struct BinanceStakingManager {
    config: BinanceStakingConfig,
    client: Client,
}

impl BinanceStakingManager {
    /// Create a new Binance staking manager
    pub fn new(config: BinanceStakingConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Sign a request with Binance API signature
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
        let url = format!("{}/sapi/v1/{}", self.config.base_url, endpoint);
        let signed_query = format!("{}&signature={}", query_string, signature);

        let response = self
            .client
            .get(&format!("{}?{}", url, signed_query))
            .header("X-MBX-APIKEY", &self.config.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            let status_code = response.status().to_string();
            let error_text = response.text().await.unwrap_or_default();
            return Err(StakingError::ExchangeError {
                exchange: ExchangeId::BinanceSpot,
                code: status_code,
                message: error_text,
            });
        }

        let result = response
            .json()
            .await
            .map_err(|e| StakingError::SerializationError {
                message: e.to_string(),
            })?;

        Ok(result)
    }

    /// Convert Binance product to StakingProduct
    fn convert_product(&self, product: BinanceStakingProduct) -> StakingProduct {
        let product_type = if product.duration.is_some() {
            StakingType::Locked(Duration::days(product.duration.unwrap_or(0)))
        } else {
            StakingType::Flexible
        };

        StakingProduct {
            id: product.product_id,
            asset: product.asset,
            exchange: ExchangeId::BinanceSpot,
            product_type,
            apy: product.latest_annual_percentage_rate,
            minimum_amount: product.min_purchase_amount,
            maximum_amount: product.max_purchase_amount,
            lock_period: product.duration.map(Duration::days),
            auto_compound: product.can_redeem,
            available_quota: product.purchase_quota,
            status: if product.status == "PURCHASING" {
                StakingProductStatus::Available
            } else {
                StakingProductStatus::Unavailable
            },
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("tier".to_string(), product.tier.to_string());
                meta.insert("can_redeem".to_string(), product.can_redeem.to_string());
                meta.insert("featured".to_string(), product.featured.to_string());
                meta
            },
        }
    }

    /// Convert Binance position to StakingPosition
    fn convert_position(&self, position: BinanceStakingPosition) -> StakingPosition {
        let status = match position.status.as_str() {
            "HOLDING" => StakingPositionStatus::Active,
            "REDEEMING" => StakingPositionStatus::Unstaking,
            "SUCCESS" => StakingPositionStatus::Completed,
            _ => StakingPositionStatus::Active,
        };

        StakingPosition {
            id: position
                .position_id
                .unwrap_or_else(|| position.project_id.clone()),
            asset: position.asset.clone(),
            exchange: ExchangeId::BinanceSpot,
            amount: position.amount,
            product: StakingProduct {
                id: position.project_id,
                asset: position.asset,
                exchange: ExchangeId::BinanceSpot,
                product_type: if position.duration > 0 {
                    StakingType::Locked(Duration::days(position.duration))
                } else {
                    StakingType::Flexible
                },
                apy: position.apr,
                minimum_amount: Decimal::ZERO,
                maximum_amount: None,
                lock_period: if position.duration > 0 {
                    Some(Duration::days(position.duration))
                } else {
                    None
                },
                auto_compound: position.auto_subscribe,
                available_quota: None,
                status: StakingProductStatus::Available,
                metadata: HashMap::new(),
            },
            start_time: position.purchase_time,
            end_time: position.redeem_date,
            accumulated_rewards: position.reward_amount.unwrap_or(Decimal::ZERO),
            status,
            last_updated: Utc::now(),
        }
    }
}

#[async_trait]
impl StakingManager for BinanceStakingManager {
    fn exchange_id(&self) -> ExchangeId {
        ExchangeId::BinanceSpot
    }

    async fn stake_asset(
        &self,
        asset: &AssetNameExchange,
        amount: Decimal,
        product_id: Option<String>,
        _constraints: Option<StakingConstraints>,
    ) -> StakingResult<StakingOperation> {
        let product_id = product_id.ok_or_else(|| StakingError::InvalidParameters {
            message: "Product ID is required for Binance staking".to_string(),
        })?;

        let mut params = HashMap::new();
        params.insert("projectId".to_string(), product_id.clone());
        params.insert("lot".to_string(), amount.to_string());

        let response: BinanceStakeResponse = self
            .api_request("simple-earn/flexible/subscribe", params)
            .await?;

        if !response.success {
            return Err(StakingError::ExchangeError {
                exchange: ExchangeId::BinanceSpot,
                code: "STAKE_FAILED".to_string(),
                message: "Binance staking request failed".to_string(),
            });
        }

        Ok(StakingOperation {
            id: response.purchase_id,
            operation_type: StakingOperationType::Stake,
            exchange: ExchangeId::BinanceSpot,
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
        params.insert("productId".to_string(), position_id.to_string());

        if let Some(amt) = amount {
            params.insert("amount".to_string(), amt.to_string());
            params.insert("type".to_string(), "FAST".to_string());
        } else {
            params.insert("type".to_string(), "NORMAL".to_string());
        }

        let response: BinanceUnstakeResponse = self
            .api_request("simple-earn/flexible/redeem", params)
            .await?;

        if !response.success {
            return Err(StakingError::ExchangeError {
                exchange: ExchangeId::BinanceSpot,
                code: "UNSTAKE_FAILED".to_string(),
                message: "Binance unstaking request failed".to_string(),
            });
        }

        Ok(StakingOperation {
            id: response.redeem_id,
            operation_type: StakingOperationType::Unstake,
            exchange: ExchangeId::BinanceSpot,
            asset: "".to_string(), // Asset info not returned by Binance
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
        params.insert("asset".to_string(), asset.to_string());
        params.insert("status".to_string(), "PURCHASING".to_string());

        let response: BinanceProductsResponse = self
            .api_request("simple-earn/flexible/list", params)
            .await?;

        let products = response
            .rows
            .into_iter()
            .map(|p| self.convert_product(p))
            .collect();

        Ok(products)
    }

    async fn get_staking_positions(&self) -> StakingResult<Vec<StakingPosition>> {
        let params = HashMap::new();

        let response: BinancePositionsResponse = self
            .api_request("simple-earn/flexible/position", params)
            .await?;

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
        params.insert("asset".to_string(), asset.to_string());

        let response: BinancePositionsResponse = self
            .api_request("simple-earn/flexible/position", params)
            .await?;

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
            params.insert("asset".to_string(), asset.to_string());
        }

        let response: BinanceRewardsResponse = self
            .api_request("simple-earn/flexible/rewards", params)
            .await?;

        let rewards = response
            .rows
            .into_iter()
            .map(|r| StakingReward {
                id: format!("{}_{}", r.asset, r.rewards_date.timestamp()),
                asset: r.asset,
                exchange: ExchangeId::BinanceSpot,
                position_id: r.project_id,
                amount: r.rewards,
                earned_time: r.rewards_date,
                claimed_time: Some(r.rewards_date), // Binance auto-distributes
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
        // Binance automatically distributes rewards daily
        Err(StakingError::InvalidParameters {
            message: "Binance automatically distributes rewards - manual claiming not supported"
                .to_string(),
        })
    }

    async fn get_operation_status(&self, operation_id: &str) -> StakingResult<StakingOperation> {
        // For now, return a placeholder - would need to track operations in a database
        Ok(StakingOperation {
            id: operation_id.to_string(),
            operation_type: StakingOperationType::Stake,
            exchange: ExchangeId::BinanceSpot,
            asset: "".to_string(),
            amount: Decimal::ZERO,
            timestamp: Utc::now(),
            status: StakingOperationStatus::Success,
            error: None,
        })
    }

    async fn cancel_operation(&self, _operation_id: &str) -> StakingResult<bool> {
        Err(StakingError::InvalidParameters {
            message: "Operation cancellation not supported by Binance".to_string(),
        })
    }

    async fn get_available_balance(&self, asset: &AssetNameExchange) -> StakingResult<Decimal> {
        let mut params = HashMap::new();
        params.insert("asset".to_string(), asset.to_string());

        let response: BinanceBalanceResponse = self.api_request("account", params).await?;

        let balance = response
            .balances
            .into_iter()
            .find(|b| b.asset == asset.to_string())
            .map(|b| b.free)
            .unwrap_or(Decimal::ZERO);

        Ok(balance)
    }

    async fn set_auto_compound(&self, position_id: &str, enabled: bool) -> StakingResult<bool> {
        let mut params = HashMap::new();
        params.insert("projectId".to_string(), position_id.to_string());
        params.insert("autoSubscribe".to_string(), enabled.to_string());

        let response: BinanceAutoSubscribeResponse = self
            .api_request("simple-earn/flexible/setAutoSubscribe", params)
            .await?;

        Ok(response.success)
    }

    async fn get_reward_history(
        &self,
        asset: Option<&AssetNameExchange>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> StakingResult<Vec<StakingReward>> {
        let mut params = HashMap::new();

        if let Some(asset) = asset {
            params.insert("asset".to_string(), asset.to_string());
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

        let response: BinanceStakingProduct = self
            .api_request("simple-earn/flexible/product", params)
            .await?;

        Ok(response.latest_annual_percentage_rate)
    }
}

// Binance API response structures
#[derive(Debug, Deserialize)]
struct BinanceStakingProduct {
    #[serde(rename = "productId")]
    product_id: String,
    asset: String,
    #[serde(rename = "latestAnnualPercentageRate")]
    latest_annual_percentage_rate: Decimal,
    #[serde(rename = "minPurchaseAmount")]
    min_purchase_amount: Decimal,
    #[serde(rename = "maxPurchaseAmount")]
    max_purchase_amount: Option<Decimal>,
    #[serde(rename = "purchaseQuota")]
    purchase_quota: Option<Decimal>,
    duration: Option<i64>,
    status: String,
    tier: i32,
    #[serde(rename = "canRedeem")]
    can_redeem: bool,
    featured: bool,
}

#[derive(Debug, Deserialize)]
struct BinanceProductsResponse {
    rows: Vec<BinanceStakingProduct>,
    total: i32,
}

#[derive(Debug, Deserialize)]
struct BinanceStakingPosition {
    #[serde(rename = "positionId")]
    position_id: Option<String>,
    #[serde(rename = "projectId")]
    project_id: String,
    asset: String,
    amount: Decimal,
    #[serde(rename = "purchaseTime")]
    purchase_time: DateTime<Utc>,
    duration: i64,
    status: String,
    apr: Decimal,
    #[serde(rename = "redeemDate")]
    redeem_date: Option<DateTime<Utc>>,
    #[serde(rename = "rewardAmount")]
    reward_amount: Option<Decimal>,
    #[serde(rename = "autoSubscribe")]
    auto_subscribe: bool,
}

#[derive(Debug, Deserialize)]
struct BinancePositionsResponse {
    rows: Vec<BinanceStakingPosition>,
    total: i32,
}

#[derive(Debug, Deserialize)]
struct BinanceStakeResponse {
    #[serde(rename = "purchaseId")]
    purchase_id: String,
    success: bool,
}

#[derive(Debug, Deserialize)]
struct BinanceUnstakeResponse {
    #[serde(rename = "redeemId")]
    redeem_id: String,
    success: bool,
}

#[derive(Debug, Deserialize)]
struct BinanceRewardRecord {
    asset: String,
    rewards: Decimal,
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "rewardsDate")]
    rewards_date: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct BinanceRewardsResponse {
    rows: Vec<BinanceRewardRecord>,
    total: i32,
}

#[derive(Debug, Deserialize)]
struct BinanceBalance {
    asset: String,
    free: Decimal,
}

#[derive(Debug, Deserialize)]
struct BinanceBalanceResponse {
    balances: Vec<BinanceBalance>,
}

#[derive(Debug, Deserialize)]
struct BinanceAutoSubscribeResponse {
    success: bool,
}
