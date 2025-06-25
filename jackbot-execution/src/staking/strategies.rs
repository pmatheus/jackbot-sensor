//! Automated staking strategies
//!
//! Provides different strategic approaches to staking including:
//! - Maximum yield strategies
//! - Diversified approaches
//! - Conservative strategies
//! - Liquidity-first strategies

use crate::staking::{
    error::StakingResult,
    optimizer::{OptimizationParams, RiskSettings, YieldOptimizer},
    *,
};
use async_trait::async_trait;
use chrono::Duration;
use rust_decimal::{prelude::FromStr, Decimal};
use std::collections::HashMap;

/// Context for strategy execution
#[derive(Debug, Clone)]
pub struct StakingContext {
    /// Available products across exchanges
    pub available_products: Vec<StakingProduct>,
    /// Current staking positions
    pub current_positions: Vec<StakingPosition>,
    /// Available balances by asset
    pub available_balances: HashMap<String, Decimal>,
    /// Market conditions and trends
    pub market_context: MarketContext,
    /// User constraints and preferences
    pub constraints: Option<StakingConstraints>,
}

/// Market context for strategy decisions
#[derive(Debug, Clone)]
pub struct MarketContext {
    /// Current market volatility (0-100)
    pub volatility: f64,
    /// Market trend direction
    pub trend: MarketTrend,
    /// Recent APY changes
    pub apy_trends: HashMap<String, ApyTrend>,
    /// Liquidity conditions
    pub liquidity_conditions: LiquidityConditions,
}

/// Market trend indicators
#[derive(Debug, Clone, PartialEq)]
pub enum MarketTrend {
    Bullish,
    Bearish,
    Sideways,
    Uncertain,
}

/// APY trend for specific assets
#[derive(Debug, Clone)]
pub struct ApyTrend {
    /// Current APY
    pub current_apy: Decimal,
    /// APY 7 days ago
    pub apy_7d_ago: Decimal,
    /// APY 30 days ago
    pub apy_30d_ago: Decimal,
    /// Trend direction
    pub trend_direction: TrendDirection,
}

/// Trend direction
#[derive(Debug, Clone, PartialEq)]
pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
}

/// Liquidity conditions across exchanges
#[derive(Debug, Clone)]
pub struct LiquidityConditions {
    /// Overall liquidity score (0-100)
    pub liquidity_score: f64,
    /// Exchange-specific conditions
    pub exchange_conditions: HashMap<String, f64>,
}

/// Core trait for staking strategies
#[async_trait]
pub trait StakingStrategy: Send + Sync + Clone {
    /// Strategy name
    fn name(&self) -> &str;

    /// Execute the strategy and return recommended actions
    async fn execute(&self, context: &StakingContext) -> StakingResult<Vec<StakingAction>>;

    /// Check if rebalancing is needed
    fn should_rebalance(&self, positions: &[StakingPosition]) -> bool;

    /// Get risk assessment for this strategy
    fn risk_assessment(&self) -> RiskProfile;

    /// Get strategy description
    fn description(&self) -> &str;
}

/// Staking action recommendations
#[derive(Debug, Clone, PartialEq)]
pub enum StakingAction {
    /// Stake a specific amount in a product
    Stake {
        product: StakingProduct,
        amount: Decimal,
        priority: u8,
        reasoning: String,
    },
    /// Unstake from a position
    Unstake {
        position_id: String,
        amount: Option<Decimal>,
        priority: u8,
        reasoning: String,
    },
    /// Move from one product to another
    Move {
        from_position_id: String,
        to_product: StakingProduct,
        amount: Decimal,
        priority: u8,
        reasoning: String,
    },
    /// Claim available rewards
    ClaimRewards {
        asset: String,
        priority: u8,
        reasoning: String,
    },
    /// No action needed
    Hold { reasoning: String },
}

/// Risk profile for strategies
#[derive(Debug, Clone, PartialEq)]
pub struct RiskProfile {
    /// Risk level (0-100)
    pub risk_level: u8,
    /// Maximum drawdown tolerance
    pub max_drawdown: Decimal,
    /// Diversification requirements
    pub diversification: DiversificationProfile,
    /// Liquidity requirements
    pub liquidity_requirements: LiquidityRequirements,
}

