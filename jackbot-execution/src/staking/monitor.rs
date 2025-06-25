//! Real-time staking monitoring and alerting

use crate::staking::{error::StakingResult, *};
use chrono::{DateTime, Duration, Utc};
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::{Decimal, prelude::FromStr};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Real-time staking monitor
#[derive(Debug, Clone)]
pub struct StakingMonitor {
    /// Monitoring configuration
    pub config: MonitorConfig,
    /// Current metrics snapshot
    pub current_metrics: MonitoringMetrics,
    /// Alert history
    pub alert_history: Vec<MonitoringAlert>,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Monitoring interval
    pub check_interval: Duration,
    /// APY change threshold for alerts
    pub apy_change_threshold: Decimal,
    /// Position value threshold for alerts
    pub position_value_threshold: Decimal,
    /// Enable auto-unstaking alerts
    pub auto_unstaking_alerts: bool,
    /// Enable reward claiming alerts
    pub reward_claiming_alerts: bool,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::minutes(5),
            apy_change_threshold: Decimal::from_str("0.01").unwrap(), // 1% change
            position_value_threshold: Decimal::from(1000), // $1000
            auto_unstaking_alerts: true,
            reward_claiming_alerts: true,
        }
    }
}

/// Comprehensive monitoring metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringMetrics {
    /// Total staked value across all exchanges
    pub total_staked_value: Decimal,
    /// Total active positions
    pub active_positions: usize,
    /// Total pending rewards
    pub pending_rewards: Decimal,
    /// Average weighted APY
    pub average_apy: Decimal,
    /// Liquidity availability
    pub liquidity_percentage: Decimal,
    /// Exchange health scores
    pub exchange_health: HashMap<ExchangeId, ExchangeHealth>,
    /// Performance metrics
    pub performance: PerformanceSnapshot,
    /// Risk metrics
    pub risk_metrics: RiskSnapshot,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

/// Exchange health indicators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeHealth {
    /// Overall health score (0-100)
    pub health_score: u8,
    /// API response time (ms)
    pub api_response_time: u64,
    /// Last successful operation
    pub last_successful_operation: DateTime<Utc>,
    /// Number of failed operations in last 24h
    pub failed_operations_24h: u32,
    /// Staking products availability
    pub products_available: bool,
    /// Withdrawal status
    pub withdrawals_enabled: bool,
}

/// Performance snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    /// Daily yield
    pub daily_yield: Decimal,
    /// Weekly yield
    pub weekly_yield: Decimal,
    /// Monthly yield
    pub monthly_yield: Decimal,
    /// Annualized return
    pub annualized_return: Decimal,
    /// Best performing asset
    pub best_asset: Option<String>,
    /// Worst performing asset
    pub worst_asset: Option<String>,
}

/// Risk metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskSnapshot {
    /// Portfolio risk score (0-100)
    pub risk_score: u8,
    /// Maximum exchange concentration
    pub max_exchange_concentration: Decimal,
    /// Maximum asset concentration
    pub max_asset_concentration: Decimal,
    /// Locked funds percentage
    pub locked_percentage: Decimal,
    /// Time to next unlock
    pub next_unlock: Option<DateTime<Utc>>,
}

/// Monitoring alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringAlert {
    /// Alert ID
    pub id: String,
    /// Alert type
    pub alert_type: AlertType,
    /// Severity level
    pub severity: AlertSeverity,
    /// Alert message
    pub message: String,
    /// Associated exchange
    pub exchange: Option<ExchangeId>,
    /// Associated asset
    pub asset: Option<String>,
    /// Alert timestamp
    pub timestamp: DateTime<Utc>,
    /// Alert acknowledged
    pub acknowledged: bool,
}

