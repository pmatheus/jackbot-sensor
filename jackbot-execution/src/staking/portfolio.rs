//! Portfolio integration for staking operations
//!
//! Extends portfolio tracking to include staking positions and provides
//! comprehensive analysis of staking performance within the overall portfolio.

use crate::staking::{
    error::{StakingError, StakingResult},
    *,
};
use chrono::{DateTime, Duration, Utc};
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::{Decimal, prelude::FromStr};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Portfolio manager for staking operations
#[derive(Debug, Clone)]
pub struct StakingPortfolioManager {
    /// Current staking positions
    positions: Vec<StakingPosition>,
    /// Reward history
    reward_history: Vec<StakingReward>,
    /// Performance metrics
    performance_metrics: PerformanceMetrics,
    /// Last update timestamp
    last_updated: DateTime<Utc>,
}

/// Performance metrics for staking portfolio
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Total staked value in base currency
    pub total_staked_value: Decimal,
    /// Total accumulated rewards
    pub total_accumulated_rewards: Decimal,
    /// Available rewards for claiming
    pub available_rewards: Decimal,
    /// Annualized return percentage
    pub annualized_return: Decimal,
    /// Sharpe ratio for staking
    pub sharpe_ratio: Option<Decimal>,
    /// Maximum drawdown
    pub max_drawdown: Decimal,
    /// Average APY across positions
    pub average_apy: Decimal,
    /// Yield contribution to total portfolio
    pub portfolio_yield_contribution: Decimal,
    /// Performance by exchange
    pub exchange_performance: HashMap<ExchangeId, ExchangePerformance>,
    /// Performance by asset
    pub asset_performance: HashMap<String, AssetPerformance>,
    /// Historical performance
    pub historical_performance: Vec<DailyPerformance>,
}

/// Performance metrics per exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangePerformance {
    /// Total staked on this exchange
    pub total_staked: Decimal,
    /// Total rewards earned
    pub total_rewards: Decimal,
    /// Average APY
    pub average_apy: Decimal,
    /// Number of active positions
    pub active_positions: usize,
    /// Reliability score (0-100)
    pub reliability_score: f64,
    /// Last reward distribution
    pub last_reward_distribution: Option<DateTime<Utc>>,
}

/// Performance metrics per asset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetPerformance {
    /// Total staked of this asset
    pub total_staked: Decimal,
    /// Total rewards earned
    pub total_rewards: Decimal,
    /// Best APY achieved
    pub best_apy: Decimal,
    /// Current weighted APY
    pub current_apy: Decimal,
    /// Number of exchanges used
    pub exchanges_used: usize,
    /// Diversification score (0-100)
    pub diversification_score: f64,
}

/// Daily performance snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyPerformance {
    /// Date of the snapshot
    pub date: DateTime<Utc>,
    /// Total portfolio value including staking
    pub total_value: Decimal,
    /// Daily yield from staking
    pub daily_yield: Decimal,
    /// Cumulative yield to date
    pub cumulative_yield: Decimal,
    /// Number of active positions
    pub active_positions: usize,
    /// Average APY on this date
    pub average_apy: Decimal,
}

/// Liquidity timeline event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityEvent {
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Event type
    pub event_type: LiquidityEventType,
    /// Asset involved
    pub asset: String,
    /// Amount becoming available
    pub amount: Decimal,
    /// Associated position
    pub position_id: String,
}

/// Types of liquidity events
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiquidityEventType {
    /// Locked position expires
    LockExpiry,
    /// Manual unstaking completion
    UnstakeCompletion,
    /// Reward distribution
    RewardDistribution,
    /// Auto-renewal
    AutoRenewal,
}

/// Portfolio optimization result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    /// Current allocation efficiency (0-100)
    pub current_efficiency: f64,
    /// Optimized allocation recommendations
    pub recommended_allocations: Vec<AllocationRecommendation>,
    /// Expected improvement in yield
    pub expected_yield_improvement: Decimal,
    /// Risk impact assessment
    pub risk_impact: RiskImpact,
    /// Implementation timeline
    pub implementation_timeline: Vec<ImplementationStep>,
}

