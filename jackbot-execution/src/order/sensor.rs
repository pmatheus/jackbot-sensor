use crate::{error::UnindexedOrderError, order::OrderKind};
use chrono::{DateTime, Utc};
use jackbot_data::books::aggregator::OrderBookAggregator;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for sensor-specific order execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorOrderConfig {
    /// Maximum execution time before order expires (for performance targets)
    pub max_execution_time: Duration,
    /// Enable multi-exchange routing
    pub enable_cross_exchange: bool,
    /// Risk management thresholds
    pub risk_limits: RiskLimits,
    /// Performance monitoring settings
    pub performance_monitoring: bool,
}

impl Default for SensorOrderConfig {
    fn default() -> Self {
        Self {
            max_execution_time: Duration::from_millis(500), // <500ms target
            enable_cross_exchange: true,
            risk_limits: RiskLimits::default(),
            performance_monitoring: true,
        }
    }
}

/// Risk management limits for sensor orders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    /// Maximum order value per instrument
    pub max_order_value: Decimal,
    /// Maximum position exposure per exchange
    pub max_position_exposure: Decimal,
    /// Maximum daily volume per strategy
    pub max_daily_volume: Decimal,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_order_value: Decimal::from(100000),       // $100k default
            max_position_exposure: Decimal::from(500000), // $500k default
            max_daily_volume: Decimal::from(1000000),     // $1M default
        }
    }
}

/// Jackpot order configuration with probability-based execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JackpotOrderParams {
    /// Base probability of execution (0.0 to 1.0)
    pub base_probability: f64,
    /// Market volatility multiplier for probability adjustment
    pub volatility_multiplier: f64,
    /// Liquidity threshold for execution
    pub liquidity_threshold: Decimal,
    /// Maximum price slippage tolerance
    pub max_slippage: Decimal,
    /// Time window for jackpot execution
    pub execution_window: Duration,
}

impl Default for JackpotOrderParams {
    fn default() -> Self {
        Self {
            base_probability: 0.7, // 70% base probability
            volatility_multiplier: 1.2,
            liquidity_threshold: Decimal::from(10000),
            max_slippage: Decimal::from_f64_retain(0.005).unwrap(), // 0.5%
            execution_window: Duration::from_secs(30),
        }
    }
}

/// Prophetic order configuration with predictive analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropheticOrderParams {
    /// Prediction confidence threshold (0.0 to 1.0)
    pub confidence_threshold: f64,
    /// Time horizon for price prediction
    pub prediction_horizon: Duration,
    /// Model accuracy weight in decision making
    pub model_weight: f64,
    /// Technical indicator weights
    pub indicator_weights: HashMap<String, f64>,
    /// Maximum position size based on prediction confidence
    pub max_position_ratio: f64,
}

impl Default for PropheticOrderParams {
    fn default() -> Self {
        let mut indicator_weights = HashMap::new();
        indicator_weights.insert("rsi".to_string(), 0.3);
        indicator_weights.insert("macd".to_string(), 0.25);
        indicator_weights.insert("bollinger".to_string(), 0.2);
        indicator_weights.insert("volume".to_string(), 0.25);

        Self {
            confidence_threshold: 0.75,                   // 75% confidence required
            prediction_horizon: Duration::from_secs(300), // 5 minutes
            model_weight: 0.6,
            indicator_weights,
            max_position_ratio: 0.8, // 80% of available capital
        }
    }
}

/// Event-triggered order configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTriggeredParams {
    /// Event types to monitor
    pub trigger_events: Vec<EventType>,
    /// Time window after event for order execution
    pub execution_delay: Duration,
    /// Maximum time to wait for event trigger
    pub max_wait_time: Duration,
    /// Event correlation threshold
    pub correlation_threshold: f64,
}

impl Default for EventTriggeredParams {
    fn default() -> Self {
        Self {
            trigger_events: vec![
                EventType::PriceMove {
                    threshold: Decimal::from(100),
                },
                EventType::VolumeSpike { multiplier: 2.0 },
            ],
            execution_delay: Duration::from_secs(5),
            max_wait_time: Duration::from_secs(300), // 5 minutes
            correlation_threshold: 0.8,
        }
    }
}

/// Types of events that can trigger orders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    /// Price movement beyond threshold
    PriceMove { threshold: Decimal },
    /// Volume spike detection
    VolumeSpike { multiplier: f64 },
    /// News sentiment change
    NewssentimentChange { score_delta: f64 },
    /// Cross-exchange arbitrage opportunity
    ArbitrageOpportunity { min_spread: Decimal },
    /// Order book imbalance
    BookImbalance { ratio_threshold: f64 },
    /// Strategy signal
    StrategySignal { signal_id: String },
}

