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
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, info, warn};

/// Dark pool aggregation and routing strategy.
///
/// This strategy routes orders to dark pools and alternative trading systems (ATS)
/// to minimize market impact and information leakage. It includes intelligent
/// routing based on fill probability, latency, and historical performance.
#[derive(Debug, Clone)]
pub struct DarkPoolRouter<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub client: C,
    pub aggregator: Arc<OrderBookAggregator>,
    rng: R,
    dark_pools: Vec<DarkPoolVenue>,
    performance_tracker: PerformanceTracker,
}

/// Configuration for dark pool routing
#[derive(Debug, Clone)]
pub struct DarkPoolConfig {
    /// Maximum percentage of order to send to dark pools (0.0 to 1.0)
    pub max_dark_allocation: f64,
    /// Minimum order size for dark pool routing (to avoid fragmentation)
    pub min_dark_size: Decimal,
    /// Time limit for dark pool execution before routing to lit venues
    pub dark_timeout: Duration,
    /// Probability threshold for routing to dark pools
    pub fill_probability_threshold: f64,
    /// Enable intelligent routing based on historical performance
    pub enable_smart_routing: bool,
    /// Preferred dark pool venues (ordered by preference)
    pub preferred_venues: Vec<DarkPoolType>,
    /// Maximum number of venues to route to simultaneously
    pub max_venues: usize,
    /// Latency penalty factor for venue selection
    pub latency_penalty: f64,
}

impl Default for DarkPoolConfig {
    fn default() -> Self {
        Self {
            max_dark_allocation: 0.70, // Up to 70% to dark pools
            min_dark_size: Decimal::from_str("1000").unwrap(),
            dark_timeout: Duration::from_secs(30),
            fill_probability_threshold: 0.3,
            enable_smart_routing: true,
            preferred_venues: vec![
                DarkPoolType::InstitutionalCrossing,
                DarkPoolType::BlockTrading,
                DarkPoolType::MidpointMatching,
            ],
            max_venues: 4,
            latency_penalty: 0.1,
        }
    }
}

/// Types of dark pool venues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DarkPoolType {
    /// Traditional institutional crossing networks
    InstitutionalCrossing,
    /// Block trading networks for large orders
    BlockTrading,
    /// Midpoint matching systems
    MidpointMatching,
    /// Alternative trading systems (ATS)
    AlernativeTrading,
    /// Internal crossing within prime brokerage
    InternalCrossing,
}

/// Dark pool venue information
#[derive(Debug, Clone)]
struct DarkPoolVenue {
    venue_type: DarkPoolType,
    exchange_id: ExchangeId,
    /// Average latency to this venue
    avg_latency_ms: f64,
    /// Historical fill rate (0.0 to 1.0)
    fill_rate: f64,
    /// Minimum order size
    min_size: Decimal,
    /// Maximum order size
    max_size: Decimal,
    /// Supported order types
    supported_types: Vec<String>,
    /// Current availability
    is_available: bool,
}

/// Performance tracking for dark pool venues
#[derive(Debug, Clone)]
struct PerformanceTracker {
    venue_stats: HashMap<DarkPoolType, VenueStats>,
    recent_performance: BTreeMap<Instant, (DarkPoolType, ExecutionResult)>,
}

#[derive(Debug, Clone)]
struct VenueStats {
    total_orders: u64,
    filled_orders: u64,
    total_quantity: Decimal,
    filled_quantity: Decimal,
    avg_fill_time: Duration,
    avg_price_improvement: Decimal,
}

#[derive(Debug, Clone)]
struct ExecutionResult {
    filled: bool,
    quantity_filled: Decimal,
    fill_time: Duration,
    price_improvement: Decimal,
}

impl PerformanceTracker {
    fn new() -> Self {
        Self {
            venue_stats: HashMap::new(),
            recent_performance: BTreeMap::new(),
        }
    }

