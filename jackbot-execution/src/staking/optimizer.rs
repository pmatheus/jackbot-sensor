//! Yield optimization engine for staking operations
//!
//! Implements multi-objective optimization algorithms to find the best
//! staking product allocations based on yield, risk, and liquidity constraints.

use crate::staking::{
    error::{StakingError, StakingResult},
    *,
};
use chrono::Duration;
use rust_decimal::{prelude::FromPrimitive, prelude::FromStr, prelude::ToPrimitive, Decimal};
use std::collections::HashMap;

/// Yield optimizer for finding optimal staking allocations
#[derive(Debug, Clone)]
pub struct YieldOptimizer {
    /// Risk tolerance settings
    pub risk_settings: RiskSettings,
    /// Optimization parameters
    pub optimization_params: OptimizationParams,
}

/// Risk assessment and tolerance settings
#[derive(Debug, Clone)]
pub struct RiskSettings {
    /// Maximum percentage of portfolio in any single exchange
    pub max_exchange_exposure: Decimal,
    /// Maximum percentage in any single asset
    pub max_asset_exposure: Decimal,
    /// Maximum percentage in locked products
    pub max_locked_exposure: Decimal,
    /// Minimum liquidity buffer (unlocked assets)
    pub min_liquidity_buffer: Decimal,
    /// Risk tolerance level
    pub risk_tolerance: RiskTolerance,
}

impl Default for RiskSettings {
    fn default() -> Self {
        Self {
            max_exchange_exposure: Decimal::from_str("0.30").unwrap(), // 30%
            max_asset_exposure: Decimal::from_str("0.50").unwrap(),    // 50%
            max_locked_exposure: Decimal::from_str("0.40").unwrap(),   // 40%
            min_liquidity_buffer: Decimal::from_str("0.10").unwrap(),  // 10%
            risk_tolerance: RiskTolerance::Moderate,
        }
    }
}

/// Optimization algorithm parameters
#[derive(Debug, Clone)]
pub struct OptimizationParams {
    /// Weight for yield in optimization (0-1)
    pub yield_weight: Decimal,
    /// Weight for risk in optimization (0-1)
    pub risk_weight: Decimal,
    /// Weight for liquidity in optimization (0-1)
    pub liquidity_weight: Decimal,
    /// Maximum number of products to include
    pub max_products: usize,
    /// Minimum allocation per product
    pub min_allocation: Decimal,
}

impl Default for OptimizationParams {
    fn default() -> Self {
        Self {
            yield_weight: Decimal::from_str("0.5").unwrap(),
            risk_weight: Decimal::from_str("0.3").unwrap(),
            liquidity_weight: Decimal::from_str("0.2").unwrap(),
            max_products: 8,
            min_allocation: Decimal::from_str("0.05").unwrap(), // 5% minimum
        }
    }
}

/// Optimization objective and scoring
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizationScore {
    /// Yield component score (0-100)
    pub yield_score: f64,
    /// Risk component score (0-100, higher is better/safer)
    pub risk_score: f64,
    /// Liquidity component score (0-100)
    pub liquidity_score: f64,
    /// Overall weighted score (0-100)
    pub total_score: f64,
}

impl YieldOptimizer {
    /// Create a new yield optimizer with default settings
    pub fn new() -> Self {
        Self {
            risk_settings: RiskSettings::default(),
            optimization_params: OptimizationParams::default(),
        }
    }

    /// Create optimizer with custom settings
    pub fn with_settings(
        risk_settings: RiskSettings,
        optimization_params: OptimizationParams,
    ) -> Self {
        Self {
            risk_settings,
            optimization_params,
        }
    }

    /// Find the best staking products for a given asset and amount
    pub fn find_best_products(
        &self,
        products: &[StakingProduct],
        total_amount: Decimal,
        constraints: Option<&StakingConstraints>,
    ) -> StakingResult<Vec<StakingRecommendation>> {
        if products.is_empty() {
            return Err(StakingError::InternalError {
                message: "No products available for optimization".to_string(),
            });
        }

        // Filter products based on constraints
        let filtered_products = self.apply_constraints(products, constraints);
        if filtered_products.is_empty() {
            return Err(StakingError::InternalError {
                message: "No products match the specified constraints".to_string(),
            });
        }

        // Score all products
        let scored_products = self.score_products(&filtered_products, total_amount)?;

        // Optimize allocation using multi-objective algorithm
        let allocations = self.optimize_allocation(&scored_products, total_amount)?;

        // Convert to recommendations
        let recommendations = allocations
            .into_iter()
            .map(|(product, amount, score)| {
                let expected_return = amount * product.apy / Decimal::from(100);
                let risk_score = (score.risk_score * 100.0) as u8;
                let confidence = (score.total_score * 100.0) as u8;

                StakingRecommendation {
                    product,
                    amount,
                    expected_return,
                    risk_score,
                    confidence,
                    reasoning: self.generate_reasoning(&score),
                }
            })
            .collect();

        Ok(recommendations)
    }