impl PartialEq for EventType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::PriceMove {
                    threshold: l_threshold,
                },
                Self::PriceMove {
                    threshold: r_threshold,
                },
            ) => l_threshold == r_threshold,
            (
                Self::VolumeSpike {
                    multiplier: l_multiplier,
                },
                Self::VolumeSpike {
                    multiplier: r_multiplier,
                },
            ) => (l_multiplier - r_multiplier).abs() < f64::EPSILON,
            (
                Self::NewssentimentChange {
                    score_delta: l_score_delta,
                },
                Self::NewssentimentChange {
                    score_delta: r_score_delta,
                },
            ) => (l_score_delta - r_score_delta).abs() < f64::EPSILON,
            (
                Self::ArbitrageOpportunity {
                    min_spread: l_min_spread,
                },
                Self::ArbitrageOpportunity {
                    min_spread: r_min_spread,
                },
            ) => l_min_spread == r_min_spread,
            (
                Self::BookImbalance {
                    ratio_threshold: l_ratio_threshold,
                },
                Self::BookImbalance {
                    ratio_threshold: r_ratio_threshold,
                },
            ) => (l_ratio_threshold - r_ratio_threshold).abs() < f64::EPSILON,
            (
                Self::StrategySignal {
                    signal_id: l_signal_id,
                },
                Self::StrategySignal {
                    signal_id: r_signal_id,
                },
            ) => l_signal_id == r_signal_id,
            _ => false,
        }
    }
}

impl Eq for EventType {}

impl std::hash::Hash for EventType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Self::PriceMove { threshold } => threshold.hash(state),
            Self::VolumeSpike { multiplier } => multiplier.to_bits().hash(state),
            Self::NewssentimentChange { score_delta } => score_delta.to_bits().hash(state),
            Self::ArbitrageOpportunity { min_spread } => min_spread.hash(state),
            Self::BookImbalance { ratio_threshold } => ratio_threshold.to_bits().hash(state),
            Self::StrategySignal { signal_id } => signal_id.hash(state),
        }
    }
}

/// Jackpot order implementation with probability-based execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JackpotOrder {
    pub params: JackpotOrderParams,
    pub created_at: DateTime<Utc>,
    pub execution_attempts: u32,
    pub last_probability_check: Option<DateTime<Utc>>,
}

impl JackpotOrder {
    pub fn new(params: JackpotOrderParams) -> Self {
        Self {
            params,
            created_at: Utc::now(),
            execution_attempts: 0,
            last_probability_check: None,
        }
    }

    /// Calculate current execution probability based on market conditions
    pub fn calculate_probability(
        &self,
        order_book: &OrderBookAggregator,
        current_volatility: f64,
    ) -> f64 {
        let mut probability = self.params.base_probability;

        // Adjust for volatility
        probability *= 1.0 + (current_volatility * self.params.volatility_multiplier);

        // Adjust for liquidity availability (simplified check using aggregated book)
        let aggregated = order_book.aggregate(5); // Get top 5 levels
        if let Some(_mid_price) = aggregated.mid_price() {
            // Use aggregated book to check liquidity
            let total_bid_volume = aggregated
                .bids()
                .levels()
                .iter()
                .map(|l| l.amount)
                .sum::<Decimal>();
            if total_bid_volume >= self.params.liquidity_threshold {
                probability *= 1.1; // 10% bonus for good liquidity
            }
        }

        // Time decay factor - probability increases as execution window closes
        let time_elapsed = Utc::now().signed_duration_since(self.created_at);
        let window_ratio = time_elapsed.num_milliseconds() as f64
            / self.params.execution_window.as_millis() as f64;

        if window_ratio > 0.5 {
            probability *= 1.0 + (window_ratio - 0.5) * 0.5; // Boost in second half of window
        }

        // Cap probability at 95%
        probability.min(0.95)
    }

    /// Check if order should be executed based on current conditions
    pub fn should_execute(
        &mut self,
        order_book: &OrderBookAggregator,
        current_volatility: f64,
    ) -> bool {
        self.execution_attempts += 1;
        self.last_probability_check = Some(Utc::now());

        let probability = self.calculate_probability(order_book, current_volatility);
        let random_value: f64 = rand::random();

        debug!(
            "Jackpot order probability check: {} vs random {}, attempt {}",
            probability, random_value, self.execution_attempts
        );

        random_value < probability
    }
}