/// Allocation recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationRecommendation {
    /// Target product
    pub product: StakingProduct,
    /// Recommended amount
    pub amount: Decimal,
    /// Expected APY
    pub expected_apy: Decimal,
    /// Confidence level (0-100)
    pub confidence: u8,
    /// Priority (0-100)
    pub priority: u8,
    /// Implementation complexity
    pub complexity: ImplementationComplexity,
}

/// Implementation complexity levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImplementationComplexity {
    Simple,   // Direct staking
    Moderate, // Requires unstaking first
    Complex,  // Multiple steps required
}

/// Risk impact assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskImpact {
    /// Change in portfolio risk score
    pub risk_score_change: i8,
    /// Liquidity impact
    pub liquidity_impact: LiquidityImpact,
    /// Concentration impact
    pub concentration_impact: ConcentrationImpact,
}

/// Liquidity impact assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityImpact {
    /// Change in liquid percentage
    pub liquidity_change: Decimal,
    /// New average lock period
    pub new_average_lock_period: Duration,
    /// Emergency liquidity availability
    pub emergency_liquidity: Decimal,
}

/// Concentration impact assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcentrationImpact {
    /// New exchange concentration levels
    pub exchange_concentration: HashMap<ExchangeId, Decimal>,
    /// New asset concentration levels
    pub asset_concentration: HashMap<String, Decimal>,
    /// Diversification score change
    pub diversification_change: f64,
}

/// Implementation step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationStep {
    /// Step sequence number
    pub step: usize,
    /// Description of the action
    pub description: String,
    /// Estimated time to complete
    pub estimated_time: Duration,
    /// Dependencies on other steps
    pub dependencies: Vec<usize>,
    /// Risk level of this step
    pub risk_level: u8,
}

