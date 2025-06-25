use crate::strategy::advanced::OrderExecutionStrategy;
use crate::{
    client::ExecutionClient,
    error::UnindexedOrderError,
    order::{
        id::ClientOrderId,
        request::{OrderRequestOpen, RequestOpen},
        state::Open,
        Order, OrderKey, OrderKind, Side,
    },
};
use async_trait::async_trait;
use jackbot_data::books::aggregator::OrderBookAggregator;
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};
use rand::prelude::*;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::{collections::VecDeque, sync::Arc};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, info};

/// Implementation Shortfall (IS) optimization strategy.
///
/// This advanced execution algorithm minimizes implementation shortfall by balancing
/// market impact costs against timing risk. It dynamically adjusts execution speed
/// based on market conditions, volatility, and the urgency of completion.
#[derive(Debug, Clone)]
pub struct ImplementationShortfallExecutor<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub client: C,
    pub aggregator: Arc<OrderBookAggregator>,
    rng: R,
    market_impact_model: MarketImpactModel,
    volatility_estimator: VolatilityEstimator,
}

/// Configuration for Implementation Shortfall strategy
#[derive(Debug, Clone)]
pub struct ImplementationShortfallConfig {
    /// Expected completion time for the order
    pub target_completion_time: Duration,
    /// Risk aversion parameter (0.0 = risk neutral, 1.0 = risk averse)
    pub risk_aversion: f64,
    /// Market impact sensitivity (higher = more sensitive to impact)
    pub impact_sensitivity: f64,
    /// Volatility window for calculations (in milliseconds)
    pub volatility_window_ms: u64,
    /// Minimum time between order placements
    pub min_interval: Duration,
    /// Maximum time between order placements
    pub max_interval: Duration,
    /// Participation rate limits (min, max)
    pub participation_limits: (f64, f64),
    /// Enable adaptive timing based on market microstructure
    pub adaptive_timing: bool,
    /// Slippage tolerance before switching to market orders
    pub slippage_tolerance: Decimal,
}

impl Default for ImplementationShortfallConfig {
    fn default() -> Self {
        Self {
            target_completion_time: Duration::from_secs(300), // 5 minutes
            risk_aversion: 0.5,
            impact_sensitivity: 1.0,
            volatility_window_ms: 300_000, // 5 minutes
            min_interval: Duration::from_millis(500),
            max_interval: Duration::from_secs(30),
            participation_limits: (0.05, 0.30), // 5% to 30%
            adaptive_timing: true,
            slippage_tolerance: Decimal::from_str("0.001").unwrap(), // 10 bps
        }
    }
}

/// Market impact model for estimating trading costs
#[derive(Debug, Clone)]
struct MarketImpactModel {
    /// Historical impact observations
    impact_history: VecDeque<(Instant, Decimal, Decimal)>, // (time, size, impact)
    /// Linear impact coefficient
    linear_coefficient: f64,
    /// Square-root impact coefficient  
    sqrt_coefficient: f64,
}

impl MarketImpactModel {
    fn new() -> Self {
        Self {
            impact_history: VecDeque::new(),
            linear_coefficient: 0.001, // 10 bps per unit
            sqrt_coefficient: 0.01,    // Square root component
        }
    }

    fn add_observation(&mut self, size: Decimal, impact: Decimal) {
        self.impact_history
            .push_back((Instant::now(), size, impact));

        // Keep only recent observations
        let cutoff = Instant::now() - Duration::from_secs(3600); // 1 hour
        while let Some(&(timestamp, _, _)) = self.impact_history.front() {
            if timestamp < cutoff {
                self.impact_history.pop_front();
            } else {
                break;
            }
        }

        // Update model parameters based on observations
        self.update_parameters();
    }

    fn update_parameters(&mut self) {
        if self.impact_history.len() < 10 {
            return; // Need minimum observations
        }

        // Simple linear regression to update coefficients
        // In practice, you'd use more sophisticated econometric methods
        let observations: Vec<_> = self.impact_history.iter().collect();
        let n = observations.len() as f64;

        let sum_size: f64 = observations
            .iter()
            .map(|(_, size, _)| size.to_f64().unwrap_or(0.0))
            .sum();
        let sum_impact: f64 = observations
            .iter()
            .map(|(_, _, impact)| impact.to_f64().unwrap_or(0.0))
            .sum();
        let sum_size_impact: f64 = observations
            .iter()
            .map(|(_, size, impact)| size.to_f64().unwrap_or(0.0) * impact.to_f64().unwrap_or(0.0))
            .sum();
        let sum_size_sq: f64 = observations
            .iter()
            .map(|(_, size, _)| size.to_f64().unwrap_or(0.0).powi(2))
            .sum();

        if n * sum_size_sq - sum_size.powi(2) != 0.0 {
            self.linear_coefficient = (n * sum_size_impact - sum_size * sum_impact)
                / (n * sum_size_sq - sum_size.powi(2));
        }
    }

