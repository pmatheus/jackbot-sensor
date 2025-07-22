use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

/// Advanced risk configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedRiskConfig {
    /// Maximum position size per exchange
    pub max_position_per_exchange: Decimal,
    /// Maximum total portfolio exposure
    pub max_total_exposure: Decimal,
    /// Maximum daily loss limit
    pub max_daily_loss: Decimal,
    /// Maximum hourly loss limit
    pub max_hourly_loss: Decimal,
    /// Value at Risk (VaR) limits
    pub var_limits: VarLimits,
    /// Volatility-based position sizing
    pub volatility_based_sizing: VolatilityBasedSizing,
    /// Correlation limits
    pub correlation_limits: CorrelationLimits,
    /// Liquidity requirements
    pub liquidity_requirements: LiquidityRequirements,
    /// Dynamic hedging configuration
    pub dynamic_hedging: DynamicHedgingConfig,
    /// Stress testing parameters
    pub stress_testing: StressTestingConfig,
    /// Circuit breaker settings
    pub circuit_breaker_settings: CircuitBreakerSettings,
    /// Predictive monitoring settings
    pub predictive_monitoring: PredictiveMonitoringConfig,
}

impl Default for AdvancedRiskConfig {
    fn default() -> Self {
        Self {
            max_position_per_exchange: Decimal::from(100000),
            max_total_exposure: Decimal::from(1000000),
            max_daily_loss: Decimal::from(10000),
            max_hourly_loss: Decimal::from(2000),
            var_limits: VarLimits::default(),
            volatility_based_sizing: VolatilityBasedSizing::default(),
            correlation_limits: CorrelationLimits::default(),
            liquidity_requirements: LiquidityRequirements::default(),
            dynamic_hedging: DynamicHedgingConfig::default(),
            stress_testing: StressTestingConfig::default(),
            circuit_breaker_settings: CircuitBreakerSettings::default(),
            predictive_monitoring: PredictiveMonitoringConfig::default(),
        }
    }
}

/// Value at Risk limits and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarLimits {
    /// 1-day VaR limit (95% confidence)
    pub daily_var_95: Decimal,
    /// 1-day VaR limit (99% confidence)
    pub daily_var_99: Decimal,
    /// Intraday VaR limit
    pub intraday_var: Decimal,
    /// Expected Shortfall limit
    pub expected_shortfall_limit: Decimal,
    /// VaR model type
    pub var_model_type: VarModelType,
    /// Lookback window for VaR calculation
    pub lookback_days: u32,
}

impl Default for VarLimits {
    fn default() -> Self {
        Self {
            daily_var_95: Decimal::from(5000),
            daily_var_99: Decimal::from(8000),
            intraday_var: Decimal::from(2000),
            expected_shortfall_limit: Decimal::from(10000),
            var_model_type: VarModelType::HistoricalSimulation,
            lookback_days: 250,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VarModelType {
    HistoricalSimulation,
    ParametricNormal,
    MonteCarlo,
    ExtremeValue,
}

/// Volatility-based position sizing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilityBasedSizing {
    /// Enable dynamic position sizing based on volatility
    pub enabled: bool,
    /// Target volatility for portfolio
    pub target_portfolio_volatility: f64,
    /// Minimum position size (regardless of volatility)
    pub min_position_size: Decimal,
    /// Maximum position size multiplier
    pub max_size_multiplier: f64,
    /// Volatility lookback window
    pub volatility_window_days: u32,
    /// Rebalancing frequency
    pub rebalancing_frequency: RebalancingFrequency,
}

impl Default for VolatilityBasedSizing {
    fn default() -> Self {
        Self {
            enabled: true,
            target_portfolio_volatility: 0.15, // 15% annualized
            min_position_size: Decimal::from(1000),
            max_size_multiplier: 3.0,
            volatility_window_days: 30,
            rebalancing_frequency: RebalancingFrequency::Daily,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RebalancingFrequency {
    Continuous,
    Hourly,
    Daily,
    Weekly,
}

/// Correlation limits and monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationLimits {
    /// Maximum correlation allowed between positions
    pub max_correlation: f64,
    /// Maximum sector concentration
    pub max_sector_concentration: f64,
    /// Maximum exchange concentration
    pub max_exchange_concentration: f64,
    /// Correlation calculation window
    pub correlation_window_days: u32,
    /// Correlation monitoring frequency
    pub monitoring_frequency: Duration,
}

impl Default for CorrelationLimits {
    fn default() -> Self {
        Self {
            max_correlation: 0.7,
            max_sector_concentration: 0.4,
            max_exchange_concentration: 0.3,
            correlation_window_days: 60,
            monitoring_frequency: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Liquidity requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityRequirements {
    /// Minimum liquidity score required
    pub min_liquidity_score: f64,
    /// Maximum position size relative to daily volume
    pub max_position_vs_volume: f64,
    /// Liquidity stress test requirements
    pub stress_test_liquidity: bool,
    /// Emergency liquidation time requirement
    pub max_liquidation_time_hours: u32,
}

impl Default for LiquidityRequirements {
    fn default() -> Self {
        Self {
            min_liquidity_score: 0.6,
            max_position_vs_volume: 0.05, // 5% of daily volume
            stress_test_liquidity: true,
            max_liquidation_time_hours: 24,
        }
    }
}

/// Dynamic hedging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicHedgingConfig {
    /// Enable automatic hedging
    pub enabled: bool,
    /// Hedging threshold (portfolio delta)
    pub hedging_threshold: f64,
    /// Hedging instruments
    pub hedging_instruments: Vec<String>,
    /// Rehedging frequency
    pub rehedging_frequency: Duration,
    /// Maximum hedge ratio
    pub max_hedge_ratio: f64,
}

impl Default for DynamicHedgingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hedging_threshold: 0.1, // 10% portfolio delta
            hedging_instruments: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
            rehedging_frequency: Duration::from_secs(900), // 15 minutes
            max_hedge_ratio: 0.8,
        }
    }
}

/// Stress testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressTestingConfig {
    /// Enable continuous stress testing
    pub enabled: bool,
    /// Stress test scenarios
    pub scenarios: Vec<StressTestScenario>,
    /// Testing frequency
    pub testing_frequency: Duration,
    /// Fail threshold for stress tests
    pub fail_threshold: f64,
}

impl Default for StressTestingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scenarios: vec![
                StressTestScenario::MarketCrash { magnitude: 0.3 },
                StressTestScenario::VolatilitySpike { multiplier: 3.0 },
                StressTestScenario::LiquidityDrying { reduction: 0.8 },
            ],
            testing_frequency: Duration::from_secs(3600), // 1 hour
            fail_threshold: 0.8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StressTestScenario {
    MarketCrash { magnitude: f64 },
    VolatilitySpike { multiplier: f64 },
    LiquidityDrying { reduction: f64 },
    CorrelationBreakdown,
    ExchangeOutage { exchanges: Vec<ExchangeId> },
    FlashCrash { duration_seconds: u64 },
}

impl std::fmt::Display for StressTestScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MarketCrash { magnitude } => write!(f, "Market Crash ({}%)", magnitude * 100.0),
            Self::VolatilitySpike { multiplier } => {
                write!(f, "Volatility Spike ({}x)", multiplier)
            }
            Self::LiquidityDrying { reduction } => {
                write!(f, "Liquidity Drying ({}% reduction)", reduction * 100.0)
            }
            Self::CorrelationBreakdown => write!(f, "Correlation Breakdown"),
            Self::ExchangeOutage { exchanges } => {
                write!(f, "Exchange Outage ({:?})", exchanges)
            }
            Self::FlashCrash { duration_seconds } => {
                write!(f, "Flash Crash ({} seconds)", duration_seconds)
            }
        }
    }
}