impl StakingPortfolioManager {
    /// Create a new portfolio manager
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            reward_history: Vec::new(),
            performance_metrics: PerformanceMetrics::default(),
            last_updated: Utc::now(),
        }
    }

    /// Update positions and recalculate metrics
    pub fn update_positions(&mut self, positions: Vec<StakingPosition>) -> StakingResult<()> {
        self.positions = positions;
        self.calculate_performance_metrics()?;
        self.last_updated = Utc::now();
        Ok(())
    }

    /// Add reward history
    pub fn add_reward_history(&mut self, rewards: Vec<StakingReward>) {
        self.reward_history.extend(rewards);
        self.reward_history.sort_by_key(|r| r.earned_time);
    }

    /// Get total staked value across all exchanges
    pub fn get_total_staked_value(&self) -> Decimal {
        self.positions.iter().map(|p| p.amount).sum()
    }

    /// Get staking yield contribution to total returns
    pub fn get_staking_yield_contribution(&self) -> Decimal {
        self.performance_metrics.portfolio_yield_contribution
    }

    /// Get liquidity timeline for next 12 months
    pub fn get_liquidity_timeline(&self) -> Vec<LiquidityEvent> {
        let mut events = Vec::new();
        let now = Utc::now();
        let end_time = now + Duration::days(365);

        for position in &self.positions {
            // Add lock expiry events
            if let Some(end_time_pos) = position.end_time {
                if end_time_pos > now && end_time_pos <= end_time {
                    events.push(LiquidityEvent {
                        timestamp: end_time_pos,
                        event_type: LiquidityEventType::LockExpiry,
                        asset: position.asset.clone(),
                        amount: position.amount,
                        position_id: position.id.clone(),
                    });
                }
            }
        }

        // Add projected reward distributions (monthly estimate)
        let mut current_date = now;
        while current_date <= end_time {
            current_date += Duration::days(30);
            
            for position in &self.positions {
                if position.status == StakingPositionStatus::Active {
                    let monthly_reward = position.amount * position.product.apy / Decimal::from(12);
                    if monthly_reward > Decimal::ZERO {
                        events.push(LiquidityEvent {
                            timestamp: current_date,
                            event_type: LiquidityEventType::RewardDistribution,
                            asset: position.asset.clone(),
                            amount: monthly_reward,
                            position_id: position.id.clone(),
                        });
                    }
                }
            }
        }

        events.sort_by_key(|e| e.timestamp);
        events
    }

    /// Optimize portfolio with staking considerations
    pub fn optimize_portfolio_with_staking(
        &self,
        available_products: &[StakingProduct],
        total_portfolio_value: Decimal,
        target_staking_percentage: Option<Decimal>,
    ) -> StakingResult<OptimizationResult> {
        let current_efficiency = self.calculate_allocation_efficiency();
        let target_percentage = target_staking_percentage.unwrap_or(Decimal::from_str("0.25").unwrap());
        let target_staking_value = total_portfolio_value * target_percentage;

        // Generate optimization recommendations
        let recommended_allocations = self.generate_allocation_recommendations(
            available_products,
            target_staking_value,
        )?;

        // Calculate expected improvements
        let expected_yield_improvement = self.calculate_yield_improvement(&recommended_allocations)?;

        // Assess risk impact
        let risk_impact = self.assess_risk_impact(&recommended_allocations)?;

        // Generate implementation timeline
        let implementation_timeline = self.generate_implementation_timeline(&recommended_allocations)?;

        Ok(OptimizationResult {
            current_efficiency,
            recommended_allocations,
            expected_yield_improvement,
            risk_impact,
            implementation_timeline,
        })
    }

    /// Get performance summary by time period
    pub fn get_performance_summary(&self, period: Duration) -> PerformanceSummary {
        let cutoff_time = Utc::now() - period;
        
        let period_rewards: Decimal = self.reward_history
            .iter()
            .filter(|r| r.earned_time >= cutoff_time)
            .map(|r| r.amount)
            .sum();

        let period_positions = self.positions
            .iter()
            .filter(|p| p.start_time >= cutoff_time)
            .count();

        PerformanceSummary {
            period,
            total_rewards: period_rewards,
            new_positions: period_positions,
            average_apy: self.performance_metrics.average_apy,
            best_performing_exchange: self.get_best_performing_exchange(),
            best_performing_asset: self.get_best_performing_asset(),
        }
    }

    fn calculate_performance_metrics(&mut self) -> StakingResult<()> {
        let total_staked_value = self.get_total_staked_value();
        let total_accumulated_rewards: Decimal = self.positions.iter().map(|p| p.accumulated_rewards).sum();
        
        let available_rewards: Decimal = self.reward_history
            .iter()
            .filter(|r| r.status == StakingRewardStatus::Available)
            .map(|r| r.amount)
            .sum();

        // Calculate weighted average APY
        let weighted_apy = if total_staked_value > Decimal::ZERO {
            self.positions
                .iter()
                .map(|p| p.amount * p.product.apy)
                .sum::<Decimal>() / total_staked_value
        } else {
            Decimal::ZERO
        };

        // Calculate exchange performance
        let mut exchange_performance = HashMap::new();
        for position in &self.positions {
            let perf = exchange_performance.entry(position.exchange).or_insert(ExchangePerformance {
                total_staked: Decimal::ZERO,
                total_rewards: Decimal::ZERO,
                average_apy: Decimal::ZERO,
                active_positions: 0,
                reliability_score: 100.0,
                last_reward_distribution: None,
            });

            perf.total_staked += position.amount;
            perf.total_rewards += position.accumulated_rewards;
            perf.active_positions += 1;
        }

        // Calculate asset performance
        let mut asset_performance = HashMap::new();
        for position in &self.positions {
            let perf = asset_performance.entry(position.asset.clone()).or_insert(AssetPerformance {
                total_staked: Decimal::ZERO,
                total_rewards: Decimal::ZERO,
                best_apy: Decimal::ZERO,
                current_apy: Decimal::ZERO,
                exchanges_used: 0,
                diversification_score: 0.0,
            });

            perf.total_staked += position.amount;
            perf.total_rewards += position.accumulated_rewards;
            perf.best_apy = perf.best_apy.max(position.product.apy);
        }

        self.performance_metrics = PerformanceMetrics {
            total_staked_value,
            total_accumulated_rewards,
            available_rewards,
            annualized_return: self.calculate_annualized_return()?,
            sharpe_ratio: self.calculate_sharpe_ratio(),
            max_drawdown: self.calculate_max_drawdown(),
            average_apy: weighted_apy,
            portfolio_yield_contribution: self.calculate_portfolio_yield_contribution(total_staked_value),
            exchange_performance,
            asset_performance,
            historical_performance: self.get_historical_performance(),
        };

        Ok(())
    }

    fn calculate_annualized_return(&self) -> StakingResult<Decimal> {
        if self.positions.is_empty() {
            return Ok(Decimal::ZERO);
        }

        let total_staked = self.get_total_staked_value();
        let total_rewards: Decimal = self.positions.iter().map(|p| p.accumulated_rewards).sum();
        
        if total_staked == Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }

        // Simple calculation - can be enhanced with time-weighted returns
        let return_rate = total_rewards / total_staked;
        Ok(return_rate)
    }

    fn calculate_sharpe_ratio(&self) -> Option<Decimal> {
        // Sharpe ratio = (Return - Risk Free Rate) / Standard Deviation
        // Using 2% as risk-free rate for staking calculations
        let risk_free_rate = Decimal::from_str("0.02").ok()?;
        
        if self.positions.len() < 2 {
            return None; // Need multiple positions for meaningful calculation
        }
        
        // Calculate average return
        let avg_return = self.calculate_annualized_return().ok()?;
        
        // Calculate standard deviation of returns
        let returns: Vec<Decimal> = self.positions.iter()
            .map(|p| p.product.apy / Decimal::from(100))
            .collect();
        
        let mean = returns.iter().sum::<Decimal>() / Decimal::from(returns.len());
        let variance = returns.iter()
            .map(|r| (r - mean) * (r - mean))
            .sum::<Decimal>() / Decimal::from(returns.len());
        
        // Simple approximation of square root for standard deviation
        let std_dev = variance.sqrt();
        
        if std_dev == Decimal::ZERO {
            return None;
        }
        
        Some((avg_return - risk_free_rate) / std_dev)
    }
    
    fn calculate_max_drawdown(&self) -> Decimal {
        if self.historical_performance.is_empty() {
            return Decimal::ZERO;
        }
        
        let mut max_drawdown = Decimal::ZERO;
        let mut peak_value = Decimal::ZERO;
        
        for snapshot in &self.historical_performance {
            if snapshot.total_value > peak_value {
                peak_value = snapshot.total_value;
            }
            
            if peak_value > Decimal::ZERO {
                let drawdown = (peak_value - snapshot.total_value) / peak_value;
                if drawdown > max_drawdown {
                    max_drawdown = drawdown;
                }
            }
        }
        
        max_drawdown
    }
    
    fn calculate_portfolio_yield_contribution(&self, total_staked_value: Decimal) -> Decimal {
        // This would need access to total portfolio value from external source
        // For now, return a placeholder calculation based on staking value
        if total_staked_value == Decimal::ZERO {
            return Decimal::ZERO;
        }
        
        // Assuming staking represents a portion of total portfolio
        // This would be calculated properly with full portfolio context
        let estimated_total_portfolio = total_staked_value * Decimal::from(10); // Placeholder
        let weighted_apy = self.performance_metrics.average_apy / Decimal::from(100);
        
        (total_staked_value * weighted_apy) / estimated_total_portfolio
    }
    
    fn get_historical_performance(&self) -> Vec<DailyPerformance> {
        // Return existing historical data or empty vec
        // In production, this would be populated from database/storage
        self.performance_metrics.historical_performance.clone()
    }
    
    fn calculate_allocation_efficiency(&self) -> f64 {
        if self.positions.is_empty() {
            return 0.0;
        }

        // Calculate efficiency based on APY distribution and diversification
        let total_value = self.get_total_staked_value();
        let weighted_apy = self.performance_metrics.average_apy.to_f64().unwrap_or(0.0);
        
        // Efficiency based on APY (max 70 points)
        let apy_score = (weighted_apy * 1000.0).min(70.0);
        
        // Efficiency based on diversification (max 30 points)
        let unique_exchanges = self.positions.iter().map(|p| p.exchange).collect::<std::collections::HashSet<_>>().len();
        let diversification_score = (unique_exchanges as f64 * 5.0).min(30.0);
        
        apy_score + diversification_score
    }

    fn generate_allocation_recommendations(
        &self,
        available_products: &[StakingProduct],
        target_value: Decimal,
    ) -> StakingResult<Vec<AllocationRecommendation>> {
        let mut recommendations = Vec::new();
        
        // Simple allocation strategy - select top APY products with diversification
        let mut sorted_products = available_products.to_vec();
        sorted_products.sort_by(|a, b| b.apy.cmp(&a.apy));
        
        let num_allocations = 5.min(sorted_products.len());
        if num_allocations == 0 {
            return Ok(recommendations);
        }
        
        let amount_per_allocation = target_value / Decimal::from(num_allocations);
        
        for (i, product) in sorted_products.iter().take(num_allocations).enumerate() {
            let complexity = if self.positions.iter().any(|p| p.asset == product.asset) {
                ImplementationComplexity::Moderate
            } else {
                ImplementationComplexity::Simple
            };
            
            recommendations.push(AllocationRecommendation {
                product: product.clone(),
                amount: amount_per_allocation,
                expected_apy: product.apy,
                confidence: 85 - (i * 5) as u8, // Decreasing confidence
                priority: 90 - (i * 10) as u8, // Decreasing priority
                complexity,
            });
        }
        
        Ok(recommendations)
    }

    fn calculate_yield_improvement(&self, recommendations: &[AllocationRecommendation]) -> StakingResult<Decimal> {
        let current_yield = self.performance_metrics.average_apy;
        let recommended_yield = if recommendations.is_empty() {
            Decimal::ZERO
        } else {
            let total_amount: Decimal = recommendations.iter().map(|r| r.amount).sum();
            let weighted_yield: Decimal = recommendations
                .iter()
                .map(|r| r.amount * r.expected_apy)
                .sum::<Decimal>() / total_amount;
            weighted_yield
        };
        
        Ok(recommended_yield - current_yield)
    }

    fn assess_risk_impact(&self, _recommendations: &[AllocationRecommendation]) -> StakingResult<RiskImpact> {
        // Simplified risk impact assessment
        Ok(RiskImpact {
            risk_score_change: 0, // No change
            liquidity_impact: LiquidityImpact {
                liquidity_change: Decimal::ZERO,
                new_average_lock_period: Duration::days(90),
                emergency_liquidity: Decimal::from_str("0.15").unwrap(),
            },
            concentration_impact: ConcentrationImpact {
                exchange_concentration: HashMap::new(),
                asset_concentration: HashMap::new(),
                diversification_change: 0.0,
            },
        })
    }

    fn generate_implementation_timeline(&self, recommendations: &[AllocationRecommendation]) -> StakingResult<Vec<ImplementationStep>> {
        let mut steps = Vec::new();
        
        for (i, recommendation) in recommendations.iter().enumerate() {
            let step_time = match recommendation.complexity {
                ImplementationComplexity::Simple => Duration::minutes(5),
                ImplementationComplexity::Moderate => Duration::hours(1),
                ImplementationComplexity::Complex => Duration::hours(24),
            };
            
            steps.push(ImplementationStep {
                step: i + 1,
                description: format!("Stake {} {} in {}", recommendation.amount, recommendation.product.asset, recommendation.product.exchange),
                estimated_time: step_time,
                dependencies: if i == 0 { Vec::new() } else { vec![i] },
                risk_level: match recommendation.complexity {
                    ImplementationComplexity::Simple => 10,
                    ImplementationComplexity::Moderate => 30,
                    ImplementationComplexity::Complex => 60,
                },
            });
        }
        
        Ok(steps)
    }

    fn get_best_performing_exchange(&self) -> Option<ExchangeId> {
        self.performance_metrics
            .exchange_performance
            .iter()
            .max_by_key(|(_, perf)| perf.average_apy)
            .map(|(exchange, _)| *exchange)
    }

    fn get_best_performing_asset(&self) -> Option<String> {
        self.performance_metrics
            .asset_performance
            .iter()
            .max_by_key(|(_, perf)| perf.current_apy)
            .map(|(asset, _)| asset.clone())
    }
}

impl Default for StakingPortfolioManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_staked_value: Decimal::ZERO,
            total_accumulated_rewards: Decimal::ZERO,
            available_rewards: Decimal::ZERO,
            annualized_return: Decimal::ZERO,
            sharpe_ratio: None,
            max_drawdown: Decimal::ZERO,
            average_apy: Decimal::ZERO,
            portfolio_yield_contribution: Decimal::ZERO,
            exchange_performance: HashMap::new(),
            asset_performance: HashMap::new(),
            historical_performance: Vec::new(),
        }
    }
}

/// Performance summary for a specific time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub period: Duration,
    pub total_rewards: Decimal,
    pub new_positions: usize,
    pub average_apy: Decimal,
    pub best_performing_exchange: Option<ExchangeId>,
    pub best_performing_asset: Option<String>,
}