/// Diversification profile
#[derive(Debug, Clone, PartialEq)]
pub struct DiversificationProfile {
    /// Maximum exposure to single exchange
    pub max_exchange_exposure: Decimal,
    /// Maximum exposure to single asset
    pub max_asset_exposure: Decimal,
    /// Required number of positions
    pub min_positions: usize,
}

/// Liquidity requirements
#[derive(Debug, Clone, PartialEq)]
pub struct LiquidityRequirements {
    /// Minimum percentage in flexible staking
    pub min_flexible_percentage: Decimal,
    /// Maximum lock period acceptable
    pub max_lock_period: Duration,
    /// Emergency liquidity buffer
    pub emergency_buffer: Decimal,
}

/// Maximum yield strategy - focuses on highest APY
#[derive(Debug, Clone)]
pub struct MaxYieldStrategy {
    optimizer: YieldOptimizer,
}

impl MaxYieldStrategy {
    pub fn new() -> Self {
        let risk_settings = RiskSettings {
            max_exchange_exposure: Decimal::from_str("0.50").unwrap(),
            max_asset_exposure: Decimal::from_str("0.70").unwrap(),
            max_locked_exposure: Decimal::from_str("0.80").unwrap(),
            min_liquidity_buffer: Decimal::from_str("0.05").unwrap(),
            risk_tolerance: RiskTolerance::Aggressive,
        };

        let optimization_params = OptimizationParams {
            yield_weight: Decimal::from_str("0.8").unwrap(),
            risk_weight: Decimal::from_str("0.1").unwrap(),
            liquidity_weight: Decimal::from_str("0.1").unwrap(),
            max_products: 5,
            min_allocation: Decimal::from_str("0.10").unwrap(),
        };

        Self {
            optimizer: YieldOptimizer::with_settings(risk_settings, optimization_params),
        }
    }
}

#[async_trait]
impl StakingStrategy for MaxYieldStrategy {
    fn name(&self) -> &str {
        "MaxYield"
    }

    async fn execute(&self, context: &StakingContext) -> StakingResult<Vec<StakingAction>> {
        let mut actions = Vec::new();

        // Find highest yield opportunities for each asset
        for (asset, balance) in &context.available_balances {
            if *balance < Decimal::from_str("10").unwrap() {
                continue; // Skip small balances
            }

            let asset_products: Vec<_> = context
                .available_products
                .iter()
                .filter(|p| &p.asset == asset)
                .cloned()
                .collect();

            if asset_products.is_empty() {
                continue;
            }

            let recommendations = self.optimizer.find_best_products(
                &asset_products,
                *balance,
                context.constraints.as_ref(),
            )?;

            for rec in recommendations {
                if rec.expected_return > Decimal::from_str("1").unwrap() {
                    actions.push(StakingAction::Stake {
                        product: rec.product,
                        amount: rec.amount,
                        priority: rec.confidence,
                        reasoning: format!("High yield opportunity: {}", rec.reasoning),
                    });
                }
            }
        }

        // Check for rebalancing opportunities
        let rebalance_actions = self.optimizer.rebalance_positions(
            &context.current_positions,
            &context.available_products,
            None,
        )?;

        for rebalance in rebalance_actions {
            match rebalance.action {
                RebalanceActionType::Move => {
                    if let Some(target) = rebalance.target_product {
                        actions.push(StakingAction::Move {
                            from_position_id: rebalance.position_id,
                            to_product: target,
                            amount: rebalance.amount,
                            priority: rebalance.priority,
                            reasoning: "Higher yield opportunity found".to_string(),
                        });
                    }
                }
                RebalanceActionType::Unstake => {
                    actions.push(StakingAction::Unstake {
                        position_id: rebalance.position_id,
                        amount: Some(rebalance.amount),
                        priority: rebalance.priority,
                        reasoning: "Risk limit violation".to_string(),
                    });
                }
                _ => {}
            }
        }

        Ok(actions)
    }

