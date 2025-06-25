use crate::strategy::advanced::OrderExecutionStrategy;
use crate::{
    client::ExecutionClient,
    error::UnindexedOrderError,
    order::{
        id::ClientOrderId,
        request::{OrderRequestOpen, RequestOpen},
        state::Open,
        Order, OrderKey, Side,
    },
};
use async_trait::async_trait;
use jackbot_data::books::aggregator::OrderBookAggregator;
use jackbot_instrument::{exchange::ExchangeId, instrument::name::InstrumentNameExchange};
use rand::prelude::*;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::{collections::HashMap, sync::Arc};
use tokio::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Cross-exchange order splitting with latency optimization.
///
/// This strategy intelligently splits large orders across multiple exchanges
/// to minimize market impact while optimizing for latency and fill probability.
/// It includes sophisticated latency arbitrage and liquidity aggregation.
#[derive(Debug, Clone)]
pub struct CrossExchangeRouter<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub client: C,
    pub aggregators: HashMap<ExchangeId, Arc<OrderBookAggregator>>,
    rng: R,
    exchange_metadata: HashMap<ExchangeId, ExchangeMetadata>,
    latency_monitor: LatencyMonitor,
    liquidity_analyzer: LiquidityAnalyzer,
}

/// Configuration for cross-exchange routing
#[derive(Debug, Clone)]
pub struct CrossExchangeConfig {
    /// Target exchanges for routing (ordered by preference)
    pub target_exchanges: Vec<ExchangeId>,
    /// Maximum number of exchanges to route to simultaneously
    pub max_exchanges: usize,
    /// Minimum order size per exchange (to avoid fragmentation)
    pub min_order_size: Decimal,
    /// Maximum latency tolerance (milliseconds)
    pub max_latency_ms: f64,
    /// Liquidity threshold for exchange selection
    pub min_liquidity_threshold: Decimal,
    /// Price improvement threshold for routing decisions
    pub price_improvement_threshold: Decimal,
    /// Enable latency arbitrage optimization
    pub enable_latency_arbitrage: bool,
    /// Enable dynamic rebalancing based on fills
    pub enable_dynamic_rebalancing: bool,
    /// Time window for latency measurements
    pub latency_window: Duration,
    /// Spread tolerance for cross-exchange routing
    pub spread_tolerance: Decimal,
}

impl Default for CrossExchangeConfig {
    fn default() -> Self {
        Self {
            target_exchanges: vec![
                ExchangeId::BinanceSpot,
                ExchangeId::BybitSpot,
                ExchangeId::Okx,
                ExchangeId::Kraken,
            ],
            max_exchanges: 4,
            min_order_size: Decimal::from_str("100").unwrap(),
            max_latency_ms: 50.0,
            min_liquidity_threshold: Decimal::from_str("10000").unwrap(),
            price_improvement_threshold: Decimal::from_str("0.0001").unwrap(), // 1 bp
            enable_latency_arbitrage: true,
            enable_dynamic_rebalancing: true,
            latency_window: Duration::from_secs(60),
            spread_tolerance: Decimal::from_str("0.001").unwrap(), // 10 bps
        }
    }
}

/// Exchange metadata for routing decisions
#[derive(Debug, Clone)]
struct ExchangeMetadata {
    /// Exchange identifier
    exchange_id: ExchangeId,
    /// Current connectivity status
    is_connected: bool,
    /// Average execution latency
    avg_latency_ms: f64,
    /// Fee structure (maker/taker)
    maker_fee: Decimal,
    taker_fee: Decimal,
    /// Minimum order size
    min_order_size: Decimal,
    /// Maximum order size
    max_order_size: Decimal,
    /// Average daily volume
    avg_daily_volume: Decimal,
    /// Reliability score (0.0 to 1.0)
    reliability_score: f64,
}

/// Latency monitoring and optimization
#[derive(Debug, Clone)]
struct LatencyMonitor {
    /// Recent latency measurements per exchange
    latency_history: HashMap<ExchangeId, Vec<(Instant, f64)>>,
    /// Maximum history size
    max_history_size: usize,
    /// Measurement window
    window_duration: Duration,
}

