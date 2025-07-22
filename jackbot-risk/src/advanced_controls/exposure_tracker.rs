use chrono::{DateTime, Duration as ChronoDuration, Utc};
use jackbot_instrument::exchange::ExchangeId;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::{HashMap, VecDeque};

/// Exposure tracking system
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of the exposure tracking system architecture
pub struct ExposureTracker {
    /// Current exposures
    current_exposures: CurrentExposures,
    /// Exposure limits
    exposure_limits: ExposureLimits,
    /// Exposure analytics
    exposure_analytics: ExposureAnalytics,
    /// Historical exposures
    exposure_history: VecDeque<(DateTime<Utc>, CurrentExposures)>,
}

#[derive(Debug, Clone)]
pub struct CurrentExposures {
    /// Gross exposure
    pub gross_exposure: Decimal,
    /// Net exposure
    pub net_exposure: Decimal,
    /// Exchange exposures
    pub exchange_exposures: HashMap<ExchangeId, Decimal>,
    /// Asset class exposures
    pub asset_exposures: HashMap<String, Decimal>,
    /// Currency exposures
    pub currency_exposures: HashMap<String, Decimal>,
}

#[derive(Debug, Clone)]
pub struct ExposureLimits {
    /// Maximum gross exposure
    pub max_gross_exposure: Decimal,
    /// Maximum net exposure
    pub max_net_exposure: Decimal,
    /// Exchange-specific limits
    pub exchange_limits: HashMap<ExchangeId, Decimal>,
    /// Asset class limits
    pub asset_limits: HashMap<String, Decimal>,
    /// Currency limits
    pub currency_limits: HashMap<String, Decimal>,
}

#[derive(Debug, Clone)]
pub struct ExposureAnalytics {
    /// Exposure utilization rates
    pub utilization_rates: HashMap<String, f64>,
    /// Exposure trends
    pub exposure_trends: HashMap<String, ExposureTrend>,
    /// Risk-adjusted exposure
    pub risk_adjusted_exposure: HashMap<ExchangeId, Decimal>,
}

#[derive(Debug, Clone)]
pub struct ExposureTrend {
    /// Trend direction
    pub direction: TrendDirection,
    /// Trend strength
    pub strength: f64,
    /// Trend duration
    pub duration: ChronoDuration,
}

#[derive(Debug, Clone)]
pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
    Volatile,
}

impl ExposureTracker {
    pub fn new(limits: ExposureLimits) -> Self {
        Self {
            current_exposures: CurrentExposures::new(),
            exposure_limits: limits,
            exposure_analytics: ExposureAnalytics::new(),
            exposure_history: VecDeque::with_capacity(1000),
        }
    }

    pub fn update_exposures(&mut self, exposures: CurrentExposures) {
        self.exposure_history.push_back((Utc::now(), exposures.clone()));
        if self.exposure_history.len() > 1000 {
            self.exposure_history.pop_front();
        }
        
        self.current_exposures = exposures;
        self.update_analytics();
    }

    pub fn check_limits(&self) -> Vec<ExposureViolation> {
        let mut violations = Vec::new();

        // Check gross exposure limit
        if self.current_exposures.gross_exposure > self.exposure_limits.max_gross_exposure {
            violations.push(ExposureViolation {
                violation_type: ExposureViolationType::GrossExposure,
                current_value: self.current_exposures.gross_exposure,
                limit_value: self.exposure_limits.max_gross_exposure,
                severity: if self.current_exposures.gross_exposure > self.exposure_limits.max_gross_exposure * Decimal::from_str_exact("1.2").unwrap() {
                    ViolationSeverity::Critical
                } else {
                    ViolationSeverity::Warning
                },
            });
        }

        // Check net exposure limit
        if self.current_exposures.net_exposure.abs() > self.exposure_limits.max_net_exposure {
            violations.push(ExposureViolation {
                violation_type: ExposureViolationType::NetExposure,
                current_value: self.current_exposures.net_exposure,
                limit_value: self.exposure_limits.max_net_exposure,
                severity: ViolationSeverity::Warning,
            });
        }

        // Check exchange-specific limits
        for (exchange, exposure) in &self.current_exposures.exchange_exposures {
            if let Some(limit) = self.exposure_limits.exchange_limits.get(exchange) {
                if exposure > limit {
                    violations.push(ExposureViolation {
                        violation_type: ExposureViolationType::ExchangeExposure(*exchange),
                        current_value: *exposure,
                        limit_value: *limit,
                        severity: ViolationSeverity::Warning,
                    });
                }
            }
        }

        violations
    }

    pub fn get_utilization_rate(&self, exposure_type: &str) -> Option<f64> {
        self.exposure_analytics.utilization_rates.get(exposure_type).copied()
    }

    pub fn get_exposure_trend(&self, exposure_type: &str) -> Option<&ExposureTrend> {
        self.exposure_analytics.exposure_trends.get(exposure_type)
    }

    pub fn get_current_exposures(&self) -> &CurrentExposures {
        &self.current_exposures
    }

