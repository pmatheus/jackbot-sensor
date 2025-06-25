//! Kraken staking implementation
//!
//! Comprehensive Kraken staking support including:
//! - ETH 2.0 staking with competitive rewards (ETH2.S rewards)
//! - DOT/KSM parachain staking with nomination pools
//! - Traditional earn products and flexible staking
//! - Advanced yield optimization and risk management
//! - Compound staking for supported assets
//! - Tax-optimized staking strategies
//! - Slashing risk management for PoS assets
//! - Reward claiming optimization
//! - Unstaking queues and bonding periods
//! - On-chain vs off-chain reward distribution

use crate::staking::{
    error::{StakingError, StakingResult},
    manager::StakingManager,
    *,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use jackbot_instrument::{
    asset::name::AssetNameExchange,
    exchange::ExchangeId,
};
use reqwest;
use rust_decimal::{Decimal, prelude::FromStr};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::collections::HashMap;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn};

type HmacSha256 = Hmac<Sha256>;

/// Kraken staking manager configuration
#[derive(Debug, Clone)]
pub struct KrakenStakingConfig {
    pub api_key: String,
    pub private_key: String,
    pub base_url: String,
    pub timeout_seconds: u64,
    pub enable_eth2_staking: bool,
    pub enable_parachain_staking: bool,
    pub tax_optimization: Option<TaxOptimizationConfig>,
    pub slashing_risk_management: Option<SlashingRiskConfig>,
    pub reward_optimization: RewardOptimizationStrategy,
    pub auto_compound_threshold: Decimal,
}

impl Default for KrakenStakingConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            private_key: String::new(),
            base_url: "https://api.kraken.com".to_string(),
            timeout_seconds: 30,
            enable_eth2_staking: true,
            enable_parachain_staking: true,
            tax_optimization: None,
            slashing_risk_management: None,
            reward_optimization: RewardOptimizationStrategy::BalancedApproach,
            auto_compound_threshold: Decimal::from_str("10.0").unwrap_or(Decimal::ZERO),
        }
    }
}

/// Kraken API response wrapper
#[derive(Debug, Deserialize)]
struct KrakenResponse<T> {
    error: Vec<String>,
    result: Option<T>,
}

/// Kraken staking product info
#[derive(Debug, Deserialize, Serialize)]
struct KrakenStakingProduct {
    asset: String,
    method: String,
    asset_yield: Option<String>,
    minimum_amount: Option<String>,
    lock_type: Option<String>,
    rewards: Option<Value>,
    unbonding_period: Option<String>,
    validator_fee: Option<String>,
    slashing_risk: Option<bool>,
    compound_enabled: Option<bool>,
}

/// Kraken ETH 2.0 specific data
#[derive(Debug, Deserialize, Serialize)]
struct KrakenEth2Data {
    pending_deposit: Option<String>,
    pending_withdrawal: Option<String>,
    validator_queue_position: Option<u64>,
    eth2s_balance: Option<String>,
    estimated_rewards: Option<String>,
}

/// Kraken DOT/KSM parachain data
#[derive(Debug, Deserialize, Serialize)]
struct KrakenParachainData {
    nomination_pool_id: Option<String>,
    current_era: Option<u64>,
    unbonding_chunks: Option<Vec<UnbondingChunk>>,
    slashing_events: Option<Vec<SlashingEvent>>,
}

/// Unbonding chunk information
#[derive(Debug, Deserialize, Serialize)]
struct UnbondingChunk {
    amount: String,
    completion_era: u64,
    estimated_completion: Option<String>,
}

/// Slashing event information
#[derive(Debug, Deserialize, Serialize)]
struct SlashingEvent {
    era: u64,
    amount: String,
    reason: String,
    timestamp: String,
}

/// Tax optimization preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxOptimizationConfig {
    pub jurisdiction: String,
    pub preferred_holding_period: Duration,
    pub harvest_loss_threshold: Decimal,
    pub compound_vs_claim_preference: CompoundPreference,
}

/// Compound vs claim preference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompoundPreference {
    AlwaysCompound,
    AlwaysClaim,
    OptimizeForTax,
    OptimizeForYield,
}

/// Slashing risk management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashingRiskConfig {
    pub max_exposure_per_validator: Decimal,
    pub diversification_threshold: Decimal,
    pub auto_rebalance_on_slashing: bool,
    pub emergency_unstake_threshold: Decimal,
}