    fn should_rebalance(&self, positions: &[StakingPosition]) -> bool {
        // Rebalance if any position has significantly lower APY than available alternatives
        positions.len() > 0
            && positions.iter().any(|p| {
                p.product.apy < Decimal::from_str("0.05").unwrap() // Less than 5% APY
            })
    }

    fn risk_assessment(&self) -> RiskProfile {
        RiskProfile {
            risk_level: 85,
            max_drawdown: Decimal::from_str("0.20").unwrap(),
            diversification: DiversificationProfile {
                max_exchange_exposure: Decimal::from_str("0.50").unwrap(),
                max_asset_exposure: Decimal::from_str("0.70").unwrap(),
                min_positions: 2,
            },
            liquidity_requirements: LiquidityRequirements {
                min_flexible_percentage: Decimal::from_str("0.20").unwrap(),
                max_lock_period: Duration::days(365),
                emergency_buffer: Decimal::from_str("0.05").unwrap(),
            },
        }
    }

    fn description(&self) -> &str {
        "Aggressive strategy focused on maximizing yield with controlled risk exposure"
    }
}

/// Diversified strategy - spreads risk across exchanges and products
#[derive(Debug, Clone)]
pub struct DiversifiedStrategy {
    optimizer: YieldOptimizer,
}

impl DiversifiedStrategy {
    pub fn new() -> Self {
        let risk_settings = RiskSettings {
            max_exchange_exposure: Decimal::from_str("0.25").unwrap(),
            max_asset_exposure: Decimal::from_str("0.40").unwrap(),
            max_locked_exposure: Decimal::from_str("0.50").unwrap(),
            min_liquidity_buffer: Decimal::from_str("0.15").unwrap(),
            risk_tolerance: RiskTolerance::Moderate,
        };

        let optimization_params = OptimizationParams {
            yield_weight: Decimal::from_str("0.4").unwrap(),
            risk_weight: Decimal::from_str("0.4").unwrap(),
            liquidity_weight: Decimal::from_str("0.2").unwrap(),
            max_products: 10,
            min_allocation: Decimal::from_str("0.05").unwrap(),
        };

        Self {
            optimizer: YieldOptimizer::with_settings(risk_settings, optimization_params),
        }
    }
}

#[async_trait]
impl StakingStrategy for DiversifiedStrategy {
    fn name(&self) -> &str {
        "Diversified"
    }

    async fn execute(&self, context: &StakingContext) -> StakingResult<Vec<StakingAction>> {
        let mut actions = Vec::new();

        // Calculate target allocation across exchanges and assets
        let total_balance: Decimal = context.available_balances.values().sum();
        let exchanges: std::collections::HashSet<_> = context
            .available_products
            .iter()
            .map(|p| p.exchange)
            .collect();

        let target_per_exchange = total_balance / Decimal::from(exchanges.len());

        // Allocate across exchanges
        for exchange in exchanges {
            let exchange_products: Vec<_> = context
                .available_products
                .iter()
                .filter(|p| p.exchange == exchange)
                .cloned()
                .collect();

            if exchange_products.is_empty() {
                continue;
            }

            // Group by asset and allocate
            let mut asset_groups: HashMap<String, Vec<_>> = HashMap::new();
            for product in exchange_products {
                asset_groups
                    .entry(product.asset.clone())
                    .or_default()
                    .push(product);
            }

            let group_count = asset_groups.len();
            for (asset, products) in asset_groups {
                let available_balance = context
                    .available_balances
                    .get(&asset)
                    .copied()
                    .unwrap_or(Decimal::ZERO);
                let target_amount =
                    (target_per_exchange / Decimal::from(group_count)).min(available_balance);

                if target_amount < Decimal::from_str("10").unwrap() {
                    continue;
                }

                let recommendations = self.optimizer.find_best_products(
                    &products,
                    target_amount,
                    context.constraints.as_ref(),
                )?;

                for rec in recommendations {
                    actions.push(StakingAction::Stake {
                        product: rec.product,
                        amount: rec.amount,
                        priority: 70, // Medium priority for diversification
                        reasoning: format!("Diversification allocation: {}", rec.reasoning),
                    });
                }
            }
        }

        Ok(actions)
    }

