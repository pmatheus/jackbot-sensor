//! Risk management integration for staking operations
//!
//! Extends the existing risk management framework to include staking-specific
//! risk controls and monitoring.

use crate::staking::{
    error::{StakingError, StakingResult},
    *,
};
use chrono::{DateTime, Duration, Utc};
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::{Decimal, prelude::FromStr};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Risk manager for staking operations
#[derive(Debug, Clone)]
pub struct StakingRiskManager {
    /// Risk limits configuration
    pub limits: StakingRiskLimits,
    /// Current risk metrics
    pub metrics: StakingRiskMetrics,
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
}

/// Comprehensive risk limits for staking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingRiskLimits {
    /// Maximum total amount that can be staked
    pub max_total_staked: Option<Decimal>,
    /// Maximum percentage of portfolio in staking
    pub max_staking_percentage: Decimal,
    /// Maximum exposure per exchange
    pub max_exchange_exposure: HashMap<ExchangeId, Decimal>,
    /// Maximum exposure per asset
    pub max_asset_exposure: HashMap<String, Decimal>,
    /// Maximum percentage in locked staking
    pub max_locked_percentage: Decimal,
    /// Maximum lock period allowed
    pub max_lock_period: Duration,
    /// Minimum liquidity buffer
    pub min_liquidity_buffer: Decimal,
    /// Maximum number of active positions
    pub max_active_positions: usize,
    /// Counterparty risk limits per exchange
    pub counterparty_limits: HashMap<ExchangeId, CounterpartyLimit>,
}

impl Default for StakingRiskLimits {
    fn default() -> Self {
        Self {
            max_total_staked: None,
            max_staking_percentage: Decimal::from_str("0.40").unwrap(), // 40% of portfolio
            max_exchange_exposure: HashMap::new(),
            max_asset_exposure: HashMap::new(),
            max_locked_percentage: Decimal::from_str("0.30").unwrap(), // 30% in locked
            max_lock_period: Duration::days(365),
            min_liquidity_buffer: Decimal::from_str("0.15").unwrap(), // 15% liquid
            max_active_positions: 20,
            counterparty_limits: HashMap::new(),
        }
    }
}

/// Counterparty risk limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterpartyLimit {
    /// Maximum amount with this counterparty
    pub max_exposure: Decimal,
    /// Credit rating (AAA, AA, A, BBB, etc.)
    pub credit_rating: String,
    /// Risk weight for concentration calculations
    pub risk_weight: Decimal,
    /// Last review date
    pub last_review: DateTime<Utc>,
}

/// Current risk metrics for staking positions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingRiskMetrics {
    /// Total staked amount
    pub total_staked: Decimal,
    /// Percentage of portfolio staked
    pub staking_percentage: Decimal,
    /// Exchange concentration
    pub exchange_concentration: HashMap<ExchangeId, Decimal>,
    /// Asset concentration
    pub asset_concentration: HashMap<String, Decimal>,
    /// Locked percentage
    pub locked_percentage: Decimal,
    /// Average lock period
    pub average_lock_period: Duration,
    /// Liquidity score (0-100)
    pub liquidity_score: f64,
    /// Counterparty exposure
    pub counterparty_exposure: HashMap<ExchangeId, Decimal>,
    /// Risk-adjusted return
    pub risk_adjusted_return: Decimal,
    /// Value at risk (1-day, 95% confidence)
    pub value_at_risk_1d: Decimal,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

impl Default for StakingRiskMetrics {
    fn default() -> Self {
        Self {
            total_staked: Decimal::ZERO,
            staking_percentage: Decimal::ZERO,
            exchange_concentration: HashMap::new(),
            asset_concentration: HashMap::new(),
            locked_percentage: Decimal::ZERO,
            average_lock_period: Duration::zero(),
            liquidity_score: 100.0,
            counterparty_exposure: HashMap::new(),
            risk_adjusted_return: Decimal::ZERO,
            value_at_risk_1d: Decimal::ZERO,
            last_updated: Utc::now(),
        }
    }
}

/// Alert thresholds for risk monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Concentration alert threshold
    pub concentration_threshold: Decimal,
    /// Liquidity alert threshold
    pub liquidity_threshold: f64,
    /// VAR alert threshold
    pub var_threshold: Decimal,
    /// Lock period warning threshold
    pub lock_period_threshold: Duration,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            concentration_threshold: Decimal::from_str("0.25").unwrap(), // 25%
            liquidity_threshold: 30.0, // Below 30 liquidity score
            var_threshold: Decimal::from_str("0.05").unwrap(), // 5% VAR
            lock_period_threshold: Duration::days(180), // 6 months
        }
    }
}