/// Types of monitoring alerts
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    /// APY change alert
    ApyChange,
    /// Position value alert
    PositionValue,
    /// Reward claiming opportunity
    RewardClaiming,
    /// Auto-unstaking event
    AutoUnstaking,
    /// Exchange health issue
    ExchangeHealth,
    /// Risk limit breach
    RiskLimit,
    /// Portfolio rebalancing suggestion
    Rebalancing,
    /// Product availability change
    ProductAvailability,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl StakingMonitor {
    /// Create a new staking monitor
    pub fn new() -> Self {
        Self {
            config: MonitorConfig::default(),
            current_metrics: MonitoringMetrics::default(),
            alert_history: Vec::new(),
        }
    }

    /// Update monitoring metrics
    pub fn update_metrics(
        &mut self,
        positions: &[StakingPosition],
        rewards: &[StakingReward],
        products: &[StakingProduct],
    ) -> StakingResult<()> {
        // Calculate basic metrics
        let total_staked_value = positions.iter().map(|p| p.amount).sum();
        let active_positions = positions.len();
        let pending_rewards = rewards
            .iter()
            .filter(|r| r.status == StakingRewardStatus::Available)
            .map(|r| r.amount)
            .sum();

        // Calculate weighted APY
        let average_apy = if total_staked_value > Decimal::ZERO {
            positions
                .iter()
                .map(|p| p.amount * p.product.apy)
                .sum::<Decimal>() / total_staked_value
        } else {
            Decimal::ZERO
        };

        // Calculate liquidity
        let flexible_amount = positions
            .iter()
            .filter(|p| matches!(p.product.product_type, StakingType::Flexible))
            .map(|p| p.amount)
            .sum::<Decimal>();
        let liquidity_percentage = if total_staked_value > Decimal::ZERO {
            flexible_amount / total_staked_value
        } else {
            Decimal::ONE
        };

        // Update exchange health (simplified)
        let mut exchange_health = HashMap::new();
        for position in positions {
            exchange_health.entry(position.exchange).or_insert(ExchangeHealth {
                health_score: 95, // Default high score
                api_response_time: 200,
                last_successful_operation: Utc::now(),
                failed_operations_24h: 0,
                products_available: true,
                withdrawals_enabled: true,
            });
        }

        self.current_metrics = MonitoringMetrics {
            total_staked_value,
            active_positions,
            pending_rewards,
            average_apy,
            liquidity_percentage,
            exchange_health,
            performance: PerformanceSnapshot::default(),
            risk_metrics: RiskSnapshot::default(),
            last_updated: Utc::now(),
        };

        Ok(())
    }

    /// Check for alerts and generate notifications
    pub fn check_alerts(
        &mut self,
        positions: &[StakingPosition],
        previous_metrics: Option<&MonitoringMetrics>,
    ) -> Vec<MonitoringAlert> {
        let mut new_alerts = Vec::new();

        // Check APY changes
        if let Some(prev) = previous_metrics {
            let apy_change = (self.current_metrics.average_apy - prev.average_apy).abs();
            if apy_change > self.config.apy_change_threshold {
                new_alerts.push(MonitoringAlert {
                    id: format!("apy_change_{}", Utc::now().timestamp()),
                    alert_type: AlertType::ApyChange,
                    severity: if apy_change > self.config.apy_change_threshold * Decimal::from(2) {
                        AlertSeverity::Warning
                    } else {
                        AlertSeverity::Info
                    },
                    message: format!(
                        "Average APY changed by {:.2}% (from {:.2}% to {:.2}%)",
                        apy_change * Decimal::from(100),
                        prev.average_apy * Decimal::from(100),
                        self.current_metrics.average_apy * Decimal::from(100)
                    ),
                    exchange: None,
                    asset: None,
                    timestamp: Utc::now(),
                    acknowledged: false,
                });
            }
        }

        // Check for large positions
        for position in positions {
            if position.amount > self.config.position_value_threshold {
                new_alerts.push(MonitoringAlert {
                    id: format!("large_position_{}", position.id),
                    alert_type: AlertType::PositionValue,
                    severity: AlertSeverity::Info,
                    message: format!(
                        "Large position detected: {} {} on {}",
                        position.amount, position.asset, position.exchange
                    ),
                    exchange: Some(position.exchange),
                    asset: Some(position.asset.clone()),
                    timestamp: Utc::now(),
                    acknowledged: false,
                });
            }
        }

        // Check for pending rewards
        if self.config.reward_claiming_alerts && self.current_metrics.pending_rewards > Decimal::ZERO {
            new_alerts.push(MonitoringAlert {
                id: format!("pending_rewards_{}", Utc::now().timestamp()),
                alert_type: AlertType::RewardClaiming,
                severity: AlertSeverity::Info,
                message: format!(
                    "Pending rewards available: {}",
                    self.current_metrics.pending_rewards
                ),
                exchange: None,
                asset: None,
                timestamp: Utc::now(),
                acknowledged: false,
            });
        }

        // Check for expiring positions
        let now = Utc::now();
        for position in positions {
            if let Some(end_time) = position.end_time {
                let time_to_expiry = end_time - now;
                if time_to_expiry <= Duration::days(1) && time_to_expiry > Duration::zero() {
                    new_alerts.push(MonitoringAlert {
                        id: format!("expiring_position_{}", position.id),
                        alert_type: AlertType::AutoUnstaking,
                        severity: AlertSeverity::Warning,
                        message: format!(
                            "Position {} expires in {} hours",
                            position.id,
                            time_to_expiry.num_hours()
                        ),
                        exchange: Some(position.exchange),
                        asset: Some(position.asset.clone()),
                        timestamp: Utc::now(),
                        acknowledged: false,
                    });
                }
            }
        }

        // Add new alerts to history
        self.alert_history.extend(new_alerts.clone());

        new_alerts
    }

    /// Get dashboard metrics
    pub fn get_dashboard_metrics(&self) -> DashboardMetrics {
        DashboardMetrics {
            total_staked_value: self.current_metrics.total_staked_value,
            active_positions: self.current_metrics.active_positions,
            average_apy: self.current_metrics.average_apy,
            pending_rewards: self.current_metrics.pending_rewards,
            liquidity_percentage: self.current_metrics.liquidity_percentage,
            unacknowledged_alerts: self.alert_history.iter().filter(|a| !a.acknowledged).count(),
            last_updated: self.current_metrics.last_updated,
        }
    }

    /// Acknowledge alert
    pub fn acknowledge_alert(&mut self, alert_id: &str) {
        if let Some(alert) = self.alert_history.iter_mut().find(|a| a.id == alert_id) {
            alert.acknowledged = true;
        }
    }

    /// Get recent alerts
    pub fn get_recent_alerts(&self, hours: i64) -> Vec<&MonitoringAlert> {
        let cutoff = Utc::now() - Duration::hours(hours);
        self.alert_history
            .iter()
            .filter(|a| a.timestamp >= cutoff)
            .collect()
    }
}

