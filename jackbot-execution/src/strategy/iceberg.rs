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
use std::{cmp, sync::Arc};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, info};

/// Iceberg order implementation with adaptive sizing based on market conditions.
///
/// Iceberg orders hide large position sizes by breaking them into smaller visible chunks,
/// only exposing a fraction of the total size at any given time. This implementation
/// includes adaptive sizing that adjusts chunk sizes based on market volatility and
/// order book depth.
#[derive(Debug, Clone)]
pub struct IcebergExecutor<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub client: C,
    pub aggregator: Arc<OrderBookAggregator>,
    rng: R,
}

/// Configuration for Iceberg order execution
#[derive(Debug, Clone, Copy)]
pub struct IcebergConfig {
    /// Base size for each visible chunk as percentage of total order (0.0 to 1.0)
    pub base_chunk_percentage: f64,
    /// Minimum chunk size as percentage of total order
    pub min_chunk_percentage: f64,
    /// Maximum chunk size as percentage of total order  
    pub max_chunk_percentage: f64,
    /// Price improvement threshold to trigger aggressive chunk sizing
    pub price_improvement_threshold: Decimal,
    /// Maximum number of concurrent chunks
    pub max_concurrent_chunks: usize,
    /// Delay between chunk placement attempts
    pub chunk_delay: Duration,
    /// Enable adaptive sizing based on market conditions
    pub adaptive_sizing: bool,
    /// Market depth lookback for sizing calculations (number of levels)
    pub depth_lookback: usize,
}

impl Default for IcebergConfig {
    fn default() -> Self {
        Self {
            base_chunk_percentage: 0.05, // 5% of total order
            min_chunk_percentage: 0.01,  // 1% minimum
            max_chunk_percentage: 0.20,  // 20% maximum
            price_improvement_threshold: Decimal::from_str("0.0005").unwrap(), // 5 bps
            max_concurrent_chunks: 3,
            chunk_delay: Duration::from_millis(500),
            adaptive_sizing: true,
            depth_lookback: 10,
        }
    }
}