/// Prophetic order implementation with predictive market analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropheticOrder {
    pub params: PropheticOrderParams,
    pub created_at: DateTime<Utc>,
    pub prediction_score: Option<f64>,
    pub last_analysis: Option<DateTime<Utc>>,
    pub market_indicators: HashMap<String, f64>,
}

impl PropheticOrder {
    pub fn new(params: PropheticOrderParams) -> Self {
        Self {
            params,
            created_at: Utc::now(),
            prediction_score: None,
            last_analysis: None,
            market_indicators: HashMap::new(),
        }
    }

    /// Analyze market conditions and generate prediction
    pub async fn analyze_market_conditions(
        &mut self,
        order_book: &OrderBookAggregator,
        historical_data: &[Decimal], // Price history for analysis
    ) -> Result<f64, UnindexedOrderError> {
        self.last_analysis = Some(Utc::now());

        // Calculate technical indicators
        let rsi = self.calculate_rsi(historical_data)?;
        let macd = self.calculate_macd(historical_data)?;
        let bollinger_position = self.calculate_bollinger_position(historical_data, order_book)?;
        let volume_profile = self.calculate_volume_profile(order_book)?;

        // Store indicators
        self.market_indicators.insert("rsi".to_string(), rsi);
        self.market_indicators.insert("macd".to_string(), macd);
        self.market_indicators
            .insert("bollinger".to_string(), bollinger_position);
        self.market_indicators
            .insert("volume".to_string(), volume_profile);

        // Calculate weighted prediction score
        let mut prediction_score = 0.0;
        for (indicator, value) in &self.market_indicators {
            if let Some(weight) = self.params.indicator_weights.get(indicator) {
                prediction_score += value * weight;
            }
        }

        // Apply model weight
        prediction_score *= self.params.model_weight;

        // Normalize to 0-1 range
        prediction_score = prediction_score.clamp(0.0, 1.0);

        self.prediction_score = Some(prediction_score);

        info!(
            "Prophetic order analysis complete: score={:.3}, indicators={:?}",
            prediction_score, self.market_indicators
        );

        Ok(prediction_score)
    }

    /// Check if order should be executed based on prediction confidence
    pub fn should_execute(&self) -> bool {
        self.prediction_score
            .map(|score| score >= self.params.confidence_threshold)
            .unwrap_or(false)
    }

    // Technical indicator calculations (simplified implementations)
    fn calculate_rsi(&self, prices: &[Decimal]) -> Result<f64, UnindexedOrderError> {
        if prices.len() < 14 {
            return Ok(0.5); // Neutral RSI
        }

        let mut gains = Vec::new();
        let mut losses = Vec::new();

        for i in 1..prices.len() {
            let change = prices[i] - prices[i - 1];
            if change > Decimal::ZERO {
                gains.push(change);
                losses.push(Decimal::ZERO);
            } else {
                gains.push(Decimal::ZERO);
                losses.push(-change);
            }
        }

        let avg_gain: Decimal = gains.iter().sum::<Decimal>() / Decimal::from(gains.len());
        let avg_loss: Decimal = losses.iter().sum::<Decimal>() / Decimal::from(losses.len());

        if avg_loss == Decimal::ZERO {
            return Ok(1.0);
        }

        let rs = avg_gain / avg_loss;
        let rsi = 100.0 - (100.0 / (1.0 + rs.to_f64().unwrap_or(1.0)));

        Ok(rsi / 100.0) // Normalize to 0-1
    }

    fn calculate_macd(&self, prices: &[Decimal]) -> Result<f64, UnindexedOrderError> {
        if prices.len() < 26 {
            return Ok(0.5); // Neutral MACD
        }

        // Simplified MACD calculation
        let ema12 = self.calculate_ema(prices, 12);
        let ema26 = self.calculate_ema(prices, 26);
        let macd_line = ema12 - ema26;

        // Normalize MACD signal (-1 to 1, then 0 to 1)
        let normalized = (macd_line.to_f64().unwrap_or(0.0) + 1.0) / 2.0;
        Ok(normalized.clamp(0.0, 1.0))
    }