/// Risk assessment result
#[derive(Debug, Clone, PartialEq)]
pub struct RiskAssessment {
    /// Overall risk score (0-100)
    pub risk_score: u8,
    /// Risk level
    pub risk_level: RiskLevel,
    /// Specific risk factors
    pub risk_factors: Vec<RiskFactor>,
    /// Recommended actions
    pub recommendations: Vec<RiskRecommendation>,
    /// Assessment timestamp
    pub timestamp: DateTime<Utc>,
}

/// Risk levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Specific risk factors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Risk factor type
    pub factor_type: RiskFactorType,
    /// Severity (0-100)
    pub severity: u8,
    /// Description
    pub description: String,
    /// Current value
    pub current_value: String,
    /// Limit value
    pub limit_value: String,
}

/// Types of risk factors
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskFactorType {
    ConcentrationRisk,
    LiquidityRisk,
    CounterpartyRisk,
    LockPeriodRisk,
    CorrelationRisk,
    SlashingRisk,
    SmartContractRisk,
}

/// Risk-based recommendations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskRecommendation {
    /// Recommendation type
    pub recommendation_type: RecommendationType,
    /// Priority (0-100)
    pub priority: u8,
    /// Description
    pub description: String,
    /// Specific actions
    pub actions: Vec<String>,
}

/// Types of risk recommendations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationType {
    ReduceExposure,
    IncreaseFlexible,
    DiversifyExchanges,
    DiversifyAssets,
    ReduceLockPeriod,
    IncreaseBuffer,
    ReviewCounterparty,
}

impl StakingRiskManager {
    /// Create a new risk manager with default limits
    pub fn new() -> Self {
        Self {
            limits: StakingRiskLimits::default(),
            metrics: StakingRiskMetrics::default(),
            alert_thresholds: AlertThresholds::default(),
        }
    }

    /// Create with custom limits
    pub fn with_limits(limits: StakingRiskLimits) -> Self {
        Self {
            limits,
            metrics: StakingRiskMetrics::default(),
            alert_thresholds: AlertThresholds::default(),
        }
    }

    /// Check if a staking position violates risk limits
    pub fn check_position_limits(
        &self,
        position: &StakingPosition,
        current_positions: &[StakingPosition],
        total_portfolio_value: Decimal,
    ) -> StakingResult<()> {
        // Calculate metrics including the new position
        let mut test_positions = current_positions.to_vec();
        test_positions.push(position.clone());
        let test_metrics = self.calculate_metrics(&test_positions, total_portfolio_value)?;

        // Check all limits
        self.validate_metrics(&test_metrics)?;

        Ok(())
    }

    /// Calculate liquidity risk score
    pub fn calculate_liquidity_risk(&self, positions: &[StakingPosition]) -> f64 {
        if positions.is_empty() {
            return 100.0; // Perfect liquidity with no positions
        }

        let total_amount: Decimal = positions.iter().map(|p| p.amount).sum();
        let flexible_amount: Decimal = positions
            .iter()
            .filter(|p| matches!(p.product.product_type, StakingType::Flexible))
            .map(|p| p.amount)
            .sum();

        let locked_positions = positions
            .iter()
            .filter(|p| !matches!(p.product.product_type, StakingType::Flexible))
            .collect::<Vec<_>>();

        // Calculate weighted liquidity score
        let flexible_ratio = if total_amount == Decimal::ZERO {
            1.0
        } else {
            (flexible_amount / total_amount).to_f64().unwrap_or(0.0)
        };

        let mut liquidity_score = flexible_ratio * 100.0;

        // Penalize based on lock periods
        for position in locked_positions {
            if let Some(lock_period) = position.product.lock_period {
                let days = lock_period.num_days();
                let penalty = (days as f64 * 0.1).min(30.0); // Max 30 point penalty
                let weight = (position.amount / total_amount).to_f64().unwrap_or(0.0);
                liquidity_score -= penalty * weight;
            }
        }

        liquidity_score.max(0.0).min(100.0)
    }

    /// Monitor for unstaking delays
    pub fn monitor_unstaking_delays(&self, positions: &[StakingPosition]) -> Vec<Alert> {
        let mut alerts = Vec::new();
        let now = Utc::now();

        for position in positions {
            if position.status == StakingPositionStatus::Unstaking {
                let delay = now - position.last_updated;
                
                if delay > Duration::hours(24) {
                    alerts.push(Alert {
                        alert_type: AlertType::UnstakingDelay,
                        severity: if delay > Duration::days(3) {
                            AlertSeverity::High
                        } else {
                            AlertSeverity::Medium
                        },
                        message: format!(
                            "Unstaking delayed for position {} ({} hours)",
                            position.id,
                            delay.num_hours()
                        ),
                        position_id: Some(position.id.clone()),
                        timestamp: now,
                    });
                }
            }

            // Check for expired locked positions
            if let Some(end_time) = position.end_time {
                if now > end_time && position.status == StakingPositionStatus::Active {
                    alerts.push(Alert {
                        alert_type: AlertType::PositionExpired,
                        severity: AlertSeverity::Medium,
                        message: format!(
                            "Position {} has expired but is still active",
                            position.id
                        ),
                        position_id: Some(position.id.clone()),
                        timestamp: now,
                    });
                }
            }
        }

        alerts
    }