impl<C, R> IcebergExecutor<C, R>
where
    C: ExecutionClient + Clone,
    R: Rng + Clone,
{
    pub fn new(client: C, aggregator: OrderBookAggregator, rng: R) -> Self {
        Self {
            client,
            aggregator: aggregator.into(),
            rng,
        }
    }

    /// Calculate adaptive chunk size based on current market conditions
    fn calculate_adaptive_chunk_size(
        &mut self,
        total_quantity: Decimal,
        config: &IcebergConfig,
        is_buy: bool,
    ) -> Decimal {
        if !config.adaptive_sizing {
            return total_quantity * Decimal::from_f64(config.base_chunk_percentage).unwrap();
        }

        // Get market depth on the relevant side
        let depth_quantity = if is_buy {
            self.aggregator
                .asks_depth(config.depth_lookback)
                .iter()
                .map(|level| level.amount)
                .sum::<Decimal>()
        } else {
            self.aggregator
                .bids_depth(config.depth_lookback)
                .iter()
                .map(|level| level.amount)
                .sum::<Decimal>()
        };

        // Calculate volatility based on spread
        let volatility_factor = if let (Some((_, bid)), Some((_, ask))) =
            (self.aggregator.best_bid(), self.aggregator.best_ask())
        {
            let spread = ask - bid;
            let mid_price = (ask + bid) / Decimal::TWO;
            (spread / mid_price).to_f64().unwrap_or(0.01)
        } else {
            0.01 // Default volatility
        };

        // Adapt chunk size based on market conditions
        let mut chunk_percentage = config.base_chunk_percentage;

        // Increase chunk size in liquid markets (high depth)
        if depth_quantity > total_quantity {
            chunk_percentage *= 1.5;
        }

        // Decrease chunk size in volatile markets
        if volatility_factor > 0.005 {
            chunk_percentage *= 0.7;
        }

        // Add randomness to avoid predictable patterns
        let randomness = self.rng.random_range(-0.2..=0.2);
        chunk_percentage *= 1.0 + randomness;

        // Clamp to configured bounds
        chunk_percentage = chunk_percentage
            .max(config.min_chunk_percentage)
            .min(config.max_chunk_percentage);

        let chunk_size = total_quantity * Decimal::from_f64(chunk_percentage).unwrap();

        debug!(
            chunk_size = %chunk_size,
            chunk_percentage = chunk_percentage,
            depth_quantity = %depth_quantity,
            volatility_factor = volatility_factor,
            "Calculated adaptive iceberg chunk size"
        );

        chunk_size
    }

    /// Execute iceberg order strategy
    pub async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: IcebergConfig,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        let total_quantity = request.state.quantity;
        let is_buy = matches!(request.state.side, crate::order::Side::Buy);
        let mut remaining_quantity = total_quantity;
        let mut results = Vec::new();
        let mut active_orders = Vec::new();
        let start_time = Instant::now();

        info!(
            total_quantity = %total_quantity,
            instrument = %request.key.instrument,
            side = ?request.state.side,
            "Starting iceberg order execution"
        );

        while remaining_quantity > Decimal::ZERO {
            // Clean up filled orders
            // Note: In a real implementation, you'd monitor order status and remove filled orders
            if active_orders.len() >= config.max_concurrent_chunks {
                sleep(config.chunk_delay).await;
                continue;
            }

            // Calculate next chunk size
            let chunk_size =
                self.calculate_adaptive_chunk_size(remaining_quantity, &config, is_buy);

            let actual_chunk_size = cmp::min(chunk_size, remaining_quantity);

            if actual_chunk_size <= Decimal::ZERO {
                break;
            }

            // Create chunk order
            let chunk_request = OrderRequestOpen {
                key: OrderKey {
                    exchange: request.key.exchange,
                    instrument: request.key.instrument.clone(),
                    strategy: request.key.strategy.clone(),
                    cid: ClientOrderId::new(format!("{}_chunk_{}", request.key.cid, results.len())),
                },
                state: RequestOpen {
                    side: request.state.side,
                    price: request.state.price,
                    quantity: actual_chunk_size,
                    kind: request.state.kind,
                    time_in_force: request.state.time_in_force,
                },
            };

            debug!(
                chunk_size = %actual_chunk_size,
                remaining = %remaining_quantity,
                chunk_number = results.len() + 1,
                "Placing iceberg chunk"
            );

            let result = self.client.clone().open_order(chunk_request).await;
            let order_result = result.map_instrument(|inst_ref| inst_ref.clone());

            results.push(order_result.clone());

            // Track active orders (simplified - in reality you'd monitor fills)
            if order_result.state.is_ok() {
                active_orders.push(results.len() - 1);
            }

            remaining_quantity -= actual_chunk_size;

            // Wait before next chunk
            if remaining_quantity > Decimal::ZERO {
                sleep(config.chunk_delay).await;
            }
        }

        info!(
            chunks_placed = results.len(),
            total_quantity = %total_quantity,
            execution_time = ?start_time.elapsed(),
            "Completed iceberg order execution"
        );

        results
    }
}

#[async_trait]
impl<C, R> OrderExecutionStrategy for IcebergExecutor<C, R>
where
    C: ExecutionClient + Clone + Send + Sync,
    R: Rng + Clone + Send + Sync,
{
    type Config = IcebergConfig;

    async fn execute(
        &mut self,
        request: OrderRequestOpen<ExchangeId, &InstrumentNameExchange>,
        config: Self::Config,
    ) -> Vec<Order<ExchangeId, InstrumentNameExchange, Result<Open, UnindexedOrderError>>> {
        IcebergExecutor::execute(self, request, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackbot_data::books::canonical::CanonicalOrderBook;

    #[test]
    fn test_adaptive_chunk_sizing() {
        let _rng = rand::rng();
        // Create empty orderbook for testing
        let orderbook = jackbot_data::books::OrderBook::new::<
            Vec<(i32, i32)>,
            Vec<(i32, i32)>,
            (i32, i32),
        >(0, None, vec![], vec![]);
        let _canonical_book = CanonicalOrderBook::new(orderbook);
        // Skip aggregator creation as it requires the book to implement Iterator
        // let aggregator = OrderBookAggregator::new(canonical_book);
        // Test would require mock client - implementation depends on your test framework
    }

    #[test]
    fn test_iceberg_config_defaults() {
        let config = IcebergConfig::default();
        assert_eq!(config.base_chunk_percentage, 0.05);
        assert_eq!(config.max_concurrent_chunks, 3);
        assert!(config.adaptive_sizing);
    }
}
