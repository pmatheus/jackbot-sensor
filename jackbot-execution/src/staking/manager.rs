//! Core staking manager trait and implementations

use crate::staking::{
    error::{StakingError, StakingResult},
    *,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use jackbot_instrument::{asset::name::AssetNameExchange, exchange::ExchangeId};
use rust_decimal::{prelude::FromStr, Decimal};
use std::collections::HashMap;

/// Core trait for staking operations across all exchanges
#[async_trait]
pub trait StakingManager: Send + Sync + Clone {
    /// Get the exchange ID this manager handles
    fn exchange_id(&self) -> ExchangeId;

    /// Stake an asset with the specified parameters
    async fn stake_asset(
        &self,
        asset: &AssetNameExchange,
        amount: Decimal,
        product_id: Option<String>,
        constraints: Option<StakingConstraints>,
    ) -> StakingResult<StakingOperation>;

    /// Unstake an asset from a specific position
    async fn unstake_asset(
        &self,
        position_id: &str,
        amount: Option<Decimal>, // None means unstake all
    ) -> StakingResult<StakingOperation>;

    /// Get available staking products for an asset
    async fn get_staking_products(
        &self,
        asset: &AssetNameExchange,
    ) -> StakingResult<Vec<StakingProduct>>;

    /// Get all staking positions for the account
    async fn get_staking_positions(&self) -> StakingResult<Vec<StakingPosition>>;

    /// Get staking positions for a specific asset
    async fn get_staking_positions_for_asset(
        &self,
        asset: &AssetNameExchange,
    ) -> StakingResult<Vec<StakingPosition>>;

    /// Get staking rewards for the account
    async fn get_staking_rewards(
        &self,
        asset: Option<&AssetNameExchange>,
    ) -> StakingResult<Vec<StakingReward>>;

    /// Claim staking rewards
    async fn claim_staking_rewards(
        &self,
        asset: &AssetNameExchange,
        reward_ids: Option<Vec<String>>, // None means claim all available
    ) -> StakingResult<StakingOperation>;

    /// Get the status of a staking operation
    async fn get_operation_status(&self, operation_id: &str) -> StakingResult<StakingOperation>;

    /// Cancel a pending staking operation (if supported)
    async fn cancel_operation(&self, operation_id: &str) -> StakingResult<bool>;

    /// Get account balance for staking operations
    async fn get_available_balance(&self, asset: &AssetNameExchange) -> StakingResult<Decimal>;

    /// Set up auto-compound for a position (if supported)
    async fn set_auto_compound(&self, position_id: &str, enabled: bool) -> StakingResult<bool>;

    /// Get historical staking rewards
    async fn get_reward_history(
        &self,
        asset: Option<&AssetNameExchange>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> StakingResult<Vec<StakingReward>>;

    /// Get estimated APY for a specific amount and product
    async fn get_estimated_apy(&self, product_id: &str, amount: Decimal) -> StakingResult<Decimal>;
}

/// Exchange-specific staking manager implementations
#[derive(Debug, Clone)]
pub enum StakingManagerImpl {
    Binance(super::binance::BinanceStakingManager),
    Bybit(super::bybit::BybitStakingManager),
    Okx(super::okx::OKXStakingManager),
}

impl StakingManagerImpl {
    async fn stake_asset(
        &self,
        asset: &AssetNameExchange,
        amount: Decimal,
        product_id: Option<String>,
        constraints: Option<StakingConstraints>,
    ) -> StakingResult<StakingOperation> {
        match self {
            Self::Binance(manager) => {
                manager
                    .stake_asset(asset, amount, product_id, constraints)
                    .await
            }
            Self::Bybit(manager) => {
                manager
                    .stake_asset(asset, amount, product_id, constraints)
                    .await
            }
            Self::Okx(manager) => {
                manager
                    .stake_asset(asset, amount, product_id, constraints)
                    .await
            }
        }
    }

    async fn get_staking_products(
        &self,
        asset: &AssetNameExchange,
    ) -> StakingResult<Vec<StakingProduct>> {
        match self {
            Self::Binance(manager) => manager.get_staking_products(asset).await,
            Self::Bybit(manager) => manager.get_staking_products(asset).await,
            Self::Okx(manager) => manager.get_staking_products(asset).await,
        }
    }

    async fn get_staking_positions(&self) -> StakingResult<Vec<StakingPosition>> {
        match self {
            Self::Binance(manager) => manager.get_staking_positions().await,
            Self::Bybit(manager) => manager.get_staking_positions().await,
            Self::Okx(manager) => manager.get_staking_positions().await,
        }
    }

    async fn get_staking_rewards(
        &self,
        asset: Option<&AssetNameExchange>,
    ) -> StakingResult<Vec<StakingReward>> {
        match self {
            Self::Binance(manager) => manager.get_staking_rewards(asset).await,
            Self::Bybit(manager) => manager.get_staking_rewards(asset).await,
            Self::Okx(manager) => manager.get_staking_rewards(asset).await,
        }
    }

    async fn claim_staking_rewards(
        &self,
        asset: &AssetNameExchange,
        reward_ids: Option<Vec<String>>,
    ) -> StakingResult<StakingOperation> {
        match self {
            Self::Binance(manager) => manager.claim_staking_rewards(asset, reward_ids).await,
            Self::Bybit(manager) => manager.claim_staking_rewards(asset, reward_ids).await,
            Self::Okx(manager) => manager.claim_staking_rewards(asset, reward_ids).await,
        }
    }

    fn exchange_id(&self) -> ExchangeId {
        match self {
            Self::Binance(manager) => manager.exchange_id(),
            Self::Bybit(manager) => manager.exchange_id(),
            Self::Okx(manager) => manager.exchange_id(),
        }
    }
}

/// Unified staking manager that routes operations to exchange-specific managers
#[derive(Debug, Clone)]
pub struct UnifiedStakingManager {
    managers: HashMap<ExchangeId, StakingManagerImpl>,
}

impl UnifiedStakingManager {
    /// Create a new unified staking manager
    pub fn new() -> Self {
        Self {
            managers: HashMap::new(),
        }
    }

    /// Add an exchange-specific staking manager
    pub fn add_manager(&mut self, manager: StakingManagerImpl) {
        let exchange_id = manager.exchange_id();
        self.managers.insert(exchange_id, manager);
    }

    /// Get manager for a specific exchange
    pub fn get_manager(&self, exchange: ExchangeId) -> Option<&StakingManagerImpl> {
        self.managers.get(&exchange)
    }

    /// Get all supported exchanges
    pub fn supported_exchanges(&self) -> Vec<ExchangeId> {
        self.managers.keys().cloned().collect()
    }

    /// Stake across multiple exchanges with optimal allocation
    pub async fn stake_optimized(
        &self,
        asset: &AssetNameExchange,
        total_amount: Decimal,
        constraints: Option<StakingConstraints>,
    ) -> StakingResult<Vec<StakingOperation>> {
        // Get all available products across exchanges
        let mut all_products = Vec::new();
        for manager in self.managers.values() {
            match manager.get_staking_products(asset).await {
                Ok(products) => all_products.extend(products),
                Err(e) => {
                    tracing::warn!(
                        "Failed to get products from {}: {}",
                        manager.exchange_id(),
                        e
                    );
                }
            }
        }

        if all_products.is_empty() {
            return Err(StakingError::AssetNotSupported {
                exchange: ExchangeId::BinanceSpot, // Use a default exchange ID
                asset: asset.clone(),
            });
        }

        // Apply constraints and filter products
        let filtered_products = self.apply_constraints(all_products, &constraints);

        // Optimize allocation across filtered products
        let allocations =
            self.optimize_allocation(&filtered_products, total_amount, &constraints)?;

        // Execute staking operations
        let mut operations = Vec::new();
        for (product, amount) in allocations {
            if let Some(manager) = self.get_manager(product.exchange) {
                match manager
                    .stake_asset(asset, amount, Some(product.id.clone()), constraints.clone())
                    .await
                {
                    Ok(operation) => operations.push(operation),
                    Err(e) => {
                        tracing::error!(
                            "Failed to stake {} {} on {}: {}",
                            amount,
                            asset,
                            product.exchange,
                            e
                        );
                        // Continue with other allocations
                    }
                }
            }
        }

        if operations.is_empty() {
            return Err(StakingError::InternalError {
                message: "No staking operations could be executed".to_string(),
            });
        }

        Ok(operations)
    }

    /// Get aggregated staking positions across all exchanges
    pub async fn get_all_positions(&self) -> StakingResult<Vec<StakingPosition>> {
        let mut all_positions = Vec::new();

        for manager in self.managers.values() {
            match manager.get_staking_positions().await {
                Ok(positions) => all_positions.extend(positions),
                Err(e) => {
                    tracing::warn!(
                        "Failed to get positions from {}: {}",
                        manager.exchange_id(),
                        e
                    );
                }
            }
        }

        Ok(all_positions)
    }

    /// Get aggregated rewards across all exchanges
    pub async fn get_all_rewards(
        &self,
        asset: Option<&AssetNameExchange>,
    ) -> StakingResult<Vec<StakingReward>> {
        let mut all_rewards = Vec::new();

        for manager in self.managers.values() {
            match manager.get_staking_rewards(asset).await {
                Ok(rewards) => all_rewards.extend(rewards),
                Err(e) => {
                    tracing::warn!(
                        "Failed to get rewards from {}: {}",
                        manager.exchange_id(),
                        e
                    );
                }
            }
        }

        Ok(all_rewards)
    }

    /// Claim all available rewards across exchanges
    pub async fn claim_all_rewards(
        &self,
        asset: &AssetNameExchange,
    ) -> StakingResult<Vec<StakingOperation>> {
        let mut operations = Vec::new();

        for manager in self.managers.values() {
            if let Ok(operation) = manager.claim_staking_rewards(asset, None).await {
                operations.push(operation);
            }
        }

        Ok(operations)
    }

    /// Get portfolio summary across all exchanges
    pub async fn get_portfolio_summary(&self) -> StakingResult<StakingPortfolioSummary> {
        let positions = self.get_all_positions().await?;
        let rewards = self.get_all_rewards(None).await?;

        let mut total_staked = Decimal::ZERO;
        let mut total_rewards = Decimal::ZERO;
        let mut exchange_breakdown = HashMap::new();
        let mut asset_breakdown = HashMap::new();

        for position in &positions {
            total_staked += position.amount;
            total_rewards += position.accumulated_rewards;

            *exchange_breakdown
                .entry(position.exchange)
                .or_insert(Decimal::ZERO) += position.amount;
            *asset_breakdown
                .entry(position.asset.clone())
                .or_insert(Decimal::ZERO) += position.amount;
        }

        let available_rewards = rewards
            .iter()
            .filter(|r| r.status == StakingRewardStatus::Available)
            .map(|r| r.amount)
            .sum();

        Ok(StakingPortfolioSummary {
            total_staked_value: total_staked,
            total_accumulated_rewards: total_rewards,
            available_rewards,
            active_positions: positions.len(),
            exchange_breakdown,
            asset_breakdown,
            last_updated: Utc::now(),
        })
    }

    fn apply_constraints(
        &self,
        products: Vec<StakingProduct>,
        constraints: &Option<StakingConstraints>,
    ) -> Vec<StakingProduct> {
        let constraints = match constraints {
            Some(c) => c,
            None => return products,
        };

        products
            .into_iter()
            .filter(|product| {
                // Check minimum APY
                if let Some(min_apy) = constraints.min_apy {
                    if product.apy < min_apy {
                        return false;
                    }
                }

                // Check maximum lock period
                if let Some(max_lock) = constraints.max_lock_period {
                    if let Some(lock_period) = product.lock_period {
                        if lock_period > max_lock {
                            return false;
                        }
                    }
                }

                // Check preferred types
                if !constraints.preferred_types.is_empty()
                    && !constraints.preferred_types.contains(&product.product_type)
                {
                    return false;
                }

                // Check exchange filter
                match &constraints.exchange_filter {
                    ExchangeFilter::All => true,
                    ExchangeFilter::Include(exchanges) => exchanges.contains(&product.exchange),
                    ExchangeFilter::Exclude(exchanges) => !exchanges.contains(&product.exchange),
                }
            })
            .collect()
    }

    fn optimize_allocation(
        &self,
        products: &[StakingProduct],
        total_amount: Decimal,
        constraints: &Option<StakingConstraints>,
    ) -> StakingResult<Vec<(StakingProduct, Decimal)>> {
        if products.is_empty() {
            return Err(StakingError::InternalError {
                message: "No products available for allocation".to_string(),
            });
        }

        // Simple allocation strategy - can be enhanced with sophisticated optimization
        let risk_tolerance = constraints
            .as_ref()
            .map(|c| &c.risk_tolerance)
            .unwrap_or(&RiskTolerance::Moderate);

        match risk_tolerance {
            RiskTolerance::Conservative => {
                // Allocate to highest-rated, lowest-risk products
                let mut sorted_products = products.to_vec();
                sorted_products.sort_by(|a, b| {
                    // Prefer flexible staking and established exchanges
                    match (&a.product_type, &b.product_type) {
                        (StakingType::Flexible, StakingType::Locked(_)) => std::cmp::Ordering::Less,
                        (StakingType::Locked(_), StakingType::Flexible) => {
                            std::cmp::Ordering::Greater
                        }
                        _ => a
                            .apy
                            .partial_cmp(&b.apy)
                            .unwrap_or(std::cmp::Ordering::Equal),
                    }
                });

                // Allocate evenly among top 3 products
                let top_products = sorted_products.into_iter().take(3).collect::<Vec<_>>();
                let amount_per_product = total_amount / Decimal::from(top_products.len());

                Ok(top_products
                    .into_iter()
                    .map(|p| (p, amount_per_product))
                    .collect())
            }
            RiskTolerance::Moderate => {
                // Balanced allocation based on APY and risk
                let mut sorted_products = products.to_vec();
                sorted_products.sort_by(|a, b| {
                    b.apy
                        .partial_cmp(&a.apy)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                // Allocate with decreasing weights
                let mut allocations = Vec::new();
                let mut remaining = total_amount;

                for (i, product) in sorted_products.into_iter().take(5).enumerate() {
                    let weight = Decimal::from(5 - i) / Decimal::from(15); // 5/15, 4/15, 3/15, 2/15, 1/15
                    let amount = total_amount * weight;

                    if amount <= remaining {
                        allocations.push((product, amount));
                        remaining -= amount;
                    }
                }

                Ok(allocations)
            }
            RiskTolerance::Aggressive => {
                // Allocate to highest APY products
                let mut sorted_products = products.to_vec();
                sorted_products.sort_by(|a, b| {
                    b.apy
                        .partial_cmp(&a.apy)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                // Top 2 products get majority allocation
                let top_product = &sorted_products[0];
                let mut allocations = vec![(
                    top_product.clone(),
                    total_amount * Decimal::from_str("0.7").unwrap(),
                )];

                if sorted_products.len() > 1 {
                    let second_product = &sorted_products[1];
                    allocations.push((
                        second_product.clone(),
                        total_amount * Decimal::from_str("0.3").unwrap(),
                    ));
                }

                Ok(allocations)
            }
        }
    }
}

impl Default for UnifiedStakingManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Portfolio summary for staking operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StakingPortfolioSummary {
    /// Total value staked across all exchanges
    pub total_staked_value: Decimal,
    /// Total accumulated rewards
    pub total_accumulated_rewards: Decimal,
    /// Available rewards for claiming
    pub available_rewards: Decimal,
    /// Number of active positions
    pub active_positions: usize,
    /// Breakdown by exchange
    pub exchange_breakdown: HashMap<ExchangeId, Decimal>,
    /// Breakdown by asset
    pub asset_breakdown: HashMap<String, Decimal>,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}