    /// Recommend rebalancing actions for existing positions
    pub fn rebalance_positions(
        &self,
        current_positions: &[StakingPosition],
        available_products: &[StakingProduct],
        target_yield: Option<Decimal>,
    ) -> StakingResult<Vec<RebalanceAction>> {
        let mut actions = Vec::new();

        // Calculate current allocation
        let total_staked = current_positions.iter().map(|p| p.amount).sum::<Decimal>();
        if total_staked == Decimal::ZERO {
            return Ok(actions);
        }

        let _target_yield = target_yield; // Mark as used to avoid warning

        // Check for rebalancing opportunities
        for position in current_positions {
            // Find better alternatives
            let better_products = available_products
                .iter()
                .filter(|p| {
                    p.asset == position.asset
                        && p.apy > position.product.apy
                        && self.is_suitable_replacement(p, &position.product)
                })
                .collect::<Vec<_>>();

            if let Some(best_alternative) = better_products.first() {
                let yield_improvement = best_alternative.apy - position.product.apy;

                // Only recommend if improvement is significant
                if yield_improvement > Decimal::from_str("0.005").unwrap() {
                    // 0.5% improvement
                    let priority = self.calculate_rebalance_priority(&yield_improvement, position);

                    actions.push(RebalanceAction {
                        action: RebalanceActionType::Move,
                        position_id: position.id.clone(),
                        amount: position.amount,
                        target_product: Some((*best_alternative).clone()),
                        priority,
                    });
                }
            }

            // Check for risk violations
            if self.violates_risk_limits(position, current_positions, total_staked) {
                actions.push(RebalanceAction {
                    action: RebalanceActionType::Unstake,
                    position_id: position.id.clone(),
                    amount: self.calculate_excess_amount(position, current_positions, total_staked),
                    target_product: None,
                    priority: 90, // High priority for risk violations
                });
            }
        }

        // Sort by priority
        actions.sort_by(|a, b| b.priority.cmp(&a.priority));

        Ok(actions)
    }

    /// Calculate optimal allocation strategy
    pub fn calculate_optimal_allocation(
        &self,
        assets: &[String],
        available_balance: &HashMap<String, Decimal>,
        products: &[StakingProduct],
    ) -> StakingResult<AllocationStrategy> {
        let mut exchange_allocations = HashMap::new();
        let mut asset_allocations = HashMap::new();
        let mut type_allocations = HashMap::new();

        let total_balance = available_balance.values().sum::<Decimal>();

        // Calculate exchange allocation based on risk settings
        let exchanges: Vec<_> = products.iter().map(|p| p.exchange).collect();
        let unique_exchanges: std::collections::HashSet<_> = exchanges.into_iter().collect();
        let exchange_count = unique_exchanges.len();

        for exchange in unique_exchanges {
            let allocation = (Decimal::ONE / Decimal::from(exchange_count))
                .min(self.risk_settings.max_exchange_exposure);
            exchange_allocations.insert(exchange, allocation);
        }

        // Calculate asset allocation
        for asset in assets {
            let balance = available_balance
                .get(asset)
                .copied()
                .unwrap_or(Decimal::ZERO);
            let allocation = balance / total_balance;
            asset_allocations.insert(asset.clone(), allocation);
        }

        // Calculate type allocation based on risk tolerance
        match self.risk_settings.risk_tolerance {
            RiskTolerance::Conservative => {
                type_allocations.insert(StakingType::Flexible, Decimal::from_str("0.7").unwrap());
                type_allocations.insert(
                    StakingType::Locked(Duration::days(30)),
                    Decimal::from_str("0.3").unwrap(),
                );
            }
            RiskTolerance::Moderate => {
                type_allocations.insert(StakingType::Flexible, Decimal::from_str("0.5").unwrap());
                type_allocations.insert(
                    StakingType::Locked(Duration::days(90)),
                    Decimal::from_str("0.4").unwrap(),
                );
                type_allocations.insert(StakingType::DeFi, Decimal::from_str("0.1").unwrap());
            }
            RiskTolerance::Aggressive => {
                type_allocations.insert(StakingType::Flexible, Decimal::from_str("0.3").unwrap());
                type_allocations.insert(
                    StakingType::Locked(Duration::days(180)),
                    Decimal::from_str("0.4").unwrap(),
                );
                type_allocations.insert(StakingType::DeFi, Decimal::from_str("0.3").unwrap());
            }
        }

        Ok(AllocationStrategy {
            exchange_allocations,
            asset_allocations,
            type_allocations,
            rebalance_frequency: Duration::days(7), // Weekly rebalancing
        })
    }

