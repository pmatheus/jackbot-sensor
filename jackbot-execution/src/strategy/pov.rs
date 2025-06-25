use crate::strategy::advanced::OrderExecutionStrategy;
use crate::{
    client::ExecutionClient,
    error::UnindexedOrderError,
    order::{
        id::ClientOrderId,
        request::{OrderRequestOpen, RequestOpen},
        state::Open,
        Order, OrderKey,
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
use tracing::{debug, info, warn};

/// POV (Percentage of Volume) execution strategy.
///
/// This strategy aims to participate in a specified percentage of market volume,
/// dynamically adjusting order sizes based on observed trading activity.
/// It includes sophisticated volume prediction and adaptive participation rates.
#[derive(Debug, Clone)]
pub struct PovExecutor<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub client: C,
    pub aggregator: Arc<OrderBookAggregator>,
    rng: R,
    volume_tracker: VolumeTracker,
}

/// Configuration for POV execution strategy
#[derive(Debug, Clone)]
pub struct PovConfig {
    /// Target participation rate (0.0 to 1.0)
    pub target_participation_rate: f64,
    /// Minimum participation rate constraint
    pub min_participation_rate: f64,
    /// Maximum participation rate constraint  
    pub max_participation_rate: f64,
    /// Volume measurement window (in milliseconds)
    pub volume_window_ms: u64,
    /// Minimum order size as percentage of total
    pub min_order_percentage: f64,
    /// Maximum order size as percentage of total
    pub max_order_percentage: f64,
    /// How often to reassess and place new orders
    pub assessment_interval: Duration,
    /// Enable volume prediction for proactive sizing
    pub enable_volume_prediction: bool,
    /// Aggressiveness factor for urgent executions (0.0 to 2.0)
    pub urgency_factor: f64,
    /// Time limit for completing the entire order (None for no limit)
    pub time_limit: Option<Duration>,
}

impl Default for PovConfig {
    fn default() -> Self {
        Self {
            target_participation_rate: 0.10,                  // 10% participation
            min_participation_rate: 0.05,                     // 5% minimum
            max_participation_rate: 0.25,                     // 25% maximum
            volume_window_ms: 60_000,                         // 1 minute window
            min_order_percentage: 0.005,                      // 0.5% of total order
            max_order_percentage: 0.10,                       // 10% of total order
            assessment_interval: Duration::from_millis(2000), // Every 2 seconds
            enable_volume_prediction: true,
            urgency_factor: 1.0,
            time_limit: None,
        }
    }
}

/// Tracks volume over time for participation rate calculations
#[derive(Debug, Clone)]
struct VolumeTracker {
    volume_history: VecDeque<(Instant, Decimal)>,
    window_duration: Duration,
}

impl VolumeTracker {
    fn new(window_duration: Duration) -> Self {
        Self {
            volume_history: VecDeque::new(),
            window_duration,
        }
    }

    fn add_volume(&mut self, volume: Decimal) {
        let now = Instant::now();
        self.volume_history.push_back((now, volume));
        self.cleanup_old_entries(now);
    }

    fn cleanup_old_entries(&mut self, current_time: Instant) {
        while let Some(&(timestamp, _)) = self.volume_history.front() {
            if current_time.duration_since(timestamp) > self.window_duration {
                self.volume_history.pop_front();
            } else {
                break;
            }
        }
    }

    fn total_volume(&self) -> Decimal {
        let now = Instant::now();
        self.volume_history
            .iter()
            .filter(|(timestamp, _)| now.duration_since(*timestamp) <= self.window_duration)
            .map(|(_, volume)| *volume)
            .sum()
    }

    fn average_volume_rate(&self) -> Decimal {
        let total = self.total_volume();
        let window_seconds = self.window_duration.as_secs() as f64;
        if window_seconds > 0.0 {
            total / Decimal::from_f64(window_seconds).unwrap_or(Decimal::ONE)
        } else {
            Decimal::ZERO
        }
    }

    fn predicted_volume(&self, prediction_horizon: Duration) -> Decimal {
        // Simple linear extrapolation based on recent volume rate
        let rate = self.average_volume_rate();
        let horizon_seconds = prediction_horizon.as_secs() as f64;
        rate * Decimal::from_f64(horizon_seconds).unwrap_or(Decimal::ONE)
    }
}

impl<C, R> PovExecutor<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub fn new(client: C, aggregator: OrderBookAggregator, rng: R) -> Self {
        Self {
            client,
            aggregator: aggregator.into(),
            rng,
            volume_tracker: VolumeTracker::new(Duration::from_millis(60_000)),
        }
    }

    /// Update volume tracker with new market data
    pub fn update_volume(&mut self, volume: Decimal) {
        self.volume_tracker.add_volume(volume);
    }

    /// Calculate next order size based on participation rate and market conditions
    fn calculate_order_size(
        &mut self,
        remaining_quantity: Decimal,
        total_quantity: Decimal,
        config: &PovConfig,
        time_elapsed: Duration,
    ) -> Decimal {
        let current_volume_rate = self.volume_tracker.average_volume_rate();

        // If no volume history, use conservative sizing
        if current_volume_rate <= Decimal::ZERO {
            let fallback_percentage = config.min_order_percentage;
            return (total_quantity * Decimal::from_f64(fallback_percentage).unwrap())
                .min(remaining_quantity);
        }

        // Calculate target participation
        let mut target_rate = config.target_participation_rate;

        // Apply urgency factor if time limit is approaching
        if let Some(time_limit) = config.time_limit {
            let time_remaining = time_limit.saturating_sub(time_elapsed);
            let urgency_multiplier = if time_remaining.as_secs() < time_limit.as_secs() / 2 {
                config.urgency_factor
            } else {
                1.0
            };
            target_rate *= urgency_multiplier;
        }

        // Clamp participation rate
        target_rate = target_rate
            .max(config.min_participation_rate)
            .min(config.max_participation_rate);

        // Calculate order size based on predicted volume
        let prediction_horizon = config.assessment_interval;
        let predicted_volume = if config.enable_volume_prediction {
            self.volume_tracker.predicted_volume(prediction_horizon)
        } else {
            current_volume_rate * Decimal::from_f64(prediction_horizon.as_secs() as f64).unwrap()
        };

        let target_participation_volume =
            predicted_volume * Decimal::from_f64(target_rate).unwrap();

        // Add randomness to avoid predictable patterns
        let randomness_factor = 1.0 + self.rng.random_range(-0.1..=0.1);
        let base_order_size =
            target_participation_volume * Decimal::from_f64(randomness_factor).unwrap();

        // Apply size constraints
        let min_size = total_quantity * Decimal::from_f64(config.min_order_percentage).unwrap();
        let max_size = total_quantity * Decimal::from_f64(config.max_order_percentage).unwrap();

        let constrained_size = base_order_size.max(min_size).min(max_size);
        let final_size = constrained_size.min(remaining_quantity);

        debug!(
            target_rate = target_rate,
            predicted_volume = %predicted_volume,
            target_participation_volume = %target_participation_volume,
            final_size = %final_size,
            current_volume_rate = %current_volume_rate,
            "Calculated POV order size"
        );

        final_size
    }

    /// Execute POV strategy for the given order
    pub async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: PovConfig,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        let total_quantity = request.state.quantity;
        let mut remaining_quantity = total_quantity;
        let mut results = Vec::new();
        let start_time = Instant::now();

        // Update volume tracking window
        self.volume_tracker = VolumeTracker::new(Duration::from_millis(config.volume_window_ms));

        info!(
            total_quantity = %total_quantity,
            target_participation_rate = config.target_participation_rate,
            instrument = %request.key.instrument,
            side = ?request.state.side,
            "Starting POV execution"
        );

        while remaining_quantity > Decimal::ZERO {
            let time_elapsed = start_time.elapsed();

            // Check time limit
            if let Some(time_limit) = config.time_limit {
                if time_elapsed >= time_limit {
                    warn!(
                        time_elapsed = ?time_elapsed,
                        time_limit = ?time_limit,
                        remaining_quantity = %remaining_quantity,
                        "POV execution time limit reached"
                    );
                    break;
                }
            }

            // Calculate next order size
            let order_size = self.calculate_order_size(
                remaining_quantity,
                total_quantity,
                &config,
                time_elapsed,
            );

            if order_size <= Decimal::ZERO {
                debug!("Calculated order size is zero, waiting for next assessment");
                sleep(config.assessment_interval).await;
                continue;
            }

            // Place order
            let order_request = OrderRequestOpen {
                key: OrderKey {
                    exchange: request.key.exchange,
                    instrument: request.key.instrument.clone(),
                    strategy: request.key.strategy.clone(),
                    cid: ClientOrderId::new(format!("{}_pov_{}", request.key.cid, results.len())),
                },
                state: RequestOpen {
                    side: request.state.side,
                    price: request.state.price,
                    quantity: order_size,
                    kind: request.state.kind,
                    time_in_force: request.state.time_in_force,
                },
            };

            debug!(
                order_size = %order_size,
                remaining = %remaining_quantity,
                order_number = results.len() + 1,
                "Placing POV order"
            );

            let result = self.client.clone().open_order(order_request).await;
            let order_result = result.map_instrument(|inst_ref| inst_ref.clone());
            results.push(order_result);

            remaining_quantity -= order_size;

            // Wait for next assessment
            sleep(config.assessment_interval).await;

            // Simulate volume update (in real implementation, this would come from market data)
            let simulated_volume =
                Decimal::from_f64(self.rng.random_range(100.0..=1000.0)).unwrap();
            self.update_volume(simulated_volume);
        }

        info!(
            orders_placed = results.len(),
            total_quantity = %total_quantity,
            execution_time = ?start_time.elapsed(),
            "Completed POV execution"
        );

        results
    }
}

#[async_trait]
impl<C, R> OrderExecutionStrategy for PovExecutor<C, R>
where
    C: ExecutionClient + Clone + Send + Sync,
    R: Rng + Clone + Send + Sync,
{
    type Config = PovConfig;

    async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: Self::Config,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        PovExecutor::execute(self, request, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_volume_tracker() {
        let mut tracker = VolumeTracker::new(Duration::from_secs(60));

        tracker.add_volume(dec!(100));
        tracker.add_volume(dec!(200));

        assert_eq!(tracker.total_volume(), dec!(300));

        let rate = tracker.average_volume_rate();
        assert!(rate > Decimal::ZERO);
    }

    #[test]
    fn test_pov_config_defaults() {
        let config = PovConfig::default();
        assert_eq!(config.target_participation_rate, 0.10);
        assert_eq!(config.volume_window_ms, 60_000);
        assert!(config.enable_volume_prediction);
    }

    #[test]
    fn test_volume_prediction() {
        let mut tracker = VolumeTracker::new(Duration::from_secs(60));
        tracker.add_volume(dec!(1000));

        let predicted = tracker.predicted_volume(Duration::from_secs(30));
        assert!(predicted > Decimal::ZERO);
    }
}