    fn record_execution(&mut self, venue: DarkPoolType, result: ExecutionResult) {
        // Update venue statistics
        let stats = self.venue_stats.entry(venue).or_insert_with(|| VenueStats {
            total_orders: 0,
            filled_orders: 0,
            total_quantity: Decimal::ZERO,
            filled_quantity: Decimal::ZERO,
            avg_fill_time: Duration::from_secs(0),
            avg_price_improvement: Decimal::ZERO,
        });

        stats.total_orders += 1;
        if result.filled {
            stats.filled_orders += 1;
        }
        stats.filled_quantity += result.quantity_filled;

        // Update rolling averages (simplified)
        if stats.filled_orders > 0 {
            stats.avg_fill_time = Duration::from_millis(
                (stats.avg_fill_time.as_millis() as u64 + result.fill_time.as_millis() as u64) / 2,
            );
            stats.avg_price_improvement =
                (stats.avg_price_improvement + result.price_improvement) / Decimal::TWO;
        }

        // Store recent performance
        self.recent_performance
            .insert(Instant::now(), (venue, result));

        // Clean up old performance data (keep last hour)
        let cutoff = Instant::now() - Duration::from_secs(3600);
        while let Some((&timestamp, _)) = self.recent_performance.first_key_value() {
            if timestamp < cutoff {
                self.recent_performance.remove(&timestamp);
            } else {
                break;
            }
        }
    }

    fn get_fill_probability(&self, venue: DarkPoolType) -> f64 {
        if let Some(stats) = self.venue_stats.get(&venue) {
            if stats.total_orders > 0 {
                stats.filled_orders as f64 / stats.total_orders as f64
            } else {
                0.5 // Default probability
            }
        } else {
            0.5
        }
    }

    fn get_expected_fill_time(&self, venue: DarkPoolType) -> Duration {
        self.venue_stats
            .get(&venue)
            .map(|stats| stats.avg_fill_time)
            .unwrap_or(Duration::from_secs(15)) // Default expectation
    }
}

impl<C, R> DarkPoolRouter<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub fn new(client: C, aggregator: OrderBookAggregator, rng: R) -> Self {
        // Initialize with some default dark pool venues
        let dark_pools = vec![
            DarkPoolVenue {
                venue_type: DarkPoolType::InstitutionalCrossing,
                exchange_id: ExchangeId::Other, // Dark crossing venue
                avg_latency_ms: 5.0,
                fill_rate: 0.65,
                min_size: Decimal::from_str("10000").unwrap(),
                max_size: Decimal::from_str("10000000").unwrap(),
                supported_types: vec!["LIMIT".to_string(), "MARKET".to_string()],
                is_available: true,
            },
            DarkPoolVenue {
                venue_type: DarkPoolType::BlockTrading,
                exchange_id: ExchangeId::Other, // Block network venue
                avg_latency_ms: 8.0,
                fill_rate: 0.45,
                min_size: Decimal::from_str("50000").unwrap(),
                max_size: Decimal::from_str("50000000").unwrap(),
                supported_types: vec!["BLOCK".to_string()],
                is_available: true,
            },
            DarkPoolVenue {
                venue_type: DarkPoolType::MidpointMatching,
                exchange_id: ExchangeId::Other, // Midpoint ATS venue
                avg_latency_ms: 3.0,
                fill_rate: 0.55,
                min_size: Decimal::from_str("5000").unwrap(),
                max_size: Decimal::from_str("5000000").unwrap(),
                supported_types: vec!["MIDPOINT".to_string()],
                is_available: true,
            },
        ];

