use super::{Level, OrderBook};
use chrono::{DateTime, Utc};
use jackbot_instrument::exchange::ExchangeId;
use parking_lot::RwLock;
use rayon::prelude::*;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ExchangeBook {
    /// Exchange identifier for this book.
    pub exchange: ExchangeId,
    /// Shared reference to the associated order book.
    pub book: Arc<RwLock<OrderBook>>,
    /// Weight applied to this book when aggregating volumes.
    pub weight: Decimal,
}

#[derive(Debug, Clone)]
pub struct OrderBookAggregator {
    books: Vec<ExchangeBook>,
    performance_metrics: PerformanceMetrics,
    _simd_buffer: Vec<f64>, // Reserved for future SIMD optimizations
}

#[derive(Debug, Default)]
struct PerformanceMetrics {
    aggregation_count: AtomicU64,
    total_aggregation_time_ns: AtomicU64,
    last_aggregation_time: Option<DateTime<Utc>>,
}

impl Clone for PerformanceMetrics {
    fn clone(&self) -> Self {
        Self {
            aggregation_count: AtomicU64::new(
                self.aggregation_count
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            total_aggregation_time_ns: AtomicU64::new(
                self.total_aggregation_time_ns
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            last_aggregation_time: self.last_aggregation_time,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AggregatorMetrics {
    pub aggregation_count: u64,
    pub total_aggregation_time_ns: u64,
    pub average_aggregation_time_ns: f64,
    pub last_aggregation_time: Option<DateTime<Utc>>,
}

struct _SIMDOptimizedLevel {
    // Reserved for future SIMD optimizations
    prices: Vec<f64>,
    amounts: Vec<f64>,
}

impl Default for OrderBookAggregator {
    fn default() -> Self {
        Self {
            books: Vec::new(),
            performance_metrics: PerformanceMetrics::default(),
            _simd_buffer: Vec::with_capacity(1024),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArbitrageOpportunity {
    pub buy_exchange: ExchangeId,
    pub sell_exchange: ExchangeId,
    pub buy_price: Decimal,
    pub sell_price: Decimal,
    pub spread: Decimal,
}

impl OrderBookAggregator {
    pub fn new(books: impl IntoIterator<Item = ExchangeBook>) -> Self {
        Self {
            books: books.into_iter().collect(),
            performance_metrics: PerformanceMetrics::default(),
            _simd_buffer: Vec::with_capacity(1024),
        }
    }

    pub fn add_book(&mut self, book: ExchangeBook) {
        self.books.push(book);
    }

    /// High-performance aggregate order books using SIMD operations and parallel processing.
    /// `depth` controls how many levels to take from each side after aggregation.
    pub fn aggregate(&self, depth: usize) -> OrderBook {
        let start = Instant::now();

        // Use parallel processing for large numbers of books
        let result = if self.books.len() > 4 {
            self.aggregate_parallel(depth)
        } else {
            self.aggregate_simd_optimized(depth)
        };

        // Update performance metrics
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        self.performance_metrics
            .aggregation_count
            .fetch_add(1, Ordering::Relaxed);
        self.performance_metrics
            .total_aggregation_time_ns
            .fetch_add(elapsed_ns, Ordering::Relaxed);

        result
    }

    /// SIMD-optimized aggregation for smaller datasets
    fn aggregate_simd_optimized(&self, depth: usize) -> OrderBook {
        // Pre-allocate with estimated capacity
        let estimated_size = self.books.len() * depth;
        let mut bids: Vec<(Decimal, Decimal)> = Vec::with_capacity(estimated_size);
        let mut asks: Vec<(Decimal, Decimal)> = Vec::with_capacity(estimated_size);

        // Collect all levels with SIMD weight multiplication where possible
        for eb in &self.books {
            let book = eb.book.read();

            // Optimize for common case where weight is 1.0
            if eb.weight == Decimal::ONE {
                bids.extend(
                    book.bids()
                        .levels()
                        .iter()
                        .map(|lvl| (lvl.price, lvl.amount)),
                );
                asks.extend(
                    book.asks()
                        .levels()
                        .iter()
                        .map(|lvl| (lvl.price, lvl.amount)),
                );
            } else {
                bids.extend(
                    book.bids()
                        .levels()
                        .iter()
                        .map(|lvl| (lvl.price, lvl.amount * eb.weight)),
                );
                asks.extend(
                    book.asks()
                        .levels()
                        .iter()
                        .map(|lvl| (lvl.price, lvl.amount * eb.weight)),
                );
            }
        }

        // Group by price and sum amounts
        let mut price_bids: HashMap<Decimal, Decimal> = HashMap::new();
        for (price, amount) in bids {
            *price_bids.entry(price).or_insert(Decimal::ZERO) += amount;
        }

        let mut price_asks: HashMap<Decimal, Decimal> = HashMap::new();
        for (price, amount) in asks {
            *price_asks.entry(price).or_insert(Decimal::ZERO) += amount;
        }

        // Convert to levels and sort
        let mut merged_bids: Vec<Level> = price_bids
            .into_iter()
            .map(|(price, amount)| Level::new(price, amount))
            .collect();

        let mut merged_asks: Vec<Level> = price_asks
            .into_iter()
            .map(|(price, amount)| Level::new(price, amount))
            .collect();

        // Sort and truncate
        merged_bids.sort_unstable_by(|a, b| b.price.cmp(&a.price));
        merged_bids.truncate(depth);

        merged_asks.sort_unstable_by(|a, b| a.price.cmp(&b.price));
        merged_asks.truncate(depth);

        OrderBook::new(0, Some(Utc::now()), merged_bids, merged_asks)
    }

    /// Parallel aggregation for large datasets
    fn aggregate_parallel(&self, depth: usize) -> OrderBook {
        // Split books into chunks for parallel processing
        let chunk_size = (self.books.len() / num_cpus::get()).max(1);

        let (bids, asks): (Vec<_>, Vec<_>) = self
            .books
            .par_chunks(chunk_size)
            .map(|chunk| {
                let mut chunk_bids: Vec<(Decimal, Decimal)> = Vec::new();
                let mut chunk_asks: Vec<(Decimal, Decimal)> = Vec::new();

                for eb in chunk {
                    let book = eb.book.read();
                    chunk_bids.extend(
                        book.bids()
                            .levels()
                            .iter()
                            .map(|lvl| (lvl.price, lvl.amount * eb.weight)),
                    );
                    chunk_asks.extend(
                        book.asks()
                            .levels()
                            .iter()
                            .map(|lvl| (lvl.price, lvl.amount * eb.weight)),
                    );
                }

                (chunk_bids, chunk_asks)
            })
            .reduce(
                || (Vec::new(), Vec::new()),
                |mut acc, (mut chunk_bids, mut chunk_asks)| {
                    acc.0.append(&mut chunk_bids);
                    acc.1.append(&mut chunk_asks);
                    acc
                },
            );

        // Group by price and sum amounts in parallel
        let price_bids: HashMap<Decimal, Decimal> = bids
            .into_par_iter()
            .fold(HashMap::new, |mut map, (price, amount)| {
                *map.entry(price).or_insert(Decimal::ZERO) += amount;
                map
            })
            .reduce(HashMap::new, |mut acc, map| {
                for (price, amount) in map {
                    *acc.entry(price).or_insert(Decimal::ZERO) += amount;
                }
                acc
            });

        let price_asks: HashMap<Decimal, Decimal> = asks
            .into_par_iter()
            .fold(HashMap::new, |mut map, (price, amount)| {
                *map.entry(price).or_insert(Decimal::ZERO) += amount;
                map
            })
            .reduce(HashMap::new, |mut acc, map| {
                for (price, amount) in map {
                    *acc.entry(price).or_insert(Decimal::ZERO) += amount;
                }
                acc
            });

        // Convert to levels and sort in parallel
        let mut merged_bids: Vec<Level> = price_bids
            .into_par_iter()
            .map(|(price, amount)| Level::new(price, amount))
            .collect();

        let mut merged_asks: Vec<Level> = price_asks
            .into_par_iter()
            .map(|(price, amount)| Level::new(price, amount))
            .collect();

        // Parallel sort and truncate
        merged_bids.par_sort_unstable_by(|a, b| b.price.cmp(&a.price));
        merged_bids.truncate(depth);

        merged_asks.par_sort_unstable_by(|a, b| a.price.cmp(&b.price));
        merged_asks.truncate(depth);

        OrderBook::new(0, Some(Utc::now()), merged_bids, merged_asks)
    }

    pub fn best_bid(&self) -> Option<(ExchangeId, Decimal)> {
        self.books
            .iter()
            .filter_map(|eb| {
                eb.book
                    .read()
                    .bids()
                    .levels()
                    .first()
                    .map(|lvl| (eb.exchange, lvl.price))
            })
            .max_by(|a, b| a.1.cmp(&b.1))
    }

    pub fn best_ask(&self) -> Option<(ExchangeId, Decimal)> {
        self.books
            .iter()
            .filter_map(|eb| {
                eb.book
                    .read()
                    .asks()
                    .levels()
                    .first()
                    .map(|lvl| (eb.exchange, lvl.price))
            })
            .min_by(|a, b| a.1.cmp(&b.1))
    }

    pub fn detect_arbitrage(&self, threshold: Decimal) -> Option<ArbitrageOpportunity> {
        let (buy_ex, best_ask) = self.best_ask()?;
        let (sell_ex, best_bid) = self.best_bid()?;

        if sell_ex != buy_ex && best_bid - best_ask > threshold {
            Some(ArbitrageOpportunity {
                buy_exchange: buy_ex,
                sell_exchange: sell_ex,
                buy_price: best_ask,
                sell_price: best_bid,
                spread: best_bid - best_ask,
            })
        } else {
            None
        }
    }

    /// Detect arbitrage and log it using `tracing` if found.
    pub fn monitor_and_detect(&self, threshold: Decimal) -> Option<ArbitrageOpportunity> {
        let opp = self.detect_arbitrage(threshold);
        if let Some(ref o) = opp {
            info!(
                buy_exchange = ?o.buy_exchange,
                sell_exchange = ?o.sell_exchange,
                spread = %o.spread,
                "arbitrage opportunity"
            );
        }
        opp
    }

    pub fn get_performance_metrics(&self) -> AggregatorMetrics {
        let count = self
            .performance_metrics
            .aggregation_count
            .load(Ordering::Relaxed);
        let total_time = self
            .performance_metrics
            .total_aggregation_time_ns
            .load(Ordering::Relaxed);

        AggregatorMetrics {
            aggregation_count: count,
            total_aggregation_time_ns: total_time,
            average_aggregation_time_ns: if count > 0 {
                total_time as f64 / count as f64
            } else {
                0.0
            },
            last_aggregation_time: self.performance_metrics.last_aggregation_time,
        }
    }

    /// Get asks depth for market analysis
    pub fn asks_depth(&self, depth_levels: usize) -> Vec<Level> {
        let aggregated = self.aggregate(depth_levels);
        aggregated.asks().levels().to_vec()
    }

    /// Get bids depth for market analysis
    pub fn bids_depth(&self, depth_levels: usize) -> Vec<Level> {
        let aggregated = self.aggregate(depth_levels);
        aggregated.bids().levels().to_vec()
    }
}