    fn should_rebalance(&self, positions: &[StakingPosition]) -> bool {
        if positions.is_empty() {
            return false;
        }

        let total_amount: Decimal = positions.iter().map(|p| p.amount).sum();
        let exchange_counts: HashMap<_, _> = positions.iter().fold(HashMap::new(), |mut acc, p| {
            *acc.entry(p.exchange).or_insert(Decimal::ZERO) += p.amount;
            acc
        });

        // Check if any exchange has more than 25% allocation
        exchange_counts
            .values()
            .any(|&amount| amount / total_amount > Decimal::from_str("0.25").unwrap())
    }

    fn risk_assessment(&self) -> RiskProfile {
        RiskProfile {
            risk_level: 50,
            max_drawdown: Decimal::from_str("0.15").unwrap(),
            diversification: DiversificationProfile {
                max_exchange_exposure: Decimal::from_str("0.25").unwrap(),
                max_asset_exposure: Decimal::from_str("0.40").unwrap(),
                min_positions: 5,
            },
            liquidity_requirements: LiquidityRequirements {
                min_flexible_percentage: Decimal::from_str("0.30").unwrap(),
                max_lock_period: Duration::days(180),
                emergency_buffer: Decimal::from_str("0.15").unwrap(),
            },
        }
    }

    fn description(&self) -> &str {
        "Balanced strategy that prioritizes diversification across exchanges and assets"
    }
}

/// Conservative strategy - focuses on safety and capital preservation
#[derive(Debug, Clone)]
pub struct ConservativeStrategy {
    optimizer: YieldOptimizer,
}

impl ConservativeStrategy {
    pub fn new() -> Self {
        let risk_settings = RiskSettings {
            max_exchange_exposure: Decimal::from_str("0.20").unwrap(),
            max_asset_exposure: Decimal::from_str("0.30").unwrap(),
            max_locked_exposure: Decimal::from_str("0.30").unwrap(),
            min_liquidity_buffer: Decimal::from_str("0.25").unwrap(),
            risk_tolerance: RiskTolerance::Conservative,
        };

        let optimization_params = OptimizationParams {
            yield_weight: Decimal::from_str("0.2").unwrap(),
            risk_weight: Decimal::from_str("0.6").unwrap(),
            liquidity_weight: Decimal::from_str("0.2").unwrap(),
            max_products: 6,
            min_allocation: Decimal::from_str("0.10").unwrap(),
        };

        Self {
            optimizer: YieldOptimizer::with_settings(risk_settings, optimization_params),
        }
    }
}

#[async_trait]
impl StakingStrategy for ConservativeStrategy {
    fn name(&self) -> &str {
        "Conservative"
    }

    async fn execute(&self, context: &StakingContext) -> StakingResult<Vec<StakingAction>> {
        let mut actions = Vec::new();

        // Focus on major exchanges and stable products
        use jackbot_instrument::exchange::ExchangeId;
        let trusted_exchanges = [
            ExchangeId::BinanceSpot,
            ExchangeId::Coinbase,
            ExchangeId::Kraken,
        ];
        let conservative_products: Vec<_> = context
            .available_products
            .iter()
            .filter(|p| {
                trusted_exchanges.contains(&p.exchange)
                    && matches!(p.product_type, StakingType::Flexible)
                    && p.apy > Decimal::from_str("0.01").unwrap() // At least 1% APY
            })
            .cloned()
            .collect();

        for (asset, balance) in &context.available_balances {
            if *balance < Decimal::from_str("100").unwrap() {
                continue; // Higher minimum for conservative approach
            }

            let asset_products: Vec<_> = conservative_products
                .iter()
                .filter(|p| &p.asset == asset)
                .cloned()
                .collect();

            if asset_products.is_empty() {
                continue;
            }

            // Only stake 75% of available balance for liquidity
            let stake_amount = *balance * Decimal::from_str("0.75").unwrap();

            let recommendations = self.optimizer.find_best_products(
                &asset_products,
                stake_amount,
                context.constraints.as_ref(),
            )?;

            for rec in recommendations {
                if rec.risk_score >= 80 {
                    // Only high-safety products
                    actions.push(StakingAction::Stake {
                        product: rec.product,
                        amount: rec.amount,
                        priority: rec.risk_score,
                        reasoning: format!("Conservative allocation: {}", rec.reasoning),
                    });
                }
            }
        }

        Ok(actions)
    }