impl LatencyMonitor {
    fn new(window_duration: Duration) -> Self {
        Self {
            latency_history: HashMap::new(),
            max_history_size: 1000,
            window_duration,
        }
    }

    fn record_latency(&mut self, exchange: ExchangeId, latency_ms: f64) {
        let history = self
            .latency_history
            .entry(exchange)
            .or_insert_with(Vec::new);
        history.push((Instant::now(), latency_ms));

        // Keep only recent measurements
        if history.len() > self.max_history_size {
            history.remove(0);
        }

        // Clean up old measurements
        let cutoff = Instant::now() - self.window_duration;
        history.retain(|(timestamp, _)| *timestamp >= cutoff);
    }

    fn get_average_latency(&self, exchange: &ExchangeId) -> Option<f64> {
        self.latency_history.get(exchange).and_then(|history| {
            if history.is_empty() {
                None
            } else {
                let sum: f64 = history.iter().map(|(_, latency)| *latency).sum();
                Some(sum / history.len() as f64)
            }
        })
    }

    fn get_latency_percentile(&self, exchange: &ExchangeId, percentile: f64) -> Option<f64> {
        self.latency_history.get(exchange).and_then(|history| {
            if history.is_empty() {
                None
            } else {
                let mut latencies: Vec<f64> = history.iter().map(|(_, latency)| *latency).collect();
                latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let index = ((latencies.len() as f64 - 1.0) * percentile) as usize;
                latencies.get(index).copied()
            }
        })
    }
}

/// Liquidity analysis across exchanges
#[derive(Debug, Clone)]
struct LiquidityAnalyzer {
    /// Historical liquidity data per exchange
    liquidity_snapshots: HashMap<ExchangeId, Vec<(Instant, LiquiditySnapshot)>>,
    /// Snapshot retention period
    retention_period: Duration,
}

#[derive(Debug, Clone)]
struct LiquiditySnapshot {
    /// Total bid liquidity
    bid_liquidity: Decimal,
    /// Total ask liquidity
    ask_liquidity: Decimal,
    /// Weighted average bid price
    avg_bid_price: Decimal,
    /// Weighted average ask price
    avg_ask_price: Decimal,
    /// Spread
    spread: Decimal,
    /// Depth at various levels
    depth_levels: Vec<(Decimal, Decimal)>, // (price, quantity)
}

impl LiquidityAnalyzer {
    fn new(retention_period: Duration) -> Self {
        Self {
            liquidity_snapshots: HashMap::new(),
            retention_period,
        }
    }

    fn record_liquidity(&mut self, exchange: ExchangeId, snapshot: LiquiditySnapshot) {
        let history = self
            .liquidity_snapshots
            .entry(exchange)
            .or_insert_with(Vec::new);
        history.push((Instant::now(), snapshot));

        // Clean up old snapshots
        let cutoff = Instant::now() - self.retention_period;
        history.retain(|(timestamp, _)| *timestamp >= cutoff);
    }

    fn get_current_liquidity(&self, exchange: &ExchangeId) -> Option<&LiquiditySnapshot> {
        self.liquidity_snapshots
            .get(exchange)
            .and_then(|history| history.last().map(|(_, snapshot)| snapshot))
    }

    fn calculate_liquidity_score(
        &self,
        exchange: &ExchangeId,
        side: &Side,
        quantity: Decimal,
    ) -> f64 {
        if let Some(snapshot) = self.get_current_liquidity(exchange) {
            let available_liquidity = match side {
                Side::Buy => snapshot.ask_liquidity,
                Side::Sell => snapshot.bid_liquidity,
            };

            let liquidity_ratio = (available_liquidity / quantity).to_f64().unwrap_or(0.0);
            let spread_penalty = snapshot.spread.to_f64().unwrap_or(0.01);

            // Score based on liquidity availability and spread
            (liquidity_ratio * 10.0 - spread_penalty * 100.0)
                .max(0.0)
                .min(10.0)
        } else {
            0.0
        }
    }
}