    fn calculate_bollinger_position(
        &self,
        prices: &[Decimal],
        order_book: &OrderBookAggregator,
    ) -> Result<f64, UnindexedOrderError> {
        if prices.len() < 20 {
            return Ok(0.5);
        }

        let sma20: Decimal = prices.iter().sum::<Decimal>() / Decimal::from(prices.len());
        let variance: f64 = prices
            .iter()
            .map(|p| (*p - sma20).to_f64().unwrap_or(0.0))
            .map(|diff| diff * diff)
            .sum::<f64>()
            / prices.len() as f64;

        let std_dev = Decimal::from_f64_retain(variance.sqrt()).unwrap_or(Decimal::ZERO);
        let upper_band = sma20 + (std_dev * Decimal::from(2));
        let lower_band = sma20 - (std_dev * Decimal::from(2));

        // Get current price from aggregated book
        let aggregated = order_book.aggregate(1);
        if let Some(current_price) = aggregated.mid_price() {
            let position = (current_price - lower_band) / (upper_band - lower_band);
            Ok(position.to_f64().unwrap_or(0.5).clamp(0.0, 1.0))
        } else {
            Ok(0.5)
        }
    }

    fn calculate_volume_profile(
        &self,
        order_book: &OrderBookAggregator,
    ) -> Result<f64, UnindexedOrderError> {
        // Calculate volume-based signal using aggregated book
        let aggregated = order_book.aggregate(10); // Top 10 levels
        let total_bid_volume = aggregated
            .bids()
            .levels()
            .iter()
            .map(|l| l.amount)
            .sum::<Decimal>();
        let total_ask_volume = aggregated
            .asks()
            .levels()
            .iter()
            .map(|l| l.amount)
            .sum::<Decimal>();

        if total_bid_volume + total_ask_volume == Decimal::ZERO {
            return Ok(0.5);
        }

        let volume_ratio = total_bid_volume / (total_bid_volume + total_ask_volume);
        Ok(volume_ratio.to_f64().unwrap_or(0.5))
    }

    fn calculate_ema(&self, prices: &[Decimal], period: usize) -> Decimal {
        if prices.is_empty() {
            return Decimal::ZERO;
        }

        let multiplier = Decimal::from(2) / Decimal::from(period + 1);
        let mut ema = prices[0];

        for price in prices.iter().skip(1) {
            ema = (*price * multiplier) + (ema * (Decimal::ONE - multiplier));
        }

        ema
    }
}

/// Event-triggered order implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTriggeredOrder {
    pub params: EventTriggeredParams,
    pub created_at: DateTime<Utc>,
    pub triggered_at: Option<DateTime<Utc>>,
    pub triggering_event: Option<EventType>,
    pub event_correlation_score: Option<f64>,
}

impl EventTriggeredOrder {
    pub fn new(params: EventTriggeredParams) -> Self {
        Self {
            params,
            created_at: Utc::now(),
            triggered_at: None,
            triggering_event: None,
            event_correlation_score: None,
        }
    }

    /// Check if any monitored events have triggered
    pub async fn check_triggers(
        &mut self,
        order_book: &OrderBookAggregator,
        market_events: &[MarketEvent],
    ) -> bool {
        for event_type in &self.params.trigger_events {
            if self
                .evaluate_event_trigger(event_type, order_book, market_events)
                .await
            {
                self.triggered_at = Some(Utc::now());
                self.triggering_event = Some(event_type.clone());

                info!(
                    "Event-triggered order activated by {:?} at {}",
                    event_type,
                    self.triggered_at.unwrap()
                );

                return true;
            }
        }

        // Check for timeout
        let elapsed = Utc::now().signed_duration_since(self.created_at);
        if elapsed > chrono::Duration::from_std(self.params.max_wait_time).unwrap() {
            warn!("Event-triggered order timed out after {:?}", elapsed);
            return false;
        }

        false
    }

    /// Check if order should be executed (after trigger and delay)
    pub fn should_execute(&self) -> bool {
        if let Some(trigger_time) = self.triggered_at {
            let elapsed = Utc::now().signed_duration_since(trigger_time);
            elapsed >= chrono::Duration::from_std(self.params.execution_delay).unwrap()
        } else {
            false
        }
    }