impl Default for StakingMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for MonitoringMetrics {
    fn default() -> Self {
        Self {
            total_staked_value: Decimal::ZERO,
            active_positions: 0,
            pending_rewards: Decimal::ZERO,
            average_apy: Decimal::ZERO,
            liquidity_percentage: Decimal::ONE,
            exchange_health: HashMap::new(),
            performance: PerformanceSnapshot::default(),
            risk_metrics: RiskSnapshot::default(),
            last_updated: Utc::now(),
        }
    }
}

impl Default for PerformanceSnapshot {
    fn default() -> Self {
        Self {
            daily_yield: Decimal::ZERO,
            weekly_yield: Decimal::ZERO,
            monthly_yield: Decimal::ZERO,
            annualized_return: Decimal::ZERO,
            best_asset: None,
            worst_asset: None,
        }
    }
}

impl Default for RiskSnapshot {
    fn default() -> Self {
        Self {
            risk_score: 50,
            max_exchange_concentration: Decimal::ZERO,
            max_asset_concentration: Decimal::ZERO,
            locked_percentage: Decimal::ZERO,
            next_unlock: None,
        }
    }
}

/// Dashboard metrics for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardMetrics {
    pub total_staked_value: Decimal,
    pub active_positions: usize,
    pub average_apy: Decimal,
    pub pending_rewards: Decimal,
    pub liquidity_percentage: Decimal,
    pub unacknowledged_alerts: usize,
    pub last_updated: DateTime<Utc>,
}