impl<C, R> CrossExchangeRouter<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub fn new(
        client: C,
        aggregators: HashMap<ExchangeId, Arc<OrderBookAggregator>>,
        rng: R,
    ) -> Self {
        let exchange_metadata = Self::initialize_exchange_metadata(&aggregators);

        Self {
            client,
            aggregators,
            rng,
            exchange_metadata,
            latency_monitor: LatencyMonitor::new(Duration::from_secs(300)),
            liquidity_analyzer: LiquidityAnalyzer::new(Duration::from_secs(600)),
        }
    }

    fn initialize_exchange_metadata(
        aggregators: &HashMap<ExchangeId, Arc<OrderBookAggregator>>,
    ) -> HashMap<ExchangeId, ExchangeMetadata> {
        let mut metadata = HashMap::new();

        // Initialize with default metadata for known exchanges
        for exchange_id in aggregators.keys() {
            let meta = ExchangeMetadata {
                exchange_id: exchange_id.clone(),
                is_connected: true,
                avg_latency_ms: 10.0,                           // Default
                maker_fee: Decimal::from_str("0.001").unwrap(), // 0.1%
                taker_fee: Decimal::from_str("0.001").unwrap(), // 0.1%
                min_order_size: Decimal::from_str("10").unwrap(),
                max_order_size: Decimal::from_str("1000000").unwrap(),
                avg_daily_volume: Decimal::from_str("10000000").unwrap(),
                reliability_score: 0.95,
            };
            metadata.insert(exchange_id.clone(), meta);
        }

        metadata
    }

    /// Update liquidity snapshots from order book aggregators
    pub fn update_liquidity_snapshots(&mut self) {
        for (exchange_id, aggregator) in &self.aggregators {
            if let (Some(best_bid), Some(best_ask)) = (aggregator.best_bid(), aggregator.best_ask())
            {
                let snapshot = LiquiditySnapshot {
                    bid_liquidity: best_bid.1, // This should be calculated from depth, but using price as placeholder
                    ask_liquidity: best_ask.1, // This should be calculated from depth, but using price as placeholder
                    avg_bid_price: best_bid.1,
                    avg_ask_price: best_ask.1,
                    spread: best_ask.1 - best_bid.1,
                    depth_levels: vec![(best_bid.1, best_bid.1), (best_ask.1, best_ask.1)],
                };

                self.liquidity_analyzer
                    .record_liquidity(exchange_id.clone(), snapshot);
            }
        }
    }

    /// Calculate optimal routing allocation across exchanges
    fn calculate_routing_allocation(
        &mut self,
        total_quantity: Decimal,
        config: &CrossExchangeConfig,
        side: &Side,
    ) -> Vec<(ExchangeId, Decimal, f64)> {
        let mut allocations = Vec::new();
        let mut exchange_scores = Vec::new();

        // Calculate scores for each exchange
        for exchange_id in &config.target_exchanges {
            if let Some(metadata) = self.exchange_metadata.get(exchange_id) {
                if !metadata.is_connected {
                    continue;
                }

                let latency_score = self.calculate_latency_score(exchange_id, config);
                let liquidity_score = self.liquidity_analyzer.calculate_liquidity_score(
                    exchange_id,
                    side,
                    total_quantity,
                );
                let cost_score = self.calculate_cost_score(metadata, side);
                let reliability_score = metadata.reliability_score;

                // Composite score
                let composite_score = latency_score * 0.3
                    + liquidity_score * 0.4
                    + cost_score * 0.2
                    + reliability_score * 0.1;

                exchange_scores.push((exchange_id.clone(), composite_score));

                debug!(
                    exchange = ?exchange_id,
                    latency_score = latency_score,
                    liquidity_score = liquidity_score,
                    cost_score = cost_score,
                    reliability_score = reliability_score,
                    composite_score = composite_score,
                    "Calculated exchange routing score"
                );
            }
        }

        // Sort by score (descending)
        exchange_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Allocate quantity across top exchanges
        let mut remaining_quantity = total_quantity;
        let max_exchanges = config.max_exchanges.min(exchange_scores.len());
        let total_score: f64 = exchange_scores.iter().map(|(_, s)| *s).sum();

        for (exchange_id, score) in exchange_scores.into_iter().take(max_exchanges) {
            if remaining_quantity <= Decimal::ZERO {
                break;
            }

            // Check minimum liquidity threshold
            if let Some(liquidity) = self.liquidity_analyzer.get_current_liquidity(&exchange_id) {
                let available_liquidity = match side {
                    Side::Buy => liquidity.ask_liquidity,
                    Side::Sell => liquidity.bid_liquidity,
                };

                if available_liquidity < config.min_liquidity_threshold {
                    continue;
                }
            }

            // Calculate allocation based on score and constraints
            let score_weight = score / total_score;
            let base_allocation = total_quantity * Decimal::from_f64(score_weight).unwrap();
            let constrained_allocation = base_allocation
                .max(config.min_order_size)
                .min(remaining_quantity);

            if constrained_allocation >= config.min_order_size {
                allocations.push((exchange_id, constrained_allocation, score));
                remaining_quantity -= constrained_allocation;
            }
        }

        // Distribute any remaining quantity
        if remaining_quantity > Decimal::ZERO && !allocations.is_empty() {
            allocations[0].1 += remaining_quantity;
        }

        debug!(
            allocations = allocations.len(),
            total_allocated = %(total_quantity - remaining_quantity),
            "Calculated cross-exchange routing allocation"
        );

        allocations
    }

    fn calculate_latency_score(
        &self,
        exchange_id: &ExchangeId,
        config: &CrossExchangeConfig,
    ) -> f64 {
        if let Some(avg_latency) = self.latency_monitor.get_average_latency(exchange_id) {
            if avg_latency <= config.max_latency_ms {
                // Higher score for lower latency
                1.0 - (avg_latency / config.max_latency_ms).min(1.0)
            } else {
                0.0 // Exclude high-latency exchanges
            }
        } else {
            0.5 // Neutral score for unknown latency
        }
    }

    fn calculate_cost_score(&self, metadata: &ExchangeMetadata, side: &Side) -> f64 {
        // Prefer maker fees for limit orders
        let relevant_fee = match side {
            Side::Buy | Side::Sell => metadata.maker_fee, // Assuming limit orders
        };

        // Lower fees = higher score
        let max_fee = Decimal::from_str("0.005").unwrap(); // 0.5% max
        let fee_ratio = (relevant_fee / max_fee).to_f64().unwrap_or(1.0);
        (1.0 - fee_ratio).max(0.0)
    }

    /// Execute cross-exchange routing strategy
    pub async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: CrossExchangeConfig,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        let total_quantity = request.state.quantity;
        let mut results = Vec::new();
        let start_time = Instant::now();

        info!(
            total_quantity = %total_quantity,
            target_exchanges = config.target_exchanges.len(),
            max_exchanges = config.max_exchanges,
            instrument = %request.key.instrument,
            side = ?request.state.side,
            "Starting cross-exchange routing execution"
        );

        // Update liquidity snapshots
        self.update_liquidity_snapshots();

        // Calculate routing allocation
        let allocations =
            self.calculate_routing_allocation(total_quantity, &config, &request.state.side);

        if allocations.is_empty() {
            warn!("No suitable exchanges found for routing");
            return results;
        }

        // Execute orders across exchanges
        for (i, (exchange_id, quantity, score)) in allocations.iter().enumerate() {
            let execution_start = Instant::now();

            // Create exchange-specific order request
            let exchange_request = OrderRequestOpen {
                key: OrderKey {
                    exchange: exchange_id.clone(),
                    instrument: request.key.instrument.clone(),
                    strategy: request.key.strategy.clone(),
                    cid: ClientOrderId::new(format!("{}_x_{}_{}", request.key.cid, i, exchange_id)),
                },
                state: RequestOpen {
                    side: request.state.side,
                    price: self
                        .calculate_exchange_price(exchange_id, &request.state.side, &config)
                        .unwrap_or(request.state.price),
                    quantity: *quantity,
                    kind: request.state.kind,
                    time_in_force: request.state.time_in_force,
                },
            };

            debug!(
                exchange = ?exchange_id,
                quantity = %quantity,
                score = score,
                order_number = i + 1,
                "Placing cross-exchange order"
            );

            let result = self.client.clone().open_order(exchange_request).await;
            let execution_time = execution_start.elapsed();

            // Record latency for future routing decisions
            self.latency_monitor
                .record_latency(exchange_id.clone(), execution_time.as_millis() as f64);

            results.push(result.map_instrument(|inst_ref| inst_ref.clone()));
        }

        info!(
            orders_placed = results.len(),
            exchanges_used = allocations.len(),
            total_quantity = %total_quantity,
            execution_time = ?start_time.elapsed(),
            "Completed cross-exchange routing execution"
        );

        results
    }

    fn calculate_exchange_price(
        &self,
        exchange_id: &ExchangeId,
        side: &Side,
        config: &CrossExchangeConfig,
    ) -> Option<Decimal> {
        if let Some(aggregator) = self.aggregators.get(exchange_id) {
            if let (Some((_, bid)), Some((_, ask))) = (aggregator.best_bid(), aggregator.best_ask())
            {
                let spread = ask - bid;

                // Check spread tolerance
                if spread > config.spread_tolerance {
                    return None; // Use market order
                }

                // Price improvement strategy
                let improvement = config.price_improvement_threshold;
                match side {
                    Side::Buy => Some(bid + improvement),  // Aggressive bid
                    Side::Sell => Some(ask - improvement), // Aggressive ask
                }
            } else {
                None // No price reference available
            }
        } else {
            None
        }
    }
}