    fn estimate_impact(&self, order_size: Decimal, adv: Decimal) -> Decimal {
        let size_f64 = order_size.to_f64().unwrap_or(0.0);
        let adv_f64 = adv.to_f64().unwrap_or(1.0);

        // Market impact = linear component + square root component
        let linear_impact = self.linear_coefficient * size_f64 / adv_f64;
        let sqrt_impact = self.sqrt_coefficient * (size_f64 / adv_f64).sqrt();

        Decimal::from_f64(linear_impact + sqrt_impact).unwrap_or(Decimal::ZERO)
    }
}

/// Volatility estimator using EWMA
#[derive(Debug, Clone)]
struct VolatilityEstimator {
    price_history: VecDeque<(Instant, Decimal)>,
    window_duration: Duration,
    decay_factor: f64,
    current_volatility: f64,
}

impl VolatilityEstimator {
    fn new(window_duration: Duration) -> Self {
        Self {
            price_history: VecDeque::new(),
            window_duration,
            decay_factor: 0.94,       // EWMA decay factor
            current_volatility: 0.01, // 1% initial volatility
        }
    }

    fn add_price(&mut self, price: Decimal) {
        let now = Instant::now();
        self.price_history.push_back((now, price));

        // Clean up old prices
        while let Some(&(timestamp, _)) = self.price_history.front() {
            if now.duration_since(timestamp) > self.window_duration {
                self.price_history.pop_front();
            } else {
                break;
            }
        }

        self.update_volatility();
    }

    fn update_volatility(&mut self) {
        if self.price_history.len() < 2 {
            return;
        }

        let price_vec: Vec<_> = self.price_history.iter().collect();
        let returns: Vec<f64> = price_vec
            .windows(2)
            .map(|window| {
                let (_, p1) = *window[0];
                let (_, p2) = *window[1];
                if p1 > Decimal::ZERO {
                    ((p2 - p1) / p1).to_f64().unwrap_or(0.0)
                } else {
                    0.0
                }
            })
            .collect();

        if returns.is_empty() {
            return;
        }

        // EWMA volatility calculation
        let mut ewma_var = 0.0;
        for ret in returns.iter().rev() {
            ewma_var = self.decay_factor * ewma_var + (1.0 - self.decay_factor) * ret.powi(2);
        }

        self.current_volatility = ewma_var.sqrt();
    }

    fn get_volatility(&self) -> f64 {
        self.current_volatility
    }
}