    fn apply_constraints(
        &self,
        products: &[StakingProduct],
        constraints: Option<&StakingConstraints>,
    ) -> Vec<StakingProduct> {
        let Some(constraints) = constraints else {
            return products.to_vec();
        };

        products
            .iter()
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
            .cloned()
            .collect()
    }

    fn score_products(
        &self,
        products: &[StakingProduct],
        _total_amount: Decimal,
    ) -> StakingResult<Vec<(StakingProduct, OptimizationScore)>> {
        let max_apy = products
            .iter()
            .map(|p| p.apy)
            .max()
            .unwrap_or(Decimal::ZERO);
        let min_apy = products
            .iter()
            .map(|p| p.apy)
            .min()
            .unwrap_or(Decimal::ZERO);

        let scored_products = products
            .iter()
            .map(|product| {
                let yield_score = if max_apy == min_apy {
                    100.0
                } else {
                    ((product.apy - min_apy) / (max_apy - min_apy))
                        .to_f64()
                        .unwrap_or(0.0)
                        * 100.0
                };

                let risk_score = self.calculate_risk_score(product);
                let liquidity_score = self.calculate_liquidity_score(product);

                let total_score = (yield_score
                    * self
                        .optimization_params
                        .yield_weight
                        .to_f64()
                        .unwrap_or(0.5))
                    + (risk_score * self.optimization_params.risk_weight.to_f64().unwrap_or(0.3))
                    + (liquidity_score
                        * self
                            .optimization_params
                            .liquidity_weight
                            .to_f64()
                            .unwrap_or(0.2));

                let score = OptimizationScore {
                    yield_score,
                    risk_score,
                    liquidity_score,
                    total_score,
                };

                (product.clone(), score)
            })
            .collect();

        Ok(scored_products)
    }

    fn optimize_allocation(
        &self,
        scored_products: &[(StakingProduct, OptimizationScore)],
        total_amount: Decimal,
    ) -> StakingResult<Vec<(StakingProduct, Decimal, OptimizationScore)>> {
        // Sort by total score
        let mut sorted_products = scored_products.to_vec();
        sorted_products.sort_by(|a, b| b.1.total_score.partial_cmp(&a.1.total_score).unwrap());

        // Take top products up to max_products limit
        let selected_products = sorted_products
            .into_iter()
            .take(self.optimization_params.max_products)
            .collect::<Vec<_>>();

        // Calculate allocations using weighted scoring
        let total_score: f64 = selected_products
            .iter()
            .map(|(_, score)| score.total_score)
            .sum();

        if total_score == 0.0 {
            return Err(StakingError::InternalError {
                message: "Total optimization score is zero".to_string(),
            });
        }

        let mut allocations = Vec::new();
        let mut remaining_amount = total_amount;

        for (i, (product, score)) in selected_products.iter().enumerate() {
            let allocation_ratio =
                Decimal::from_f64(score.total_score / total_score).unwrap_or(Decimal::ZERO);

            let allocated_amount = if i == selected_products.len() - 1 {
                // Last product gets remaining amount
                remaining_amount
            } else {
                let amount = total_amount * allocation_ratio;
                remaining_amount -= amount;
                amount
            };

            // Check minimum allocation
            if allocated_amount >= self.optimization_params.min_allocation * total_amount {
                allocations.push((product.clone(), allocated_amount, score.clone()));
            }
        }

        if allocations.is_empty() {
            return Err(StakingError::InternalError {
                message: "No valid allocations found".to_string(),
            });
        }

        Ok(allocations)
    }