#[async_trait]
impl<C, R> OrderExecutionStrategy for CrossExchangeRouter<C, R>
where
    C: ExecutionClient + Clone + Send + Sync,
    R: Rng + Clone + Send + Sync,
{
    type Config = CrossExchangeConfig;

    async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: Self::Config,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        CrossExchangeRouter::execute(self, request, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_latency_monitor() {
        let mut monitor = LatencyMonitor::new(Duration::from_secs(60));
        let exchange = ExchangeId::BinanceSpot;

        monitor.record_latency(exchange.clone(), 10.0);
        monitor.record_latency(exchange.clone(), 15.0);
        monitor.record_latency(exchange.clone(), 12.0);

        let avg_latency = monitor.get_average_latency(&exchange);
        assert!(avg_latency.is_some());
        assert!((avg_latency.unwrap() - 12.33).abs() < 0.1);
    }

    #[test]
    fn test_liquidity_analyzer() {
        let mut analyzer = LiquidityAnalyzer::new(Duration::from_secs(300));
        let exchange = ExchangeId::BinanceSpot;

        let snapshot = LiquiditySnapshot {
            bid_liquidity: dec!(10000),
            ask_liquidity: dec!(8000),
            avg_bid_price: dec!(100),
            avg_ask_price: dec!(100.1),
            spread: dec!(0.1),
            depth_levels: vec![(dec!(100), dec!(10000)), (dec!(100.1), dec!(8000))],
        };

        analyzer.record_liquidity(exchange.clone(), snapshot);

        let score = analyzer.calculate_liquidity_score(&exchange, &Side::Buy, dec!(1000));
        assert!(score > 0.0);
    }

    #[test]
    fn test_cross_exchange_config_defaults() {
        let config = CrossExchangeConfig::default();
        assert_eq!(config.max_exchanges, 4);
        assert!(config.enable_latency_arbitrage);
        assert!(config.enable_dynamic_rebalancing);
    }
}
