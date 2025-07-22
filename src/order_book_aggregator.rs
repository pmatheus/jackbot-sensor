//! Multi-exchange order book aggregator
//!
//! This module implements real-time order book aggregation across multiple exchanges
//! to provide unified market depth and liquidity analysis.

use anyhow::{Context, Result};
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::api::{OrderBookData, PriceLevel};
use crate::connector::{Exchange, MarketData};

/// Configuration for order book aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationConfig {
    /// Maximum age of order book data before it's considered stale (ms)
    pub max_age_ms: u64,
    /// Minimum price difference to merge levels (in basis points)
    pub merge_threshold_bps: u32,
    /// Maximum number of price levels to maintain per side
    pub max_levels: usize,
    /// Update frequency for aggregated book (ms)
    pub update_interval_ms: u64,
    /// Minimum volume threshold to include a level
    pub min_volume_threshold: f64,
}

impl Default for AggregationConfig {
    fn default() -> Self {
        Self {
            max_age_ms: 1000,        // 1 second
            merge_threshold_bps: 1,   // 0.01%
            max_levels: 50,
            update_interval_ms: 100,  // 100ms updates
            min_volume_threshold: 0.001,
        }
    }
}

/// Aggregated price level with exchange attribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedLevel {
    pub price: f64,
    pub total_quantity: f64,
    pub exchange_contributions: Vec<ExchangeContribution>,
    pub best_exchange: String,
    #[serde(skip)]
    #[serde(default = "Instant::now")]
    pub update_time: Instant,
}

/// Exchange contribution to a price level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeContribution {
    pub exchange: String,
    pub quantity: f64,
    pub original_price: f64,
    pub latency_ms: u64,
}

/// Aggregated order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedOrderBook {
    pub symbol: String,
    pub bids: Vec<AggregatedLevel>,
    pub asks: Vec<AggregatedLevel>,
    #[serde(skip)]
    #[serde(default = "Instant::now")]
    pub last_update: Instant,
    pub contributing_exchanges: Vec<String>,
    pub total_bid_volume: f64,
    pub total_ask_volume: f64,
    pub weighted_mid_price: f64,
    pub spread_bps: u32,
}

/// Market statistics derived from aggregated book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketStatistics {
    pub symbol: String,
    pub best_bid: f64,
    pub best_ask: f64,
    pub mid_price: f64,
    pub spread_bps: u32,
    pub bid_depth_1pct: f64,    // Volume within 1% of best bid
    pub ask_depth_1pct: f64,    // Volume within 1% of best ask
    pub total_bid_liquidity: f64,
    pub total_ask_liquidity: f64,
    pub exchange_count: usize,
    pub data_quality_score: f64,
    #[serde(skip)]
    #[serde(default = "Instant::now")]
    pub last_update: Instant,
}

/// Individual exchange order book state
#[derive(Debug, Clone)]
struct ExchangeBookState {
    exchange_name: String,
    order_book: OrderBookData,
    last_update: Instant,
    latency_ms: u64,
    quality_score: f64,
}

/// Multi-exchange order book aggregator
pub struct OrderBookAggregator {
    config: AggregationConfig,
    exchange_books: Arc<RwLock<HashMap<String, ExchangeBookState>>>,
    aggregated_books: Arc<RwLock<HashMap<String, AggregatedOrderBook>>>,
    update_sender: broadcast::Sender<AggregatedOrderBook>,
    statistics: Arc<RwLock<HashMap<String, MarketStatistics>>>,
}