/// Circuit breaker settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerSettings {
    /// Enable circuit breakers
    pub enabled: bool,
    /// Daily loss circuit breaker
    pub daily_loss_breaker: CircuitBreakerRule,
    /// Hourly loss circuit breaker
    pub hourly_loss_breaker: CircuitBreakerRule,
    /// VaR breach circuit breaker
    pub var_breach_breaker: CircuitBreakerRule,
    /// Volatility spike circuit breaker
    pub volatility_spike_breaker: CircuitBreakerRule,
    /// Correlation breakdown circuit breaker
    pub correlation_breakdown_breaker: CircuitBreakerRule,
}

impl Default for CircuitBreakerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            daily_loss_breaker: CircuitBreakerRule {
                threshold: 0.8, // 80% of daily limit
                action: CircuitBreakerAction::ReducePositions,
                recovery_time: Duration::from_secs(1800), // 30 minutes
            },
            hourly_loss_breaker: CircuitBreakerRule {
                threshold: 0.9, // 90% of hourly limit
                action: CircuitBreakerAction::HaltTrading,
                recovery_time: Duration::from_secs(900), // 15 minutes
            },
            var_breach_breaker: CircuitBreakerRule {
                threshold: 1.2, // 120% of VaR limit
                action: CircuitBreakerAction::ReducePositions,
                recovery_time: Duration::from_secs(3600), // 1 hour
            },
            volatility_spike_breaker: CircuitBreakerRule {
                threshold: 2.0, // 200% of normal volatility
                action: CircuitBreakerAction::ReducePositions,
                recovery_time: Duration::from_secs(1800),
            },
            correlation_breakdown_breaker: CircuitBreakerRule {
                threshold: 0.9, // 90% correlation spike
                action: CircuitBreakerAction::HaltTrading,
                recovery_time: Duration::from_secs(3600),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerRule {
    pub threshold: f64,
    pub action: CircuitBreakerAction,
    pub recovery_time: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CircuitBreakerAction {
    HaltTrading,
    ReducePositions,
    EmergencyLiquidation,
    AlertOnly,
}

/// Predictive monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictiveMonitoringConfig {
    /// Enable predictive risk monitoring
    pub enabled: bool,
    /// Prediction horizon
    pub prediction_horizon: Duration,
    /// Model update frequency
    pub model_update_frequency: Duration,
    /// Minimum model accuracy threshold
    pub min_model_accuracy: f64,
    /// Alert threshold for predictions
    pub prediction_alert_threshold: f64,
}

impl Default for PredictiveMonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prediction_horizon: Duration::from_secs(3600), // 1 hour ahead
            model_update_frequency: Duration::from_secs(900), // 15 minutes
            min_model_accuracy: 0.7,
            prediction_alert_threshold: 0.8,
        }
    }
}