impl<C, R> ImplementationShortfallExecutor<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub fn new(client: C, aggregator: OrderBookAggregator, rng: R) -> Self {
        Self {
            client,
            aggregator: aggregator.into(),
            rng,
            market_impact_model: MarketImpactModel::new(),
            volatility_estimator: VolatilityEstimator::new(Duration::from_secs(300)),
        }
    }

    /// Update market data for model calibration
    pub fn update_market_data(&mut self, price: Decimal) {
        self.volatility_estimator.add_price(price);
    }

    /// Calculate optimal execution schedule using IS optimization
    fn calculate_execution_schedule(
        &self,
        total_quantity: Decimal,
        config: &ImplementationShortfallConfig,
    ) -> Vec<(Duration, Decimal)> {
        let total_time = config.target_completion_time.as_secs_f64();
        let volatility = self.volatility_estimator.get_volatility();

        // Estimate average daily volume (ADV) - simplified
        let _estimated_adv = Decimal::from_f64(1_000_000.0).unwrap(); // Placeholder

        // Calculate optimal participation rate using IS theory
        let risk_penalty = config.risk_aversion * volatility.powi(2);
        let impact_cost_sensitivity = config.impact_sensitivity;

        // Optimal participation rate balances market impact vs. risk
        let base_participation_rate = (risk_penalty / impact_cost_sensitivity)
            .sqrt()
            .max(config.participation_limits.0)
            .min(config.participation_limits.1);

        // Create execution schedule
        let num_intervals = ((total_time / config.min_interval.as_secs_f64()) as usize)
            .max(1)
            .min(100); // Reasonable upper bound

        let mut schedule = Vec::new();
        let mut remaining = total_quantity;

        for i in 0..num_intervals {
            let time_progress = (i as f64) / (num_intervals as f64);
            let time_from_start = Duration::from_secs_f64(time_progress * total_time);

            // Adjust participation rate based on remaining time and quantity
            let urgency_multiplier = if i > num_intervals * 3 / 4 {
                1.5 // Increase urgency near end
            } else {
                1.0
            };

            let adjusted_rate = base_participation_rate * urgency_multiplier;
            let interval_quantity =
                (remaining * Decimal::from_f64(adjusted_rate).unwrap()).min(remaining);

            if interval_quantity > Decimal::ZERO {
                schedule.push((time_from_start, interval_quantity));
                remaining -= interval_quantity;
            }

            if remaining <= Decimal::ZERO {
                break;
            }
        }

        // Ensure all quantity is allocated
        if remaining > Decimal::ZERO && !schedule.is_empty() {
            schedule.last_mut().unwrap().1 += remaining;
        }

        debug!(
            num_intervals = schedule.len(),
            base_participation_rate = base_participation_rate,
            volatility = volatility,
            "Generated IS execution schedule"
        );

        schedule
    }

    /// Execute Implementation Shortfall strategy
    pub async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: ImplementationShortfallConfig,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        let total_quantity = request.state.quantity;
        let start_time = Instant::now();
        let mut results = Vec::new();

        info!(
            total_quantity = %total_quantity,
            target_completion_time = ?config.target_completion_time,
            risk_aversion = config.risk_aversion,
            instrument = %request.key.instrument,
            side = ?request.state.side,
            "Starting Implementation Shortfall execution"
        );

        // Calculate optimal execution schedule
        let schedule = self.calculate_execution_schedule(total_quantity, &config);

        for (target_time, quantity) in schedule {
            // Wait until target time
            let elapsed = start_time.elapsed();
            if target_time > elapsed {
                sleep(target_time - elapsed).await;
            }

            // Check if we should switch to market orders due to slippage
            let current_spread = if let (Some((_, bid)), Some((_, ask))) =
                (self.aggregator.best_bid(), self.aggregator.best_ask())
            {
                ask - bid
            } else {
                Decimal::ZERO
            };

            let use_market_order = current_spread > config.slippage_tolerance;

            // Determine order type and price
            let (order_kind, order_price) = if use_market_order {
                (OrderKind::Market, None)
            } else {
                // Use limit order at mid price with small improvement
                if let (Some((_, bid)), Some((_, ask))) =
                    (self.aggregator.best_bid(), self.aggregator.best_ask())
                {
                    let mid_price = (bid + ask) / Decimal::TWO;
                    let improvement = current_spread * Decimal::from_str("0.1").unwrap(); // 10% of spread
                    let limit_price = match request.state.side {
                        Side::Buy => mid_price + improvement,
                        Side::Sell => mid_price - improvement,
                    };
                    (OrderKind::Limit, Some(limit_price))
                } else {
                    (request.state.kind, Some(request.state.price))
                }
            };

            // Place order
            let order_request = OrderRequestOpen {
                key: OrderKey {
                    exchange: request.key.exchange,
                    instrument: request.key.instrument.clone(),
                    strategy: request.key.strategy.clone(),
                    cid: ClientOrderId::new(format!("{}_is_{}", request.key.cid, results.len())),
                },
                state: RequestOpen {
                    side: request.state.side,
                    price: order_price.unwrap_or(request.state.price),
                    quantity,
                    kind: order_kind,
                    time_in_force: request.state.time_in_force,
                },
            };

            debug!(
                quantity = %quantity,
                order_type = ?order_kind,
                price = ?order_price,
                order_number = results.len() + 1,
                elapsed_time = ?elapsed,
                "Placing IS order"
            );

            let result = self.client.clone().open_order(order_request).await;
            let order_result = result.map_instrument(|inst_ref| inst_ref.clone());
            results.push(order_result);
        }

        info!(
            orders_placed = results.len(),
            total_quantity = %total_quantity,
            execution_time = ?start_time.elapsed(),
            "Completed Implementation Shortfall execution"
        );

        results
    }
}

#[async_trait]
impl<C, R> OrderExecutionStrategy for ImplementationShortfallExecutor<C, R>
where
    C: ExecutionClient + Clone + Send + Sync,
    R: Rng + Clone + Send + Sync,
{
    type Config = ImplementationShortfallConfig;

    async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: Self::Config,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        ImplementationShortfallExecutor::execute(self, request, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_market_impact_model() {
        let mut model = MarketImpactModel::new();

        model.add_observation(dec!(1000), dec!(0.001));
        model.add_observation(dec!(2000), dec!(0.002));

        let impact = model.estimate_impact(dec!(1500), dec!(100000));
        assert!(impact > Decimal::ZERO);
    }

    #[test]
    fn test_volatility_estimator() {
        let mut estimator = VolatilityEstimator::new(Duration::from_secs(300));

        estimator.add_price(dec!(100));
        estimator.add_price(dec!(101));
        estimator.add_price(dec!(99));

        let vol = estimator.get_volatility();
        assert!(vol > 0.0);
    }

    #[test]
    fn test_is_config_defaults() {
        let config = ImplementationShortfallConfig::default();
        assert_eq!(config.risk_aversion, 0.5);
        assert_eq!(config.participation_limits, (0.05, 0.30));
    }
}