        Self {
            client,
            aggregator: aggregator.into(),
            rng,
            dark_pools,
            performance_tracker: PerformanceTracker::new(),
        }
    }

    /// Calculate routing allocation across dark pools
    fn calculate_routing_allocation(
        &self,
        total_quantity: Decimal,
        config: &DarkPoolConfig,
    ) -> Vec<(DarkPoolVenue, Decimal)> {
        let max_dark_quantity =
            total_quantity * Decimal::from_f64(config.max_dark_allocation).unwrap();
        let mut allocations = Vec::new();
        let mut remaining_quantity = max_dark_quantity;

        // Filter available venues
        let mut available_venues: Vec<_> = self
            .dark_pools
            .iter()
            .filter(|venue| {
                venue.is_available
                    && total_quantity >= venue.min_size
                    && total_quantity <= venue.max_size
            })
            .collect();

        if available_venues.is_empty() {
            return allocations;
        }

        // Sort venues by preference and performance
        available_venues.sort_by(|a, b| {
            let a_pref = config
                .preferred_venues
                .iter()
                .position(|&x| x == a.venue_type)
                .unwrap_or(100);
            let b_pref = config
                .preferred_venues
                .iter()
                .position(|&x| x == b.venue_type)
                .unwrap_or(100);

            if config.enable_smart_routing {
                let a_score = self.calculate_venue_score(a, config);
                let b_score = self.calculate_venue_score(b, config);
                a_score
                    .partial_cmp(&b_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .reverse()
            } else {
                a_pref.cmp(&b_pref)
            }
        });

        // Allocate quantity across top venues
        let num_venues = config.max_venues.min(available_venues.len());
        for venue in available_venues.iter().take(num_venues) {
            if remaining_quantity <= Decimal::ZERO {
                break;
            }

            let fill_probability = self
                .performance_tracker
                .get_fill_probability(venue.venue_type);
            if fill_probability < config.fill_probability_threshold {
                continue;
            }

            // Calculate allocation based on venue capacity and performance
            let base_allocation =
                remaining_quantity / Decimal::from(num_venues - allocations.len());
            let performance_multiplier = Decimal::from_f64(fill_probability).unwrap();
            let venue_allocation = (base_allocation * performance_multiplier)
                .min(remaining_quantity)
                .min(venue.max_size)
                .max(venue.min_size);

            if venue_allocation >= config.min_dark_size {
                allocations.push(((*venue).clone(), venue_allocation));
                remaining_quantity -= venue_allocation;
            }
        }

        debug!(
            allocations = allocations.len(),
            total_dark_quantity = %(max_dark_quantity - remaining_quantity),
            remaining_for_lit = %remaining_quantity,
            "Calculated dark pool routing allocation"
        );

        allocations
    }

    /// Calculate venue scoring for smart routing
    fn calculate_venue_score(&self, venue: &DarkPoolVenue, config: &DarkPoolConfig) -> f64 {
        let fill_prob = self
            .performance_tracker
            .get_fill_probability(venue.venue_type);
        let latency_penalty = venue.avg_latency_ms * config.latency_penalty;
        let historical_fill_rate = venue.fill_rate;

        // Composite score balancing fill probability, latency, and historical performance
        let score = fill_prob * 0.5 + historical_fill_rate * 0.3 - (latency_penalty / 100.0) * 0.2;

        debug!(
            venue_type = ?venue.venue_type,
            fill_prob = fill_prob,
            latency = venue.avg_latency_ms,
            score = score,
            "Calculated venue score"
        );

        score
    }

    /// Execute dark pool routing strategy
    pub async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: DarkPoolConfig,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        let total_quantity = request.state.quantity;
        let mut results = Vec::new();
        let start_time = Instant::now();

        info!(
            total_quantity = %total_quantity,
            max_dark_allocation = config.max_dark_allocation,
            instrument = %request.key.instrument,
            side = ?request.state.side,
            "Starting dark pool routing execution"
        );

        // Calculate routing allocation
        let allocations = self.calculate_routing_allocation(total_quantity, &config);

        if allocations.is_empty() {
            warn!("No suitable dark pool venues found, routing to lit markets");
            // Fall back to lit market execution
            let lit_request = OrderRequestOpen {
                key: OrderKey {
                    exchange: request.key.exchange,
                    instrument: request.key.instrument.clone(),
                    strategy: request.key.strategy.clone(),
                    cid: ClientOrderId::new(format!("{}_lit_fallback", request.key.cid)),
                },
                state: request.state.clone(),
            };

            let result = self.client.clone().open_order(lit_request).await;
            results.push(result.map_instrument(|inst_ref| inst_ref.clone()));
            return results;
        }

        // Execute dark pool orders
        for (i, (venue, quantity)) in allocations.iter().enumerate() {
            let dark_request = OrderRequestOpen {
                key: OrderKey {
                    exchange: venue.exchange_id,
                    instrument: request.key.instrument.clone(),
                    strategy: request.key.strategy.clone(),
                    cid: ClientOrderId::new(format!(
                        "{}_dark_{}_{:?}",
                        request.key.cid, i, venue.venue_type
                    )),
                },
                state: RequestOpen {
                    side: request.state.side,
                    price: self
                        .calculate_dark_price(&request.state.side)
                        .unwrap_or(request.state.price),
                    quantity: *quantity,
                    kind: self.select_dark_order_type(venue),
                    time_in_force: request.state.time_in_force,
                },
            };

            debug!(
                venue_type = ?venue.venue_type,
                quantity = %quantity,
                order_number = i + 1,
                "Placing dark pool order"
            );

            let execution_start = Instant::now();
            let result = self.client.clone().open_order(dark_request).await;
            let execution_time = execution_start.elapsed();

            // Record performance
            let execution_result = ExecutionResult {
                filled: result.state.is_ok(),
                quantity_filled: if result.state.is_ok() {
                    *quantity
                } else {
                    Decimal::ZERO
                },
                fill_time: execution_time,
                price_improvement: Decimal::ZERO, // Would be calculated from actual fill price
            };

            self.performance_tracker
                .record_execution(venue.venue_type, execution_result);

            results.push(result.map_instrument(|inst_ref| inst_ref.clone()));
        }

        // Wait for dark pool timeout, then route remainder to lit markets
        sleep(config.dark_timeout).await;

        // Calculate unfilled quantity (simplified - in reality you'd track actual fills)
        let dark_quantity: Decimal = allocations.iter().map(|(_, qty)| *qty).sum();
        let remaining_quantity = total_quantity - dark_quantity;

        if remaining_quantity > Decimal::ZERO {
            info!(
                remaining_quantity = %remaining_quantity,
                "Routing remainder to lit markets after dark pool timeout"
            );

            let lit_request = OrderRequestOpen {
                key: OrderKey {
                    exchange: request.key.exchange,
                    instrument: request.key.instrument.clone(),
                    strategy: request.key.strategy.clone(),
                    cid: ClientOrderId::new(format!("{}_lit_remainder", request.key.cid)),
                },
                state: RequestOpen {
                    side: request.state.side,
                    price: request.state.price,
                    quantity: remaining_quantity,
                    kind: request.state.kind,
                    time_in_force: request.state.time_in_force,
                },
            };

            let result = self.client.clone().open_order(lit_request).await;
            results.push(result.map_instrument(|inst_ref| inst_ref.clone()));
        }

        info!(
            orders_placed = results.len(),
            dark_orders = allocations.len(),
            total_quantity = %total_quantity,
            execution_time = ?start_time.elapsed(),
            "Completed dark pool routing execution"
        );

        results
    }

    /// Calculate appropriate price for dark pool orders
    fn calculate_dark_price(&self, _side: &Side) -> Option<Decimal> {
        if let (Some((_, bid)), Some((_, ask))) =
            (self.aggregator.best_bid(), self.aggregator.best_ask())
        {
            let mid_price = (bid + ask) / Decimal::TWO;
            Some(mid_price) // Most dark pools match at midpoint
        } else {
            None // Market order if no price reference
        }
    }

    /// Select appropriate order type for dark pool venue
    fn select_dark_order_type(&self, venue: &DarkPoolVenue) -> OrderKind {
        // Simplified selection based on venue capabilities
        if venue.supported_types.contains(&"MIDPOINT".to_string()) {
            OrderKind::Limit // Midpoint matching
        } else if venue.supported_types.contains(&"BLOCK".to_string()) {
            OrderKind::Limit // Block trading
        } else {
            OrderKind::Limit // Default to limit
        }
    }
}