    /// Assess counterparty risk for an exchange
    pub fn assess_counterparty_risk(&self, exchange: ExchangeId, amount: Decimal) -> RiskScore {
        let limit = self.limits.counterparty_limits.get(&exchange);
        let current_exposure = self.metrics.counterparty_exposure.get(&exchange).copied().unwrap_or(Decimal::ZERO);
        let total_exposure = current_exposure + amount;

        let risk_score = if let Some(limit) = limit {
            let utilization = total_exposure / limit.max_exposure;
            let base_score = match limit.credit_rating.as_str() {
                "AAA" => 10,
                "AA" => 20,
                "A" => 30,
                "BBB" => 50,
                "BB" => 70,
                _ => 90,
            };

            let utilization_penalty = (utilization.to_f64().unwrap_or(0.0) * 50.0) as u8;
            (base_score + utilization_penalty).min(100)
        } else {
            // Unknown exchange - high risk
            80
        };

        RiskScore {
            score: risk_score,
            level: match risk_score {
                0..=25 => RiskLevel::Low,
                26..=50 => RiskLevel::Medium,
                51..=75 => RiskLevel::High,
                _ => RiskLevel::Critical,
            },
            factors: vec![
                format!("Exchange: {}", exchange),
                format!("Total exposure: {}", total_exposure),
                format!("Risk score: {}", risk_score),
            ],
        }
    }

    /// Perform comprehensive risk assessment
    pub fn assess_risk(
        &self,
        positions: &[StakingPosition],
        total_portfolio_value: Decimal,
    ) -> StakingResult<RiskAssessment> {
        let metrics = self.calculate_metrics(positions, total_portfolio_value)?;
        let mut risk_factors = Vec::new();
        let mut recommendations = Vec::new();

        // Check concentration risk
        for (exchange, concentration) in &metrics.exchange_concentration {
            if *concentration > self.alert_thresholds.concentration_threshold {
                risk_factors.push(RiskFactor {
                    factor_type: RiskFactorType::ConcentrationRisk,
                    severity: (concentration.to_f64().unwrap_or(0.0) * 100.0) as u8,
                    description: format!("High concentration in {}", exchange),
                    current_value: format!("{:.1}%", concentration * Decimal::from(100)),
                    limit_value: format!("{:.1}%", self.alert_thresholds.concentration_threshold * Decimal::from(100)),
                });

                recommendations.push(RiskRecommendation {
                    recommendation_type: RecommendationType::DiversifyExchanges,
                    priority: 80,
                    description: format!("Reduce exposure to {} below {}%", exchange, self.alert_thresholds.concentration_threshold * Decimal::from(100)),
                    actions: vec![
                        "Consider unstaking partial positions".to_string(),
                        "Diversify to other exchanges".to_string(),
                    ],
                });
            }
        }

        // Check liquidity risk
        if metrics.liquidity_score < self.alert_thresholds.liquidity_threshold {
            risk_factors.push(RiskFactor {
                factor_type: RiskFactorType::LiquidityRisk,
                severity: (100.0 - metrics.liquidity_score) as u8,
                description: "Low liquidity".to_string(),
                current_value: format!("{:.1}", metrics.liquidity_score),
                limit_value: format!("{:.1}", self.alert_thresholds.liquidity_threshold),
            });

            recommendations.push(RiskRecommendation {
                recommendation_type: RecommendationType::IncreaseFlexible,
                priority: 70,
                description: "Increase flexible staking allocation".to_string(),
                actions: vec![
                    "Move to flexible products".to_string(),
                    "Avoid new locked positions".to_string(),
                ],
            });
        }

        // Check lock period risk
        if metrics.average_lock_period > self.alert_thresholds.lock_period_threshold {
            risk_factors.push(RiskFactor {
                factor_type: RiskFactorType::LockPeriodRisk,
                severity: (metrics.average_lock_period.num_days() as f64 / 365.0 * 100.0) as u8,
                description: "Long average lock period".to_string(),
                current_value: format!("{} days", metrics.average_lock_period.num_days()),
                limit_value: format!("{} days", self.alert_thresholds.lock_period_threshold.num_days()),
            });
        }

        // Calculate overall risk score
        let risk_score = if risk_factors.is_empty() {
            20 // Low risk baseline
        } else {
            let average_severity: f64 = risk_factors.iter().map(|f| f.severity as f64).sum::<f64>() / risk_factors.len() as f64;
            average_severity as u8
        };

        let risk_level = match risk_score {
            0..=25 => RiskLevel::Low,
            26..=50 => RiskLevel::Medium,
            51..=75 => RiskLevel::High,
            _ => RiskLevel::Critical,
        };

        Ok(RiskAssessment {
            risk_score,
            risk_level,
            risk_factors,
            recommendations,
            timestamp: Utc::now(),
        })
    }