    fn should_rebalance(&self, positions: &[StakingPosition]) -> bool {
        // Conservative rebalancing - only if risk limits are violated
        positions.iter().any(|p| {
            !matches!(p.product.product_type, StakingType::Flexible)
                || p.product.apy < Decimal::from_str("0.005").unwrap() // Less than 0.5% APY
        })
    }

    fn risk_assessment(&self) -> RiskProfile {
        RiskProfile {
            risk_level: 25,
            max_drawdown: Decimal::from_str("0.05").unwrap(),
            diversification: DiversificationProfile {
                max_exchange_exposure: Decimal::from_str("0.20").unwrap(),
                max_asset_exposure: Decimal::from_str("0.30").unwrap(),
                min_positions: 3,
            },
            liquidity_requirements: LiquidityRequirements {
                min_flexible_percentage: Decimal::from_str("0.70").unwrap(),
                max_lock_period: Duration::days(30),
                emergency_buffer: Decimal::from_str("0.25").unwrap(),
            },
        }
    }

    fn description(&self) -> &str {
        "Conservative strategy focusing on capital preservation and high liquidity"
    }
}

/// Liquidity-first strategy - prioritizes instant access to funds
#[derive(Debug, Clone)]
pub struct LiquidityFirstStrategy;

#[async_trait]
impl StakingStrategy for LiquidityFirstStrategy {
    fn name(&self) -> &str {
        "LiquidityFirst"
    }

    async fn execute(&self, context: &StakingContext) -> StakingResult<Vec<StakingAction>> {
        let mut actions = Vec::new();

        // Only consider flexible staking products
        let flexible_products: Vec<_> = context
            .available_products
            .iter()
            .filter(|p| matches!(p.product_type, StakingType::Flexible))
            .cloned()
            .collect();

        for (asset, balance) in &context.available_balances {
            if *balance < Decimal::from_str("50").unwrap() {
                continue;
            }

            let asset_products: Vec<_> = flexible_products
                .iter()
                .filter(|p| &p.asset == asset)
                .cloned()
                .collect();

            if let Some(best_product) = asset_products.iter().max_by_key(|p| p.apy) {
                // Stake 90% of balance, keep 10% liquid
                let stake_amount = *balance * Decimal::from_str("0.90").unwrap();

                actions.push(StakingAction::Stake {
                    product: best_product.clone(),
                    amount: stake_amount,
                    priority: 60,
                    reasoning: "Best flexible staking option with instant liquidity".to_string(),
                });
            }
        }

        Ok(actions)
    }

    fn should_rebalance(&self, positions: &[StakingPosition]) -> bool {
        // Rebalance if any locked positions exist
        positions
            .iter()
            .any(|p| !matches!(p.product.product_type, StakingType::Flexible))
    }

    fn risk_assessment(&self) -> RiskProfile {
        RiskProfile {
            risk_level: 15,
            max_drawdown: Decimal::from_str("0.02").unwrap(),
            diversification: DiversificationProfile {
                max_exchange_exposure: Decimal::from_str("0.40").unwrap(),
                max_asset_exposure: Decimal::from_str("0.50").unwrap(),
                min_positions: 1,
            },
            liquidity_requirements: LiquidityRequirements {
                min_flexible_percentage: Decimal::from_str("1.0").unwrap(),
                max_lock_period: Duration::zero(),
                emergency_buffer: Decimal::from_str("0.10").unwrap(),
            },
        }
    }

    fn description(&self) -> &str {
        "Ultra-conservative strategy with 100% flexible staking for maximum liquidity"
    }
}