    fn update_analytics(&mut self) {
        // Update utilization rates
        let gross_util = (self.current_exposures.gross_exposure / self.exposure_limits.max_gross_exposure)
            .to_f64()
            .unwrap_or(0.0);
        self.exposure_analytics.utilization_rates.insert("gross".to_string(), gross_util);

        let net_util = (self.current_exposures.net_exposure.abs() / self.exposure_limits.max_net_exposure)
            .to_f64()
            .unwrap_or(0.0);
        self.exposure_analytics.utilization_rates.insert("net".to_string(), net_util);

        // Update exchange utilization
        for (exchange, exposure) in &self.current_exposures.exchange_exposures {
            if let Some(limit) = self.exposure_limits.exchange_limits.get(exchange) {
                let util = (*exposure / *limit).to_f64().unwrap_or(0.0);
                self.exposure_analytics.utilization_rates.insert(
                    format!("exchange_{:?}", exchange),
                    util
                );
            }
        }

        // Update trends
        self.update_trends();
    }

    fn update_trends(&mut self) {
        if self.exposure_history.len() < 10 {
            return;
        }

        let recent_history: Vec<_> = self.exposure_history.iter().rev().take(20).collect();
        
        // Analyze gross exposure trend
        let gross_values: Vec<f64> = recent_history.iter()
            .map(|(_, e)| e.gross_exposure.to_f64().unwrap_or(0.0))
            .collect();
        
        if let Some(trend) = Self::calculate_trend(&gross_values) {
            self.exposure_analytics.exposure_trends.insert("gross".to_string(), trend);
        }

        // Analyze net exposure trend
        let net_values: Vec<f64> = recent_history.iter()
            .map(|(_, e)| e.net_exposure.to_f64().unwrap_or(0.0))
            .collect();
        
        if let Some(trend) = Self::calculate_trend(&net_values) {
            self.exposure_analytics.exposure_trends.insert("net".to_string(), trend);
        }
    }

    fn calculate_trend(values: &[f64]) -> Option<ExposureTrend> {
        if values.len() < 3 {
            return None;
        }

        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        let recent_mean = values.iter().rev().take(5).sum::<f64>() / 5.0f64.min(values.len() as f64);
        let older_mean = values.iter().skip(values.len().saturating_sub(10)).take(5).sum::<f64>() / 5.0;

        let direction = if (recent_mean - older_mean).abs() < std_dev * 0.1 {
            TrendDirection::Stable
        } else if recent_mean > older_mean {
            TrendDirection::Increasing
        } else {
            TrendDirection::Decreasing
        };

        let strength = ((recent_mean - older_mean).abs() / mean).min(1.0);

        Some(ExposureTrend {
            direction,
            strength,
            duration: ChronoDuration::minutes(values.len() as i64 * 5), // Assuming 5-minute intervals
        })
    }
}

impl Default for CurrentExposures {
    fn default() -> Self {
        Self {
            gross_exposure: Decimal::ZERO,
            net_exposure: Decimal::ZERO,
            exchange_exposures: HashMap::new(),
            asset_exposures: HashMap::new(),
            currency_exposures: HashMap::new(),
        }
    }
}

impl CurrentExposures {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_from_positions(&mut self, positions: &HashMap<ExchangeId, PositionData>) {
        self.gross_exposure = Decimal::ZERO;
        self.net_exposure = Decimal::ZERO;
        self.exchange_exposures.clear();

        for (exchange, position) in positions {
            let gross = position.long_value + position.short_value;
            let net = position.long_value - position.short_value;

            self.gross_exposure += gross;
            self.net_exposure += net;
            self.exchange_exposures.insert(*exchange, gross);

            // Update asset exposures
            for (asset, value) in &position.asset_breakdown {
                *self.asset_exposures.entry(asset.clone()).or_insert(Decimal::ZERO) += value;
            }

            // Update currency exposures
            for (currency, value) in &position.currency_breakdown {
                *self.currency_exposures.entry(currency.clone()).or_insert(Decimal::ZERO) += value;
            }
        }
    }
}

impl Default for ExposureAnalytics {
    fn default() -> Self {
        Self {
            utilization_rates: HashMap::new(),
            exposure_trends: HashMap::new(),
            risk_adjusted_exposure: HashMap::new(),
        }
    }
}

impl ExposureAnalytics {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub struct ExposureViolation {
    pub violation_type: ExposureViolationType,
    pub current_value: Decimal,
    pub limit_value: Decimal,
    pub severity: ViolationSeverity,
}

#[derive(Debug, Clone)]
pub enum ExposureViolationType {
    GrossExposure,
    NetExposure,
    ExchangeExposure(ExchangeId),
    AssetExposure(String),
    CurrencyExposure(String),
}

#[derive(Debug, Clone)]
pub enum ViolationSeverity {
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct PositionData {
    pub long_value: Decimal,
    pub short_value: Decimal,
    pub asset_breakdown: HashMap<String, Decimal>,
    pub currency_breakdown: HashMap<String, Decimal>,
}