    fn calculate_metrics(
        &self,
        positions: &[StakingPosition],
        total_portfolio_value: Decimal,
    ) -> StakingResult<StakingRiskMetrics> {
        let total_staked: Decimal = positions.iter().map(|p| p.amount).sum();
        let staking_percentage = if total_portfolio_value == Decimal::ZERO {
            Decimal::ZERO
        } else {
            total_staked / total_portfolio_value
        };

        // Calculate exchange concentration
        let mut exchange_concentration = HashMap::new();
        for position in positions {
            let exposure = exchange_concentration.entry(position.exchange).or_insert(Decimal::ZERO);
            *exposure += position.amount;
        }
        for (_, exposure) in exchange_concentration.iter_mut() {
            if total_staked > Decimal::ZERO {
                *exposure /= total_staked;
            }
        }

        // Calculate asset concentration
        let mut asset_concentration = HashMap::new();
        for position in positions {
            let exposure = asset_concentration.entry(position.asset.clone()).or_insert(Decimal::ZERO);
            *exposure += position.amount;
        }
        for (_, exposure) in asset_concentration.iter_mut() {
            if total_staked > Decimal::ZERO {
                *exposure /= total_staked;
            }
        }

        // Calculate locked percentage
        let locked_amount: Decimal = positions
            .iter()
            .filter(|p| !matches!(p.product.product_type, StakingType::Flexible))
            .map(|p| p.amount)
            .sum();
        let locked_percentage = if total_staked == Decimal::ZERO {
            Decimal::ZERO
        } else {
            locked_amount / total_staked
        };

        // Calculate average lock period
        let weighted_lock_days: i64 = positions
            .iter()
            .filter_map(|p| {
                p.product.lock_period.map(|period| {
                    let weight = (p.amount / total_staked).to_f64().unwrap_or(0.0);
                    (period.num_days() as f64 * weight) as i64
                })
            })
            .sum();
        let average_lock_period = Duration::days(weighted_lock_days);

        // Calculate liquidity score
        let liquidity_score = self.calculate_liquidity_risk(positions);

        // Calculate counterparty exposure
        let mut counterparty_exposure = HashMap::new();
        for position in positions {
            let exposure = counterparty_exposure.entry(position.exchange).or_insert(Decimal::ZERO);
            *exposure += position.amount;
        }

        Ok(StakingRiskMetrics {
            total_staked,
            staking_percentage,
            exchange_concentration,
            asset_concentration,
            locked_percentage,
            average_lock_period,
            liquidity_score,
            counterparty_exposure,
            risk_adjusted_return: Decimal::ZERO, // Risk-adjusted return calculation - see STAKING_RISK_SPEC.md
            value_at_risk_1d: Decimal::ZERO, // VaR calculation - see STAKING_RISK_SPEC.md
            last_updated: Utc::now(),
        })
    }

    fn validate_metrics(&self, metrics: &StakingRiskMetrics) -> StakingResult<()> {
        // Check total staking percentage
        if metrics.staking_percentage > self.limits.max_staking_percentage {
            return Err(StakingError::RiskLimitExceeded {
                risk_type: "portfolio_staking_percentage".to_string(),
                limit: self.limits.max_staking_percentage,
                attempted: metrics.staking_percentage,
            });
        }

        // Check locked percentage
        if metrics.locked_percentage > self.limits.max_locked_percentage {
            return Err(StakingError::RiskLimitExceeded {
                risk_type: "locked_staking_percentage".to_string(),
                limit: self.limits.max_locked_percentage,
                attempted: metrics.locked_percentage,
            });
        }

        // Check exchange concentration
        for (exchange, concentration) in &metrics.exchange_concentration {
            if let Some(limit) = self.limits.max_exchange_exposure.get(exchange) {
                if concentration > limit {
                    return Err(StakingError::RiskLimitExceeded {
                        risk_type: format!("exchange_concentration_{}", exchange),
                        limit: *limit,
                        attempted: *concentration,
                    });
                }
            }
        }

        Ok(())
    }
}

/// Risk score result
#[derive(Debug, Clone, PartialEq)]
pub struct RiskScore {
    pub score: u8,
    pub level: RiskLevel,
    pub factors: Vec<String>,
}

/// Alert types for risk monitoring
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub position_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    ConcentrationRisk,
    LiquidityRisk,
    UnstakingDelay,
    PositionExpired,
    CounterpartyRisk,
    LockPeriodViolation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for StakingRiskManager {
    fn default() -> Self {
        Self::new()
    }
}