impl OrderBookAggregator {
    /// Create a new order book aggregator
    pub fn new(config: AggregationConfig) -> Self {
        let (update_sender, _) = broadcast::channel(1000);
        
        let aggregator = Self {
            config,
            exchange_books: Arc::new(RwLock::new(HashMap::new())),
            aggregated_books: Arc::new(RwLock::new(HashMap::new())),
            update_sender,
            statistics: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Start aggregation task
        aggregator.start_aggregation_task();
        
        aggregator
    }

    /// Subscribe to aggregated order book updates
    pub fn subscribe(&self) -> broadcast::Receiver<AggregatedOrderBook> {
        self.update_sender.subscribe()
    }

    /// Add order book data from an exchange
    pub async fn update_exchange_book(
        &self,
        exchange_name: String,
        order_book: OrderBookData,
        latency_ms: u64,
    ) -> Result<()> {
        let now = Instant::now();
        
        // Validate order book data
        if order_book.bids.is_empty() && order_book.asks.is_empty() {
            warn!("Received empty order book from {}", exchange_name);
            return Ok(());
        }

        let quality_score = self.calculate_quality_score(&order_book, latency_ms);
        
        let book_state = ExchangeBookState {
            exchange_name: exchange_name.clone(),
            order_book,
            last_update: now,
            latency_ms,
            quality_score,
        };

        // Update exchange book state
        {
            let mut books = self.exchange_books.write().await;
            books.insert(exchange_name.clone(), book_state.clone());
        }

        debug!("Updated order book for exchange with {} bids, {} asks", 
               book_state.order_book.bids.len(), 
               book_state.order_book.asks.len());

        Ok(())
    }

    /// Get the latest aggregated order book for a symbol
    pub async fn get_aggregated_book(&self, symbol: &str) -> Option<AggregatedOrderBook> {
        let books = self.aggregated_books.read().await;
        books.get(symbol).cloned()
    }

    /// Get market statistics for a symbol
    pub async fn get_market_statistics(&self, symbol: &str) -> Option<MarketStatistics> {
        let stats = self.statistics.read().await;
        stats.get(symbol).cloned()
    }

    /// Get all available symbols
    pub async fn get_available_symbols(&self) -> Vec<String> {
        let books = self.aggregated_books.read().await;
        books.keys().cloned().collect()
    }

    /// Start the aggregation task
    fn start_aggregation_task(&self) {
        let config = self.config.clone();
        let exchange_books = Arc::clone(&self.exchange_books);
        let aggregated_books = Arc::clone(&self.aggregated_books);
        let update_sender = self.update_sender.clone();
        let statistics = Arc::clone(&self.statistics);

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(config.update_interval_ms));
            
            loop {
                interval.tick().await;
                
                // Get all active symbols
                let symbols = {
                    let books = exchange_books.read().await;
                    books.values()
                        .map(|state| state.order_book.symbol.clone())
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                };

                for symbol in symbols {
                    if let Some(aggregated) = Self::aggregate_symbol(&config, &exchange_books, &symbol).await {
                        // Calculate statistics
                        let stats = Self::calculate_statistics(&aggregated);
                        
                        // Store aggregated book
                        {
                            let mut books = aggregated_books.write().await;
                            books.insert(symbol.clone(), aggregated.clone());
                        }
                        
                        // Store statistics
                        {
                            let mut stats_map = statistics.write().await;
                            stats_map.insert(symbol.clone(), stats);
                        }
                        
                        // Broadcast update
                        if let Err(_) = update_sender.send(aggregated) {
                            // No receivers, continue
                        }
                    }
                }
            }
        });
    }

    /// Aggregate order books for a specific symbol
    async fn aggregate_symbol(
        config: &AggregationConfig,
        exchange_books: &Arc<RwLock<HashMap<String, ExchangeBookState>>>,
        symbol: &str,
    ) -> Option<AggregatedOrderBook> {
        let now = Instant::now();
        let books = exchange_books.read().await;
        
        // Filter books for this symbol and remove stale data
        let valid_books: Vec<&ExchangeBookState> = books
            .values()
            .filter(|state| {
                state.order_book.symbol == symbol
                    && now.duration_since(state.last_update).as_millis() < config.max_age_ms as u128
            })
            .collect();

        if valid_books.is_empty() {
            return None;
        }

        // Aggregate bids (buy orders) - sort by price descending
        let mut aggregated_bids = BTreeMap::new();
        let mut aggregated_asks = BTreeMap::new();

        for book_state in &valid_books {
            // Process bids
            for level in &book_state.order_book.bids {
                let price_key = Self::price_to_key(level[0], config.merge_threshold_bps);
                
                aggregated_bids
                    .entry(price_key)
                    .or_insert_with(Vec::new)
                    .push((
                        book_state.exchange_name.clone(),
                        PriceLevel {
                            price: level[0],
                            quantity: level[1],
                        },
                        book_state.latency_ms,
                    ));
            }

            // Process asks
            for level in &book_state.order_book.asks {
                let price_key = Self::price_to_key(level[0], config.merge_threshold_bps);
                
                aggregated_asks
                    .entry(price_key)
                    .or_insert_with(Vec::new)
                    .push((
                        book_state.exchange_name.clone(),
                        PriceLevel {
                            price: level[0],
                            quantity: level[1],
                        },
                        book_state.latency_ms,
                    ));
            }
        }

        // Convert to aggregated levels
        let bids = Self::merge_levels(aggregated_bids, config, false);
        let asks = Self::merge_levels(aggregated_asks, config, true);

        // Calculate metrics
        let total_bid_volume: f64 = bids.iter().map(|l| l.total_quantity).sum();
        let total_ask_volume: f64 = asks.iter().map(|l| l.total_quantity).sum();
        
        let weighted_mid_price = if !bids.is_empty() && !asks.is_empty() {
            let best_bid = bids[0].price;
            let best_ask = asks[0].price;
            (best_bid + best_ask) / 2.0
        } else {
            0.0
        };

        let spread_bps = if !bids.is_empty() && !asks.is_empty() {
            let spread = asks[0].price - bids[0].price;
            ((spread / weighted_mid_price) * 10000.0) as u32
        } else {
            0
        };

        Some(AggregatedOrderBook {
            symbol: symbol.to_string(),
            bids,
            asks,
            last_update: now,
            contributing_exchanges: valid_books.iter().map(|b| b.exchange_name.clone()).collect(),
            total_bid_volume,
            total_ask_volume,
            weighted_mid_price,
            spread_bps,
        })
    }

    /// Merge price levels from multiple exchanges
    fn merge_levels(
        levels_map: BTreeMap<i64, Vec<(String, PriceLevel, u64)>>,
        config: &AggregationConfig,
        is_ask: bool,
    ) -> Vec<AggregatedLevel> {
        let mut result = Vec::new();
        
        for (_, exchange_levels) in levels_map {
            let mut total_quantity = 0.0;
            let mut contributions = Vec::new();
            let mut best_exchange = String::new();
            let mut best_latency = u64::MAX;

            // Calculate average price and aggregate quantities
            let mut weighted_price_sum = 0.0;
            let mut total_weight = 0.0;

            for (exchange, level, latency) in exchange_levels {
                total_quantity += level.quantity;
                weighted_price_sum += level.price * level.quantity;
                total_weight += level.quantity;

                contributions.push(ExchangeContribution {
                    exchange: exchange.clone(),
                    quantity: level.quantity,
                    original_price: level.price,
                    latency_ms: latency,
                });

                // Track best exchange (lowest latency)
                if latency < best_latency {
                    best_latency = latency;
                    best_exchange = exchange;
                }
            }

            if total_quantity >= config.min_volume_threshold {
                let average_price = if total_weight > 0.0 {
                    weighted_price_sum / total_weight
                } else {
                    contributions[0].original_price
                };

                result.push(AggregatedLevel {
                    price: average_price,
                    total_quantity,
                    exchange_contributions: contributions,
                    best_exchange,
                    update_time: Instant::now(),
                });
            }
        }

        // Sort levels appropriately
        if is_ask {
            result.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap()); // Ascending for asks
        } else {
            result.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap()); // Descending for bids
        }

        // Limit to max levels
        result.truncate(config.max_levels);
        
        result
    }

    /// Convert price to discrete key for merging
    fn price_to_key(price: f64, threshold_bps: u32) -> i64 {
        let factor = 10000.0 / threshold_bps as f64;
        (price * factor).round() as i64
    }

    /// Calculate quality score for order book data
    fn calculate_quality_score(&self, order_book: &OrderBookData, latency_ms: u64) -> f64 {
        let mut score = 1.0;

        // Penalize high latency
        let latency_factor = (200.0 / (latency_ms as f64 + 1.0)).min(1.0);
        score *= latency_factor;

        // Reward depth
        let depth_factor = ((order_book.bids.len() + order_book.asks.len()) as f64 / 20.0).min(1.0);
        score *= depth_factor;

        // Check for reasonable spreads
        if !order_book.bids.is_empty() && !order_book.asks.is_empty() {
            let best_bid = order_book.bids[0][0];
            let best_ask = order_book.asks[0][0];
            let spread_pct = (best_ask - best_bid) / best_bid;
            if spread_pct > 0.05 {
                // Penalize wide spreads
                score *= 0.5;
            }
        }

        score.max(0.1).min(1.0)
    }

    /// Calculate market statistics from aggregated book
    fn calculate_statistics(book: &AggregatedOrderBook) -> MarketStatistics {
        let best_bid = book.bids.first().map(|l| l.price).unwrap_or(0.0);
        let best_ask = book.asks.first().map(|l| l.price).unwrap_or(0.0);
        let mid_price = if best_bid > 0.0 && best_ask > 0.0 {
            (best_bid + best_ask) / 2.0
        } else {
            0.0
        };

        // Calculate depth within 1% of best prices
        let bid_depth_1pct = book.bids.iter()
            .take_while(|l| l.price >= best_bid * 0.99)
            .map(|l| l.total_quantity)
            .sum();

        let ask_depth_1pct = book.asks.iter()
            .take_while(|l| l.price <= best_ask * 1.01)
            .map(|l| l.total_quantity)
            .sum();

        // Calculate data quality score based on contributing exchanges and freshness
        let data_quality_score = (book.contributing_exchanges.len() as f64 / 5.0).min(1.0);

        MarketStatistics {
            symbol: book.symbol.clone(),
            best_bid,
            best_ask,
            mid_price,
            spread_bps: book.spread_bps,
            bid_depth_1pct,
            ask_depth_1pct,
            total_bid_liquidity: book.total_bid_volume,
            total_ask_liquidity: book.total_ask_volume,
            exchange_count: book.contributing_exchanges.len(),
            data_quality_score,
            last_update: book.last_update,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::PriceLevel;

    fn create_test_order_book(symbol: &str, exchange: &str) -> OrderBookData {
        OrderBookData {
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            bids: vec![
                PriceLevel { price: 50000.0, quantity: 1.0 },
                PriceLevel { price: 49990.0, quantity: 2.0 },
                PriceLevel { price: 49980.0, quantity: 1.5 },
            ],
            asks: vec![
                PriceLevel { price: 50010.0, quantity: 1.2 },
                PriceLevel { price: 50020.0, quantity: 1.8 },
                PriceLevel { price: 50030.0, quantity: 2.5 },
            ],
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    #[tokio::test]
    async fn test_order_book_aggregation() {
        let config = AggregationConfig::default();
        let aggregator = OrderBookAggregator::new(config);

        // Add order books from multiple exchanges
        let book1 = create_test_order_book("BTC/USDT", "exchange1");
        let book2 = create_test_order_book("BTC/USDT", "exchange2");

        aggregator.update_exchange_book("exchange1".to_string(), book1, 50).await.unwrap();
        aggregator.update_exchange_book("exchange2".to_string(), book2, 75).await.unwrap();

        // Wait for aggregation
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Check aggregated book
        let aggregated = aggregator.get_aggregated_book("BTC/USDT").await;
        assert!(aggregated.is_some());

        let book = aggregated.unwrap();
        assert_eq!(book.symbol, "BTC/USDT");
        assert_eq!(book.contributing_exchanges.len(), 2);
        assert!(!book.bids.is_empty());
        assert!(!book.asks.is_empty());

        // Check statistics
        let stats = aggregator.get_market_statistics("BTC/USDT").await;
        assert!(stats.is_some());

        let stats = stats.unwrap();
        assert!(stats.best_bid > 0.0);
        assert!(stats.best_ask > 0.0);
        assert_eq!(stats.exchange_count, 2);
    }

    #[tokio::test]
    async fn test_price_level_merging() {
        let config = AggregationConfig {
            merge_threshold_bps: 10, // 0.1% threshold
            ..Default::default()
        };

        let aggregator = OrderBookAggregator::new(config);

        // Create order books with similar prices that should merge
        let mut book1 = create_test_order_book("BTC/USDT", "exchange1");
        let mut book2 = create_test_order_book("BTC/USDT", "exchange2");

        // Adjust prices to be within merge threshold
        book2.bids[0].price = 50005.0; // Close to 50000.0
        book2.asks[0].price = 50015.0; // Close to 50010.0

        aggregator.update_exchange_book("exchange1".to_string(), book1, 50).await.unwrap();
        aggregator.update_exchange_book("exchange2".to_string(), book2, 75).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let aggregated = aggregator.get_aggregated_book("BTC/USDT").await.unwrap();
        
        // Should have merged levels from both exchanges
        assert!(aggregated.bids[0].exchange_contributions.len() > 1);
        assert!(aggregated.asks[0].exchange_contributions.len() > 1);
    }

    #[tokio::test]
    async fn test_stale_data_filtering() {
        let config = AggregationConfig {
            max_age_ms: 100, // Very short max age
            ..Default::default()
        };

        let aggregator = OrderBookAggregator::new(config);

        let book = create_test_order_book("BTC/USDT", "exchange1");
        aggregator.update_exchange_book("exchange1".to_string(), book, 50).await.unwrap();

        // Wait for data to become stale
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

        let aggregated = aggregator.get_aggregated_book("BTC/USDT").await;
        assert!(aggregated.is_none(), "Stale data should be filtered out");
    }
}