    fn calculate_risk_score(&self, product: &StakingProduct) -> f64 {
        let mut score = 100.0;

        // Penalize based on lock period
        if let Some(lock_period) = product.lock_period {
            let days = lock_period.num_days();
            score -= (days as f64 * 0.1).min(30.0); // Max 30 point penalty
        }

        // Adjust based on exchange reputation (simplified)
        match product.exchange {
            jackbot_instrument::exchange::ExchangeId::BinanceSpot
            | jackbot_instrument::exchange::ExchangeId::Coinbase
            | jackbot_instrument::exchange::ExchangeId::Kraken => score += 10.0,
            jackbot_instrument::exchange::ExchangeId::BybitSpot
            | jackbot_instrument::exchange::ExchangeId::Okx => score += 5.0,
            _ => score -= 5.0,
        }

        // Penalize DeFi products (higher risk)
        if matches!(product.product_type, StakingType::DeFi) {
            score -= 20.0;
        }

        score.max(0.0).min(100.0)
    }

    fn calculate_liquidity_score(&self, product: &StakingProduct) -> f64 {
        match &product.product_type {
            StakingType::Flexible => 100.0,
            StakingType::Liquid => 95.0,
            StakingType::Locked(duration) => {
                let days = duration.num_days();
                (100.0 - days as f64 * 0.5).max(10.0)
            }
            StakingType::DeFi => 70.0, // Variable liquidity
        }
    }

    fn generate_reasoning(&self, score: &OptimizationScore) -> String {
        let mut reasons = Vec::new();

        if score.yield_score > 80.0 {
            reasons.push("High yield potential");
        }
        if score.risk_score > 80.0 {
            reasons.push("Low risk profile");
        }
        if score.liquidity_score > 80.0 {
            reasons.push("High liquidity");
        }

        if reasons.is_empty() {
            "Balanced risk-reward profile".to_string()
        } else {
            reasons.join(", ")
        }
    }

    fn is_suitable_replacement(
        &self,
        new_product: &StakingProduct,
        current_product: &StakingProduct,
    ) -> bool {
        // Check if the new product is a suitable replacement
        match (&new_product.product_type, &current_product.product_type) {
            (StakingType::Flexible, _) => true, // Flexible is always suitable
            (StakingType::Locked(new_duration), StakingType::Locked(current_duration)) => {
                new_duration <= current_duration // Don't increase lock period
            }
            (StakingType::Locked(_), StakingType::Flexible) => false, // Don't lock flexible
            _ => new_product.apy > current_product.apy,
        }
    }

    fn calculate_rebalance_priority(
        &self,
        yield_improvement: &Decimal,
        position: &StakingPosition,
    ) -> u8 {
        let improvement_percent = yield_improvement * Decimal::from(100);
        let base_priority = (improvement_percent.to_f64().unwrap_or(0.0) * 10.0) as u8;

        // Adjust based on position size
        let size_factor = if position.amount > Decimal::from(10000) {
            10
        } else {
            0
        };

        (base_priority + size_factor).min(100).max(1)
    }

    fn violates_risk_limits(
        &self,
        position: &StakingPosition,
        all_positions: &[StakingPosition],
        total_staked: Decimal,
    ) -> bool {
        // Check exchange exposure
        let exchange_exposure = all_positions
            .iter()
            .filter(|p| p.exchange == position.exchange)
            .map(|p| p.amount)
            .sum::<Decimal>()
            / total_staked;

        if exchange_exposure > self.risk_settings.max_exchange_exposure {
            return true;
        }

        // Check asset exposure
        let asset_exposure = all_positions
            .iter()
            .filter(|p| p.asset == position.asset)
            .map(|p| p.amount)
            .sum::<Decimal>()
            / total_staked;

        if asset_exposure > self.risk_settings.max_asset_exposure {
            return true;
        }

        false
    }

    fn calculate_excess_amount(
        &self,
        position: &StakingPosition,
        all_positions: &[StakingPosition],
        total_staked: Decimal,
    ) -> Decimal {
        // Calculate how much to reduce to meet limits
        let exchange_exposure = all_positions
            .iter()
            .filter(|p| p.exchange == position.exchange)
            .map(|p| p.amount)
            .sum::<Decimal>()
            / total_staked;

        if exchange_exposure > self.risk_settings.max_exchange_exposure {
            let excess_ratio = exchange_exposure - self.risk_settings.max_exchange_exposure;
            return position.amount * excess_ratio;
        }

        Decimal::ZERO
    }
}

impl Default for YieldOptimizer {
    fn default() -> Self {
        Self::new()
    }
}