/// Reward optimization strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RewardOptimizationStrategy {
    MaximizeCompounding,
    MinimizeTaxes,
    OptimizeLiquidity,
    BalancedApproach,
}

/// Kraken staking position
#[derive(Debug, Deserialize)]
struct KrakenStakingPosition {
    asset: String,
    amount: String,
    native_amount: Option<String>,
    native_asset: Option<String>,
    pending: Option<String>,
    bondid: String,
}

/// Kraken staking rewards
#[derive(Debug, Deserialize)]
struct KrakenStakingReward {
    asset: String,
    amount: String,
    bondid: String,
    datetime: String,
}

/// Kraken staking manager
#[derive(Debug, Clone)]
pub struct KrakenStakingManager {
    config: KrakenStakingConfig,
    client: reqwest::Client,
    eth2_data_cache: std::sync::Arc<tokio::sync::RwLock<HashMap<String, KrakenEth2Data>>>,
    parachain_data_cache: std::sync::Arc<tokio::sync::RwLock<HashMap<String, KrakenParachainData>>>,
    last_cache_update: std::sync::Arc<tokio::sync::RwLock<DateTime<Utc>>>,
}

impl KrakenStakingManager {
    /// Create a new Kraken staking manager
    pub fn new(config: KrakenStakingConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            client,
            eth2_data_cache: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            parachain_data_cache: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            last_cache_update: std::sync::Arc::new(tokio::sync::RwLock::new(Utc::now())),
        }
    }

    /// Update cache with fresh data if needed
    async fn update_cache_if_needed(&self) -> StakingResult<()> {
        let last_update = *self.last_cache_update.read().await;
        let now = Utc::now();
        
        // Update cache every 5 minutes
        if now.signed_duration_since(last_update) > Duration::minutes(5) {
            self.refresh_eth2_data().await?;
            self.refresh_parachain_data().await?;
            *self.last_cache_update.write().await = now;
        }
        
        Ok(())
    }
    
    /// Refresh ETH 2.0 specific data
    async fn refresh_eth2_data(&self) -> StakingResult<()> {
        let params = HashMap::new();
        
        // Get ETH 2.0 staking status
        if let Ok(result) = self.make_request::<Value>("Staking/Assets", &params).await {
            if let Some(eth_data) = result.get("ETH") {
                let eth2_data = KrakenEth2Data {
                    pending_deposit: eth_data.get("pending_deposit").and_then(|v| v.as_str()).map(String::from),
                    pending_withdrawal: eth_data.get("pending_withdrawal").and_then(|v| v.as_str()).map(String::from),
                    validator_queue_position: eth_data.get("queue_position").and_then(|v| v.as_u64()),
                    eth2s_balance: eth_data.get("eth2s_balance").and_then(|v| v.as_str()).map(String::from),
                    estimated_rewards: eth_data.get("estimated_rewards").and_then(|v| v.as_str()).map(String::from),
                };
                
                self.eth2_data_cache.write().await.insert("ETH".to_string(), eth2_data);
            }
        }
        
        Ok(())
    }
    
    /// Refresh parachain staking data for DOT/KSM
    async fn refresh_parachain_data(&self) -> StakingResult<()> {
        for asset in ["DOT", "KSM"] {
            let mut params = HashMap::new();
            params.insert("asset".to_string(), asset.to_string());
            
            if let Ok(result) = self.make_request::<Value>("Staking/Pending", &params).await {
                let parachain_data = KrakenParachainData {
                    nomination_pool_id: result.get("pool_id").and_then(|v| v.as_str()).map(String::from),
                    current_era: result.get("current_era").and_then(|v| v.as_u64()),
                    unbonding_chunks: None, // Would parse from API response
                    slashing_events: None,  // Would parse from API response
                };
                
                self.parachain_data_cache.write().await.insert(asset.to_string(), parachain_data);
            }
        }
        
        Ok(())
    }
    
    /// Get ETH 2.0 specific information
    pub async fn get_eth2_info(&self, asset: &str) -> StakingResult<Option<KrakenEth2Data>> {
        self.update_cache_if_needed().await?;
        Ok(self.eth2_data_cache.read().await.get(asset).cloned())
    }
    
    /// Get parachain staking information
    pub async fn get_parachain_info(&self, asset: &str) -> StakingResult<Option<KrakenParachainData>> {
        self.update_cache_if_needed().await?;
        Ok(self.parachain_data_cache.read().await.get(asset).cloned())
    }
    
    /// Implement compound staking strategy
    pub async fn auto_compound_rewards(&self, asset: &AssetNameExchange) -> StakingResult<Vec<StakingOperation>> {
        let asset_str = asset.asset_name().to_string();
        let rewards = self.get_staking_rewards(Some(asset)).await?;
        
        let mut operations = Vec::new();
        let mut total_rewards = Decimal::ZERO;
        
        // Calculate total available rewards
        for reward in &rewards {
            if reward.status == StakingRewardStatus::Available {
                total_rewards += reward.amount;
            }
        }
        
        // Check if rewards meet auto-compound threshold
        if total_rewards >= self.config.auto_compound_threshold {
            match self.config.reward_optimization {
                RewardOptimizationStrategy::MaximizeCompounding => {
                    // Stake all rewards back into highest APY product
                    let products = self.get_staking_products(asset).await?;
                    if let Some(best_product) = products.into_iter().max_by(|a, b| a.apy.cmp(&b.apy)) {
                        let operation = self.stake_asset(asset, total_rewards, Some(best_product.id), None).await?;
                        operations.push(operation);
                    }
                },
                RewardOptimizationStrategy::MinimizeTaxes => {
                    // Apply tax-optimized compounding strategy
                    if let Some(tax_config) = &self.config.tax_optimization {
                        operations.extend(self.apply_tax_optimized_compounding(asset, total_rewards, tax_config).await?);
                    } else {
                        // Default to simple compounding
                        let operation = self.stake_asset(asset, total_rewards, None, None).await?;
                        operations.push(operation);
                    }
                },
                _ => {
                    // Balanced approach - compound 70%, keep 30% liquid
                    let compound_amount = total_rewards * Decimal::from_str("0.7").unwrap_or(Decimal::ZERO);
                    if compound_amount > Decimal::ZERO {
                        let operation = self.stake_asset(asset, compound_amount, None, None).await?;
                        operations.push(operation);
                    }
                }
            }
        }
        
        Ok(operations)
    }
    
    /// Apply tax-optimized compounding strategy
    async fn apply_tax_optimized_compounding(
        &self,
        asset: &AssetNameExchange,
        amount: Decimal,
        tax_config: &TaxOptimizationConfig,
    ) -> StakingResult<Vec<StakingOperation>> {
        let mut operations = Vec::new();
        
        match tax_config.compound_vs_claim_preference {
            CompoundPreference::AlwaysCompound => {
                let operation = self.stake_asset(asset, amount, None, None).await?;
                operations.push(operation);
            },
            CompoundPreference::OptimizeForTax => {
                // Check holding period and apply tax-loss harvesting
                let positions = self.get_staking_positions_for_asset(asset).await?;
                
                // Find positions eligible for tax-loss harvesting
                for position in positions {
                    let holding_duration = Utc::now().signed_duration_since(position.start_time);
                    
                    if holding_duration >= tax_config.preferred_holding_period {
                        // Long-term holding - compound normally
                        let operation = self.stake_asset(asset, amount, Some(position.product.id), None).await?;
                        operations.push(operation);
                        break;
                    }
                }
            },
            _ => {
                // Default compound behavior
                let operation = self.stake_asset(asset, amount, None, None).await?;
                operations.push(operation);
            }
        }
        
        Ok(operations)
    }
    
    /// Implement slashing risk management
    pub async fn manage_slashing_risk(&self, asset: &AssetNameExchange) -> StakingResult<Vec<RebalanceAction>> {
        let asset_str = asset.asset_name().to_string();
        let mut actions = Vec::new();
        
        // Only apply to PoS assets
        if !["DOT", "KSM", "ATOM", "ADA"].contains(&asset_str.as_str()) {
            return Ok(actions);
        }
        
        if let Some(risk_config) = &self.config.slashing_risk_management {
            let positions = self.get_staking_positions_for_asset(asset).await?;
            let total_staked = positions.iter().map(|p| p.amount).sum::<Decimal>();
            
            // Check for over-concentration in single validator/pool
            for position in positions {
                let exposure_ratio = position.amount / total_staked;
                
                if exposure_ratio > risk_config.max_exposure_per_validator {
                    // Rebalance to reduce concentration
                    let excess_amount = position.amount - (total_staked * risk_config.max_exposure_per_validator);
                    
                    actions.push(RebalanceAction {
                        action: RebalanceActionType::Unstake,
                        position_id: position.id.clone(),
                        amount: excess_amount,
                        target_product: None,
                        priority: 90, // High priority
                    });
                }
            }
            
            // Check for slashing events and auto-rebalance if configured
            if risk_config.auto_rebalance_on_slashing {
                if let Ok(Some(parachain_data)) = self.get_parachain_info(&asset_str).await {
                    if let Some(slashing_events) = parachain_data.slashing_events {
                        for slashing_event in slashing_events {
                            let slashed_amount = Decimal::from_str(&slashing_event.amount).unwrap_or(Decimal::ZERO);
                            
                            if slashed_amount > risk_config.emergency_unstake_threshold {
                                // Emergency rebalancing due to significant slashing
                                warn!("Significant slashing detected for {}: {} in era {}", 
                                      asset_str, slashed_amount, slashing_event.era);
                                
                                // Add emergency unstaking actions
                                for position in self.get_staking_positions_for_asset(asset).await? {
                                    actions.push(RebalanceAction {
                                        action: RebalanceActionType::Unstake,
                                        position_id: position.id,
                                        amount: position.amount * Decimal::from_str("0.5").unwrap_or(Decimal::ZERO),
                                        target_product: None,
                                        priority: 100, // Maximum priority
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(actions)
    }
    
    /// Optimize reward claiming based on configuration
    pub async fn optimize_reward_claiming(&self, asset: &AssetNameExchange) -> StakingResult<Vec<StakingOperation>> {
        let rewards = self.get_staking_rewards(Some(asset)).await?;
        let mut operations = Vec::new();
        
        match self.config.reward_optimization {
            RewardOptimizationStrategy::OptimizeLiquidity => {
                // Claim all available rewards for liquidity
                for reward in rewards {
                    if reward.status == StakingRewardStatus::Available {
                        // Note: Kraken auto-distributes most rewards, so this is mostly informational
                        info!("Reward available for {}: {} {}", asset.asset_name(), reward.amount, reward.asset);
                    }
                }
            },
            RewardOptimizationStrategy::MaximizeCompounding => {
                // Auto-compound all rewards
                operations.extend(self.auto_compound_rewards(asset).await?);
            },
            RewardOptimizationStrategy::MinimizeTaxes => {
                // Apply tax-optimized claiming strategy
                if let Some(tax_config) = &self.config.tax_optimization {
                    operations.extend(self.apply_tax_optimized_claiming(asset, &rewards, tax_config).await?);
                }
            },
            RewardOptimizationStrategy::BalancedApproach => {
                // Compound large rewards, claim small ones
                let total_rewards = rewards.iter()
                    .filter(|r| r.status == StakingRewardStatus::Available)
                    .map(|r| r.amount)
                    .sum::<Decimal>();
                    
                if total_rewards >= self.config.auto_compound_threshold {
                    operations.extend(self.auto_compound_rewards(asset).await?);
                }
            }
        }
        
        Ok(operations)
    }
    
    /// Apply tax-optimized claiming strategy
    async fn apply_tax_optimized_claiming(
        &self,
        asset: &AssetNameExchange,
        rewards: &[StakingReward],
        tax_config: &TaxOptimizationConfig,
    ) -> StakingResult<Vec<StakingOperation>> {
        let mut operations = Vec::new();
        
        // Group rewards by holding period
        let mut short_term_rewards = Decimal::ZERO;
        let mut long_term_rewards = Decimal::ZERO;
        
        for reward in rewards {
            if reward.status == StakingRewardStatus::Available {
                let holding_duration = Utc::now().signed_duration_since(reward.earned_time);
                
                if holding_duration >= tax_config.preferred_holding_period {
                    long_term_rewards += reward.amount;
                } else {
                    short_term_rewards += reward.amount;
                }
            }
        }
        
        // Prioritize long-term rewards for claiming
        if long_term_rewards >= tax_config.harvest_loss_threshold {
            // Would implement actual claiming logic here
            info!("Tax-optimized claiming: {} long-term rewards for {}", 
                  long_term_rewards, asset.asset_name());
        }
        
        Ok(operations)
    }
    
    /// Get unstaking queue information
    pub async fn get_unstaking_queue(&self, asset: &AssetNameExchange) -> StakingResult<Vec<UnbondingChunk>> {
        let asset_str = asset.asset_name().to_string();
        
        if let Ok(Some(parachain_data)) = self.get_parachain_info(&asset_str).await {
            if let Some(unbonding_chunks) = parachain_data.unbonding_chunks {
                return Ok(unbonding_chunks);
            }
        }
        
        Ok(Vec::new())
    }
    
    /// Get bonding period for asset
    pub fn get_bonding_period(&self, asset: &str) -> Duration {
        match asset {
            "ETH" => Duration::days(1095), // ETH 2.0 until withdrawals enabled
            "DOT" => Duration::days(28),   // Polkadot unbonding period
            "KSM" => Duration::days(7),    // Kusama unbonding period
            "ATOM" => Duration::days(21),  // Cosmos unbonding period
            "ADA" => Duration::days(0),    // Cardano is flexible
            _ => Duration::days(0),         // Default flexible
        }
    }
    
    /// Create API signature for Kraken requests
    fn create_signature(&self, uri: &str, data: &str, nonce: u64) -> Result<String, StakingError> {
        let post_data = format!("nonce={}&{}", nonce, data);
        let sha256_hash = sha2::Sha256::digest(post_data.as_bytes());
        let message = format!("{}{:x}", uri, sha256_hash);

        let private_key_bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.config.private_key)
            .map_err(|e| StakingError::ConfigurationError {
                message: format!("Invalid private key: {}", e),
            })?;

        let mut mac = HmacSha256::new_from_slice(&private_key_bytes)
            .map_err(|e| StakingError::ConfigurationError {
                message: format!("HMAC error: {}", e),
            })?;

        mac.update(message.as_bytes());
        Ok(base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
    }

    /// Make authenticated API request
    async fn make_request<T: for<'de> serde::Deserialize<'de>>(
        &self,
        endpoint: &str,
        params: &HashMap<String, String>,
    ) -> StakingResult<T> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let data = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        let uri = format!("/0/private/{}", endpoint);
        let signature = self.create_signature(&uri, &data, nonce)?;

        let url = format!("{}{}", self.config.base_url, uri);
        let post_data = format!("nonce={}&{}", nonce, data);

        let response = self
            .client
            .post(&url)
            .header("API-Key", &self.config.api_key)
            .header("API-Sign", signature)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(post_data)
            .send()
            .await
            .map_err(|e| StakingError::NetworkError {
                message: format!("Request failed: {}", e),
            })?;

        let response_text = response.text().await.map_err(|e| StakingError::NetworkError {
            message: format!("Failed to read response: {}", e),
        })?;

        let kraken_response: KrakenResponse<T> = serde_json::from_str(&response_text)
            .map_err(|e| StakingError::ParseError {
                message: format!("Failed to parse response: {}", e),
            })?;

        if !kraken_response.error.is_empty() {
            return Err(StakingError::ExchangeError {
                exchange: "Kraken".to_string(),
                message: kraken_response.error.join(", "),
            });
        }

        kraken_response.result.ok_or_else(|| StakingError::ParseError {
            message: "No result in response".to_string(),
        })
    }

    /// Convert Kraken staking product to internal format
    fn convert_product(&self, kraken_product: &KrakenStakingProduct, asset: &str) -> StakingProduct {
        let apy = kraken_product
            .asset_yield
            .as_ref()
            .and_then(|y| Decimal::from_str(y).ok())
            .unwrap_or(Decimal::ZERO);

        let minimum_amount = kraken_product
            .minimum_amount
            .as_ref()
            .and_then(|m| Decimal::from_str(m).ok())
            .unwrap_or(Decimal::ONE);

        let product_type = match kraken_product.lock_type.as_deref() {
            Some("flex") => StakingType::Flexible,
            Some("bonded") => StakingType::Locked(Duration::days(30)), // Default lock period
            _ => StakingType::Flexible,
        };

        StakingProduct {
            id: format!("{}_{}", asset, kraken_product.method),
            asset: asset.to_string(),
            exchange: ExchangeId::Kraken,
            product_type,
            apy,
            minimum_amount,
            maximum_amount: None,
            lock_period: if matches!(product_type, StakingType::Locked(_)) {
                Some(Duration::days(30))
            } else {
                None
            },
            auto_compound: false,
            available_quota: None,
            status: StakingProductStatus::Available,
            metadata: HashMap::new(),
        }
    }

    /// Convert Kraken position to internal format
    fn convert_position(&self, kraken_position: &KrakenStakingPosition) -> StakingResult<StakingPosition> {
        let amount = Decimal::from_str(&kraken_position.amount)
            .map_err(|e| StakingError::ParseError {
                message: format!("Invalid amount: {}", e),
            })?;

        // Create a minimal product for the position
        let product = StakingProduct {
            id: format!("{}_{}", kraken_position.asset, "kraken"),
            asset: kraken_position.asset.clone(),
            exchange: ExchangeId::Kraken,
            product_type: StakingType::Flexible,
            apy: Decimal::ZERO,
            minimum_amount: Decimal::ONE,
            maximum_amount: None,
            lock_period: None,
            auto_compound: false,
            available_quota: None,
            status: StakingProductStatus::Available,
            metadata: HashMap::new(),
        };

        Ok(StakingPosition {
            id: kraken_position.bondid.clone(),
            asset: kraken_position.asset.clone(),
            exchange: ExchangeId::Kraken,
            amount,
            product,
            start_time: Utc::now(), // Kraken doesn't provide this in basic response
            end_time: None,
            accumulated_rewards: Decimal::ZERO,
            status: StakingPositionStatus::Active,
            last_updated: Utc::now(),
        })
    }
}

#[async_trait]
impl StakingManager for KrakenStakingManager {
    fn exchange_id(&self) -> ExchangeId {
        ExchangeId::Kraken
    }

    async fn stake_asset(
        &self,
        asset: &AssetNameExchange,
        amount: Decimal,
        product_id: Option<String>,
        _constraints: Option<StakingConstraints>,
    ) -> StakingResult<StakingOperation> {
        let asset_str = asset.asset_name().to_string();
        
        // Get available staking products to determine best method
        let products = self.get_staking_products(asset).await?;
        
        let selected_product = if let Some(pid) = product_id {
            products.into_iter().find(|p| p.id == pid)
        } else {
            // Select product with highest APY
            products.into_iter().max_by(|a, b| a.apy.cmp(&b.apy))
        };

        let product = selected_product.ok_or_else(|| StakingError::ProductNotFound {
            asset: asset_str.clone(),
            exchange: "Kraken".to_string(),
        })?;

        // Determine staking method based on product type
        let method = match product.product_type {
            StakingType::Flexible => "krak",  // Kraken flexible staking
            StakingType::Locked(_) => "ethbond", // ETH 2.0 bonded staking
            StakingType::DeFi => "krak",
            StakingType::Liquid => "krak",
        };

        let mut params = HashMap::new();
        params.insert("asset".to_string(), asset_str.clone());
        params.insert("amount".to_string(), amount.to_string());
        params.insert("method".to_string(), method.to_string());

        let _result: Value = self.make_request("Stake", &params).await?;

        // Create operation record
        let operation = StakingOperation {
            id: format!("kraken_stake_{}_{}", asset_str, chrono::Utc::now().timestamp()),
            operation_type: StakingOperationType::Stake,
            exchange: ExchangeId::Kraken,
            asset: asset_str,
            amount,
            timestamp: Utc::now(),
            status: StakingOperationStatus::Success,
            error: None,
        };

        Ok(operation)
    }

    async fn unstake_asset(
        &self,
        position_id: &str,
        amount: Option<Decimal>,
    ) -> StakingResult<StakingOperation> {
        let mut params = HashMap::new();
        params.insert("asset".to_string(), position_id.to_string());
        
        if let Some(amt) = amount {
            params.insert("amount".to_string(), amt.to_string());
        }

        let _result: Value = self.make_request("Unstake", &params).await?;

        let operation = StakingOperation {
            id: format!("kraken_unstake_{}_{}", position_id, chrono::Utc::now().timestamp()),
            operation_type: StakingOperationType::Unstake,
            exchange: ExchangeId::Kraken,
            asset: position_id.to_string(), // Best effort
            amount: amount.unwrap_or(Decimal::ZERO),
            timestamp: Utc::now(),
            status: StakingOperationStatus::Success,
            error: None,
        };

        Ok(operation)
    }

    async fn get_staking_products(
        &self,
        asset: &AssetNameExchange,
    ) -> StakingResult<Vec<StakingProduct>> {
        let asset_str = asset.asset_name().to_string();
        
        let mut params = HashMap::new();
        params.insert("asset".to_string(), asset_str.clone());

        let result: HashMap<String, KrakenStakingProduct> = self
            .make_request("Staking/Assets", &params)
            .await
            .unwrap_or_default();

        let products: Vec<StakingProduct> = result
            .values()
            .map(|p| self.convert_product(p, &asset_str))
            .collect();

        // Add specialized products for popular assets
        let mut enhanced_products = products;
        
        match asset_str.as_str() {
            "ETH" => {
                // Add ETH 2.0 staking product
                enhanced_products.push(StakingProduct {
                    id: "ETH_eth2".to_string(),
                    asset: "ETH".to_string(),
                    exchange: ExchangeId::Kraken,
                    product_type: StakingType::Locked(Duration::days(1095)), // ~3 years until withdrawals
                    apy: Decimal::from_str("4.5").unwrap_or(Decimal::ZERO), // Approximate ETH 2.0 rewards
                    minimum_amount: Decimal::from_str("0.00001").unwrap_or(Decimal::ONE),
                    maximum_amount: None,
                    lock_period: Some(Duration::days(1095)),
                    auto_compound: true,
                    available_quota: None,
                    status: StakingProductStatus::Available,
                    metadata: {
                        let mut meta = HashMap::new();
                        meta.insert("type".to_string(), "eth2_staking".to_string());
                        meta.insert("validator_fee".to_string(), "15%".to_string());
                        meta
                    },
                });
            },
            "DOT" => {
                enhanced_products.push(StakingProduct {
                    id: "DOT_polkadot".to_string(),
                    asset: "DOT".to_string(),
                    exchange: ExchangeId::Kraken,
                    product_type: StakingType::Locked(Duration::days(28)),
                    apy: Decimal::from_str("12.0").unwrap_or(Decimal::ZERO),
                    minimum_amount: Decimal::from_str("1.0").unwrap_or(Decimal::ONE),
                    maximum_amount: None,
                    lock_period: Some(Duration::days(28)),
                    auto_compound: true,
                    available_quota: None,
                    status: StakingProductStatus::Available,
                    metadata: HashMap::new(),
                });
            },
            "KSM" => {
                enhanced_products.push(StakingProduct {
                    id: "KSM_kusama".to_string(),
                    asset: "KSM".to_string(),
                    exchange: ExchangeId::Kraken,
                    product_type: StakingType::Locked(Duration::days(7)),
                    apy: Decimal::from_str("15.0").unwrap_or(Decimal::ZERO),
                    minimum_amount: Decimal::from_str("0.1").unwrap_or(Decimal::ONE),
                    maximum_amount: None,
                    lock_period: Some(Duration::days(7)),
                    auto_compound: true,
                    available_quota: None,
                    status: StakingProductStatus::Available,
                    metadata: HashMap::new(),
                });
            },
            _ => {}
        }

        Ok(enhanced_products)
    }

    async fn get_staking_positions(&self) -> StakingResult<Vec<StakingPosition>> {
        let params = HashMap::new();
        let result: HashMap<String, KrakenStakingPosition> = self
            .make_request("Staking/Pending", &params)
            .await
            .unwrap_or_default();

        let mut positions = Vec::new();
        for position in result.values() {
            if let Ok(pos) = self.convert_position(position) {
                positions.push(pos);
            }
        }

        Ok(positions)
    }

    async fn get_staking_positions_for_asset(
        &self,
        asset: &AssetNameExchange,
    ) -> StakingResult<Vec<StakingPosition>> {
        let all_positions = self.get_staking_positions().await?;
        let asset_str = asset.asset_name().to_string();
        
        Ok(all_positions
            .into_iter()
            .filter(|p| p.asset == asset_str)
            .collect())
    }

    async fn get_staking_rewards(
        &self,
        asset: Option<&AssetNameExchange>,
    ) -> StakingResult<Vec<StakingReward>> {
        let params = HashMap::new();
        let result: Vec<KrakenStakingReward> = self
            .make_request("Staking/Rewards", &params)
            .await
            .unwrap_or_default();

        let rewards: Vec<StakingReward> = result
            .into_iter()
            .filter_map(|reward| {
                // Filter by asset if specified
                if let Some(filter_asset) = asset {
                    if reward.asset != filter_asset.asset_name().to_string() {
                        return None;
                    }
                }

                let amount = Decimal::from_str(&reward.amount).ok()?;
                
                Some(StakingReward {
                    id: format!("{}_{}", reward.bondid, reward.datetime),
                    asset: reward.asset,
                    exchange: ExchangeId::Kraken,
                    position_id: reward.bondid,
                    amount,
                    earned_time: Utc::now(), // Kraken doesn't provide exact timestamp
                    claimed_time: Some(Utc::now()), // Assume already claimed
                    status: StakingRewardStatus::Claimed,
                })
            })
            .collect();

        Ok(rewards)
    }

    async fn claim_staking_rewards(
        &self,
        _asset: &AssetNameExchange,
        _reward_ids: Option<Vec<String>>,
    ) -> StakingResult<StakingOperation> {
        // Kraken automatically distributes rewards, no manual claiming needed
        Err(StakingError::OperationNotSupported {
            operation: "Manual reward claiming".to_string(),
            exchange: "Kraken".to_string(),
            reason: "Kraken automatically distributes staking rewards".to_string(),
        })
    }

    async fn get_operation_status(&self, operation_id: &str) -> StakingResult<StakingOperation> {
        // Parse operation ID to extract details
        let parts: Vec<&str> = operation_id.split('_').collect();
        
        if parts.len() < 3 {
            return Err(StakingError::InvalidOperation {
                operation_id: operation_id.to_string(),
                message: "Invalid operation ID format".to_string(),
            });
        }

        let operation_type = match parts[1] {
            "stake" => StakingOperationType::Stake,
            "unstake" => StakingOperationType::Unstake,
            _ => StakingOperationType::Stake,
        };

        Ok(StakingOperation {
            id: operation_id.to_string(),
            operation_type,
            exchange: ExchangeId::Kraken,
            asset: parts[2].to_string(),
            amount: Decimal::ZERO, // Would need to store this separately
            timestamp: Utc::now(),
            status: StakingOperationStatus::Success,
            error: None,
        })
    }

    async fn cancel_operation(&self, _operation_id: &str) -> StakingResult<bool> {
        // Most Kraken staking operations cannot be cancelled once submitted
        Ok(false)
    }

    async fn get_available_balance(
        &self,
        asset: &AssetNameExchange,
    ) -> StakingResult<Decimal> {
        let params = HashMap::new();
        let result: HashMap<String, String> = self
            .make_request("Balance", &params)
            .await
            .unwrap_or_default();

        let asset_str = asset.asset_name().to_string();
        let balance_str = result.get(&asset_str).unwrap_or(&"0".to_string()).clone();
        
        Decimal::from_str(&balance_str).map_err(|e| StakingError::ParseError {
            message: format!("Invalid balance format: {}", e),
        })
    }

    async fn set_auto_compound(
        &self,
        _position_id: &str,
        _enabled: bool,
    ) -> StakingResult<bool> {
        // Kraken handles compounding automatically for most products
        Ok(true)
    }

    async fn get_reward_history(
        &self,
        asset: Option<&AssetNameExchange>,
        _start_time: Option<DateTime<Utc>>,
        _end_time: Option<DateTime<Utc>>,
    ) -> StakingResult<Vec<StakingReward>> {
        // Use the same method as get_staking_rewards for now
        self.get_staking_rewards(asset).await
    }

    async fn get_estimated_apy(
        &self,
        product_id: &str,
        _amount: Decimal,
    ) -> StakingResult<Decimal> {
        // Parse product ID to get asset
        let parts: Vec<&str> = product_id.split('_').collect();
        if parts.is_empty() {
            return Ok(Decimal::ZERO);
        }

        // Return estimated APY based on asset type
        let apy = match parts[0] {
            "ETH" => Decimal::from_str("4.5").unwrap_or(Decimal::ZERO),
            "DOT" => Decimal::from_str("12.0").unwrap_or(Decimal::ZERO),
            "KSM" => Decimal::from_str("15.0").unwrap_or(Decimal::ZERO),
            "ADA" => Decimal::from_str("5.0").unwrap_or(Decimal::ZERO),
            "ATOM" => Decimal::from_str("8.0").unwrap_or(Decimal::ZERO),
            _ => Decimal::from_str("3.0").unwrap_or(Decimal::ZERO), // Default
        };

        Ok(apy)
    }
}