    async fn evaluate_event_trigger(
        &self,
        event_type: &EventType,
        order_book: &OrderBookAggregator,
        market_events: &[MarketEvent],
    ) -> bool {
        match event_type {
            EventType::PriceMove { threshold } => {
                let aggregated = order_book.aggregate(1);
                if let Some(mid_price) = aggregated.mid_price() {
                    // Check for significant price movement (simplified)
                    // In practice, this would compare with historical prices
                    return mid_price != *threshold;
                }
                false
            }
            EventType::VolumeSpike { multiplier } => {
                let aggregated = order_book.aggregate(10);
                let current_volume = aggregated
                    .bids()
                    .levels()
                    .iter()
                    .map(|l| l.amount)
                    .sum::<Decimal>()
                    + aggregated
                        .asks()
                        .levels()
                        .iter()
                        .map(|l| l.amount)
                        .sum::<Decimal>();
                // Compare with average volume (simplified - would use historical data)
                current_volume > Decimal::from_f64_retain(*multiplier).unwrap_or(Decimal::ONE)
            }
            EventType::ArbitrageOpportunity { min_spread } => {
                // Check cross-exchange spread using best bid/ask from aggregator
                if let (Some((_bid_ex, bid_price)), Some((_ask_ex, ask_price))) =
                    (order_book.best_bid(), order_book.best_ask())
                {
                    (ask_price - bid_price) >= *min_spread
                } else {
                    false
                }
            }
            EventType::BookImbalance { ratio_threshold } => {
                let aggregated = order_book.aggregate(5);
                let bid_volume = aggregated
                    .bids()
                    .levels()
                    .iter()
                    .map(|l| l.amount)
                    .sum::<Decimal>();
                let ask_volume = aggregated
                    .asks()
                    .levels()
                    .iter()
                    .map(|l| l.amount)
                    .sum::<Decimal>();

                if ask_volume > Decimal::ZERO {
                    let ratio = (bid_volume / ask_volume).to_f64().unwrap_or(1.0);
                    ratio > *ratio_threshold || ratio < (1.0 / ratio_threshold)
                } else {
                    false
                }
            }
            EventType::NewssentimentChange { score_delta: _ } => {
                // Check recent market events for news sentiment
                market_events
                    .iter()
                    .any(|event| matches!(event, MarketEvent::NewsSentiment { .. }))
            }
            EventType::StrategySignal { signal_id } => {
                // Check for specific strategy signals
                market_events.iter().any(|event| {
                    if let MarketEvent::StrategySignal { id, .. } = event {
                        id == signal_id
                    } else {
                        false
                    }
                })
            }
        }
    }
}

/// Market events that can trigger orders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    PriceChange {
        instrument: String,
        old_price: Decimal,
        new_price: Decimal,
        timestamp: DateTime<Utc>,
    },
    VolumeSpike {
        instrument: String,
        volume: Decimal,
        multiplier: f64,
        timestamp: DateTime<Utc>,
    },
    NewsSentiment {
        sentiment_score: f64,
        confidence: f64,
        keywords: Vec<String>,
        timestamp: DateTime<Utc>,
    },
    StrategySignal {
        id: String,
        signal_type: String,
        strength: f64,
        metadata: HashMap<String, String>,
        timestamp: DateTime<Utc>,
    },
}

/// Enhanced order state for sensor-specific orders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensorOrderState {
    /// Jackpot order awaiting probability trigger
    JackpotPending(JackpotOrder),
    /// Prophetic order analyzing market conditions
    PropheticAnalyzing(PropheticOrder),
    /// Event-triggered order waiting for event
    EventWaiting(EventTriggeredOrder),
    /// Order ready for execution
    ReadyForExecution {
        order_type: OrderKind,
        confidence_score: Option<f64>,
        execution_priority: u8, // 1-255, higher = more urgent
    },
}

/// Performance metrics for order execution
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrderExecutionMetrics {
    pub total_orders: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub average_execution_time: Duration,
    pub fastest_execution: Duration,
    pub slowest_execution: Duration,
    pub jackpot_hit_rate: f64,
    pub prophetic_accuracy: f64,
    pub event_trigger_rate: f64,
}

impl OrderExecutionMetrics {
    pub fn success_rate(&self) -> f64 {
        if self.total_orders == 0 {
            0.0
        } else {
            self.successful_executions as f64 / self.total_orders as f64
        }
    }

    pub fn update_execution(&mut self, execution_time: Duration, success: bool) {
        self.total_orders += 1;

        if success {
            self.successful_executions += 1;
        } else {
            self.failed_executions += 1;
        }

        // Update timing metrics
        if self.total_orders == 1 {
            self.average_execution_time = execution_time;
            self.fastest_execution = execution_time;
            self.slowest_execution = execution_time;
        } else {
            // Rolling average
            let total_time = self.average_execution_time.as_nanos() as u64
                * (self.total_orders - 1)
                + execution_time.as_nanos() as u64;
            self.average_execution_time = Duration::from_nanos(total_time / self.total_orders);

            if execution_time < self.fastest_execution {
                self.fastest_execution = execution_time;
            }
            if execution_time > self.slowest_execution {
                self.slowest_execution = execution_time;
            }
        }
    }
}
