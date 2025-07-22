#![allow(dead_code)]

pub mod advanced_controls;
pub mod alert;
pub mod correlation;
pub mod drawdown;
pub mod exposure;
pub mod position_tracker;
pub mod volatility;

// Re-export commonly used types
pub use alert::{RiskAlertHook, RiskViolation as RiskAlert};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Risk level enumeration
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Comprehensive risk metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetrics {
    /// Value at Risk (95% confidence)
    pub var_95: Decimal,
    /// Value at Risk (99% confidence)
    pub var_99: Decimal,
    /// Expected Shortfall
    pub expected_shortfall: Decimal,
    /// Maximum drawdown
    pub max_drawdown: Decimal,
    /// Current exposure
    pub current_exposure: Decimal,
    /// Risk level
    pub risk_level: RiskLevel,
    /// Portfolio volatility
    pub portfolio_volatility: Decimal,
    /// Sharpe ratio
    pub sharpe_ratio: Decimal,
    /// Additional fields for advanced risk controller
    pub value_at_risk: Decimal,
    pub total_exposure: Decimal,
    pub current_pnl: Decimal,
    pub liquidity_score: f64,
}

impl Default for RiskMetrics {
    fn default() -> Self {
        Self {
            var_95: Decimal::ZERO,
            var_99: Decimal::ZERO,
            expected_shortfall: Decimal::ZERO,
            max_drawdown: Decimal::ZERO,
            current_exposure: Decimal::ZERO,
            risk_level: RiskLevel::Low,
            portfolio_volatility: Decimal::ZERO,
            sharpe_ratio: Decimal::ZERO,
            value_at_risk: Decimal::ZERO,
            total_exposure: Decimal::ZERO,
            current_pnl: Decimal::ZERO,
            liquidity_score: 1.0,
        }
    }
}