#[async_trait]
impl<C, R> OrderExecutionStrategy for DarkPoolRouter<C, R>
where
    C: ExecutionClient + Clone + Send + Sync,
    R: Rng + Clone + Send + Sync,
{
    type Config = DarkPoolConfig;

    async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: Self::Config,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        DarkPoolRouter::execute(self, request, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_tracker() {
        let mut tracker = PerformanceTracker::new();

        let result = ExecutionResult {
            filled: true,
            quantity_filled: Decimal::from(1000),
            fill_time: Duration::from_secs(10),
            price_improvement: Decimal::from_str("0.001").unwrap(),
        };

        tracker.record_execution(DarkPoolType::InstitutionalCrossing, result);

        let fill_prob = tracker.get_fill_probability(DarkPoolType::InstitutionalCrossing);
        assert!(fill_prob > 0.0);
    }

    #[test]
    fn test_dark_pool_config_defaults() {
        let config = DarkPoolConfig::default();
        assert_eq!(config.max_dark_allocation, 0.70);
        assert_eq!(config.max_venues, 4);
        assert!(config.enable_smart_routing);
    }

    #[test]
    fn test_venue_stats() {
        let stats = VenueStats {
            total_orders: 100,
            filled_orders: 75,
            total_quantity: Decimal::from(1000000),
            filled_quantity: Decimal::from(750000),
            avg_fill_time: Duration::from_secs(15),
            avg_price_improvement: Decimal::from_str("0.0005").unwrap(),
        };

        let fill_rate = stats.filled_orders as f64 / stats.total_orders as f64;
        assert_eq!(fill_rate, 0.75);
    }
}
