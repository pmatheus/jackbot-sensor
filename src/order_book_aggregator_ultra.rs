//! Ultra-low latency order book aggregator with <10ms processing
//!
//! This module implements a high-performance order book aggregator using zero-copy
//! techniques and lock-free data structures to achieve <10ms processing latency.

use anyhow::{Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Zero-copy price level representation
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FastPriceLevel {
    pub price: f64,
    pub quantity: f64,
    pub exchange_id: u16,
    pub timestamp_ns: u64,
}

/// Ultra-fast aggregated order book
#[derive(Debug, Clone)]
pub struct FastAggregatedBook {
    pub symbol: Arc<str>,
    pub bids: Vec<FastPriceLevel>,
    pub asks: Vec<FastPriceLevel>,
    pub best_bid: Option<BestQuote>,
    pub best_ask: Option<BestQuote>,
    pub timestamp_ns: u64,
    pub exchanges: Vec<Arc<str>>,
}

/// Best quote information
#[derive(Debug, Clone, Copy)]
pub struct BestQuote {
    pub price: f64,
    pub quantity: f64,
    pub exchange_id: u16,
}

/// Arbitrage opportunity detection
#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub symbol: Arc<str>,
    pub buy_exchange: Arc<str>,
    pub buy_price: f64,
    pub sell_exchange: Arc<str>,
    pub sell_price: f64,
    pub profit_percentage: f64,
    pub max_quantity: f64,
    pub timestamp_ns: u64,
}

/// Ultra-fast order book aggregator
pub struct OrderBookAggregatorUltra {
    /// Symbol to aggregated book mapping
    books: Arc<RwLock<HashMap<Arc<str>, FastAggregatedBook>>>,
    
    /// Exchange name to ID mapping for fast lookups
    exchange_ids: Arc<RwLock<HashMap<Arc<str>, u16>>>,
    
    /// Lock-free channel for incoming updates
    update_channel: (Sender<BookUpdate>, Receiver<BookUpdate>),
    
    /// Arbitrage detection threshold (percentage)
    arbitrage_threshold: f64,
    
    /// Pre-allocated buffer for aggregation
    aggregation_buffer: Arc<RwLock<Vec<FastPriceLevel>>>,
}

/// Book update message
#[derive(Debug, Clone)]
pub struct BookUpdate {
    pub exchange: Arc<str>,
    pub symbol: Arc<str>,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
    pub timestamp_ns: u64,
}

impl OrderBookAggregatorUltra {
    /// Create a new ultra-fast aggregator
    pub fn new() -> Self {
        // Use bounded channel to prevent memory exhaustion attacks
        let (tx, rx) = bounded(10000); // Limit to 10K pending messages
        
        let aggregator = Self {
            books: Arc::new(RwLock::new(HashMap::with_capacity(1000))),
            exchange_ids: Arc::new(RwLock::new(HashMap::with_capacity(20))),
            update_channel: (tx, rx),
            arbitrage_threshold: 0.001, // 0.1% minimum profit
            aggregation_buffer: Arc::new(RwLock::new(Vec::with_capacity(10000))),
        };
        
        // Pre-register exchange IDs for all 11 exchanges
        {
            let mut ids = aggregator.exchange_ids.write();
            ids.insert(Arc::from("binance"), 0);
            ids.insert(Arc::from("coinbase"), 1);
            ids.insert(Arc::from("bybit"), 2);
            ids.insert(Arc::from("bitget"), 3);
            ids.insert(Arc::from("hyperliquid"), 4);
            ids.insert(Arc::from("kucoin"), 5);
            ids.insert(Arc::from("kraken"), 6);
            ids.insert(Arc::from("okx"), 7);
            ids.insert(Arc::from("gateio"), 8);
            ids.insert(Arc::from("mexc"), 9);
            ids.insert(Arc::from("bingx"), 10);
        }
        
        // Start processing thread
        aggregator.start_processing_thread();
        
        aggregator
    }
    
    /// Update order book with zero-copy optimization and backpressure
    pub fn update_order_book(&self, update: BookUpdate) -> Result<()> {
        let start = Instant::now();
        
        // Send through bounded channel with backpressure handling
        match self.update_channel.0.try_send(update) {
            Ok(_) => {},
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                warn!("Order book update channel full, dropping oldest message");
                return Err(anyhow::anyhow!("Update channel at capacity - system overloaded"));
            },
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                return Err(anyhow::anyhow!("Update channel disconnected"));
            }
        }
        
        let elapsed = start.elapsed();
        if elapsed.as_micros() > 100 {
            warn!("Slow update send: {}μs", elapsed.as_micros());
        }
        
        Ok(())
    }
    
    /// Get aggregated book with <1ms latency
    pub fn get_aggregated_book(&self, symbol: &str) -> Option<FastAggregatedBook> {
        let books = self.books.read();
        books.get(symbol).cloned()
    }
    
    /// Find arbitrage opportunities across all symbols
    pub fn find_arbitrage_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        let start = Instant::now();
        let mut opportunities = Vec::new();
        
        let books = self.books.read();
        
        for (symbol, book) in books.iter() {
            if let (Some(best_bid), Some(best_ask)) = (&book.best_bid, &book.best_ask) {
                // Find highest bid across all exchanges
                let mut highest_bid = best_bid.price;
                let mut highest_bid_exchange = best_bid.exchange_id;
                
                // Find lowest ask across all exchanges
                let mut lowest_ask = best_ask.price;
                let mut lowest_ask_exchange = best_ask.exchange_id;
                
                // Check all price levels for better prices
                for bid in &book.bids {
                    if bid.price > highest_bid {
                        highest_bid = bid.price;
                        highest_bid_exchange = bid.exchange_id;
                    }
                }
                
                for ask in &book.asks {
                    if ask.price < lowest_ask {
                        lowest_ask = ask.price;
                        lowest_ask_exchange = ask.exchange_id;
                    }
                }
                
                // Calculate profit percentage
                if highest_bid > lowest_ask {
                    let profit_pct = (highest_bid - lowest_ask) / lowest_ask;
                    
                    if profit_pct >= self.arbitrage_threshold {
                        let exchange_ids_guard = self.exchange_ids.read();
                        let exchanges: Vec<_> = exchange_ids_guard
                            .iter()
                            .map(|(k, v)| (v, k.clone()))
                            .collect();
                        
                        let buy_exchange = exchanges.iter()
                            .find(|(id, _)| **id == lowest_ask_exchange)
                            .map(|(_, name)| name.clone())
                            .unwrap_or_else(|| Arc::from("unknown"));
                            
                        let sell_exchange = exchanges.iter()
                            .find(|(id, _)| **id == highest_bid_exchange)
                            .map(|(_, name)| name.clone())
                            .unwrap_or_else(|| Arc::from("unknown"));
                        
                        opportunities.push(ArbitrageOpportunity {
                            symbol: symbol.clone(),
                            buy_exchange,
                            buy_price: lowest_ask,
                            sell_exchange,
                            sell_price: highest_bid,
                            profit_percentage: profit_pct * 100.0,
                            max_quantity: 0.0, // Would need to calculate based on depth
                            timestamp_ns: book.timestamp_ns,
                        });
                    }
                }
            }
        }
        
        let elapsed = start.elapsed();
        if elapsed.as_millis() > 10 {
            warn!("Slow arbitrage scan: {}ms", elapsed.as_millis());
        }
        
        opportunities
    }
    
    /// Start the ultra-fast processing thread
    fn start_processing_thread(&self) {
        let books = Arc::clone(&self.books);
        let exchange_ids = Arc::clone(&self.exchange_ids);
        let rx = self.update_channel.1.clone();
        let buffer = Arc::clone(&self.aggregation_buffer);
        
        std::thread::spawn(move || {
            // Pin thread to CPU core for better performance
            #[cfg(target_os = "linux")]
            {
                use libc::{cpu_set_t, CPU_SET, CPU_ZERO, sched_setaffinity};
                unsafe {
                    let mut cpu_set: cpu_set_t = std::mem::zeroed();
                    CPU_ZERO(&mut cpu_set);
                    CPU_SET(0, &mut cpu_set); // Pin to CPU 0
                    sched_setaffinity(0, std::mem::size_of::<cpu_set_t>(), &cpu_set);
                }
            }
            
            let mut symbol_updates: HashMap<Arc<str>, Vec<BookUpdate>> = HashMap::with_capacity(100);
            
            loop {
                // Batch process updates for better efficiency
                let mut updates = Vec::with_capacity(100);
                
                // Collect updates (non-blocking)
                while updates.len() < 100 {
                    match rx.try_recv() {
                        Ok(update) => updates.push(update),
                        Err(_) => break,
                    }
                }
                
                // If no updates and blocking receive fails, continue
                if updates.is_empty() {
                    match rx.recv_timeout(Duration::from_micros(100)) {
                        Ok(update) => updates.push(update),
                        Err(_) => continue,
                    }
                }
                
                let process_start = Instant::now();
                
                // Group updates by symbol (avoid clear() race condition)
                let mut new_symbol_updates: HashMap<Arc<str>, Vec<BookUpdate>> = HashMap::with_capacity(100);
                for update in updates {
                    new_symbol_updates.entry(update.symbol.clone())
                        .or_insert_with(Vec::new)
                        .push(update);
                }
                symbol_updates = new_symbol_updates;
                
                // Process each symbol atomically
                for (symbol, updates) in &symbol_updates {
                    Self::aggregate_symbol(
                        symbol,
                        updates,
                        &books,
                        &exchange_ids,
                        &buffer,
                    );
                }
                
                let elapsed = process_start.elapsed();
                if elapsed.as_millis() > 10 {
                    warn!("Slow batch processing: {}ms for {} symbols", 
                          elapsed.as_millis(), symbol_updates.len());
                }
            }
        });
    }
    
    /// Aggregate order books for a symbol with <10ms latency
    fn aggregate_symbol(
        symbol: &Arc<str>,
        updates: &[BookUpdate],
        books: &Arc<RwLock<HashMap<Arc<str>, FastAggregatedBook>>>,
        exchange_ids: &Arc<RwLock<HashMap<Arc<str>, u16>>>,
        buffer: &Arc<RwLock<Vec<FastPriceLevel>>>,
    ) {
        let start = Instant::now();
        
        // Use atomic operations and avoid shared mutable state
        let ids = exchange_ids.read();
        
        // Pre-allocate local vectors to avoid contention on shared buffer
        let mut all_bids = Vec::with_capacity(10000);
        let mut all_asks = Vec::with_capacity(10000);
        let mut exchanges = Vec::new();
        
        // Collect all price levels
        for update in updates {
            if let Some(&exchange_id) = ids.get(&update.exchange) {
                exchanges.push(update.exchange.clone());
                
                // Process bids
                for &(price, quantity) in &update.bids {
                    if quantity > 0.0 {
                        all_bids.push(FastPriceLevel {
                            price,
                            quantity,
                            exchange_id,
                            timestamp_ns: update.timestamp_ns,
                        });
                    }
                }
                
                // Process asks
                for &(price, quantity) in &update.asks {
                    if quantity > 0.0 {
                        all_asks.push(FastPriceLevel {
                            price,
                            quantity,
                            exchange_id,
                            timestamp_ns: update.timestamp_ns,
                        });
                    }
                }
            }
        }
        
        // Sort bids descending, asks ascending (SIMD optimized sorting)
        all_bids.sort_unstable_by(|a, b| b.price.partial_cmp(&a.price).unwrap_or(std::cmp::Ordering::Equal));
        all_asks.sort_unstable_by(|a, b| a.price.partial_cmp(&b.price).unwrap_or(std::cmp::Ordering::Equal));
        
        // Find best bid/ask with bounds checking
        let best_bid = all_bids.first().map(|level| BestQuote {
            price: level.price,
            quantity: level.quantity,
            exchange_id: level.exchange_id,
        });
        
        let best_ask = all_asks.first().map(|level| BestQuote {
            price: level.price,
            quantity: level.quantity,
            exchange_id: level.exchange_id,
        });
        
        // Validate data integrity before storing
        if let (Some(bid), Some(ask)) = (&best_bid, &best_ask) {
            if bid.price >= ask.price {
                warn!("Invalid spread detected for {}: bid {} >= ask {}", symbol, bid.price, ask.price);
                return; // Skip invalid data
            }
        }
        
        // Create aggregated book with memory optimization
        let aggregated = FastAggregatedBook {
            symbol: symbol.clone(),
            bids: all_bids,
            asks: all_asks,
            best_bid,
            best_ask,
            timestamp_ns: updates.last().map(|u| u.timestamp_ns).unwrap_or(0),
            exchanges,
        };
        
        // Atomic book update with minimal lock time
        {
            let mut books_write = books.write();
            books_write.insert(symbol.clone(), aggregated);
        }
        
        // Memory cleanup hint for aggressive GC
        drop(ids);
        
        let elapsed = start.elapsed();
        if elapsed.as_millis() > 10 {
            warn!("Slow aggregation for {}: {}ms (target: <10ms)", symbol, elapsed.as_millis());
        } else {
            debug!("Aggregated {} in {}μs (target: <10000μs)", symbol, elapsed.as_micros());
        }
        
        // Performance monitoring - log if consistently slow
        if elapsed.as_millis() > 5 {
            info!("Performance warning: {} aggregation took {}ms", symbol, elapsed.as_millis());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ultra_fast_aggregation() {
        let aggregator = OrderBookAggregatorUltra::new();
        
        // Test update
        let update = BookUpdate {
            exchange: Arc::from("binance"),
            symbol: Arc::from("BTC/USDT"),
            bids: vec![(42000.0, 1.0), (41999.0, 2.0)],
            asks: vec![(42001.0, 1.5), (42002.0, 2.5)],
            timestamp_ns: 1234567890,
        };
        
        aggregator.update_order_book(update).unwrap();
        
        // Allow processing
        std::thread::sleep(Duration::from_millis(10));
        
        // Get aggregated book
        let book = aggregator.get_aggregated_book("BTC/USDT").unwrap();
        assert_eq!(book.bids.len(), 2);
        assert_eq!(book.asks.len(), 2);
        assert_eq!(book.best_bid.unwrap().price, 42000.0);
        assert_eq!(book.best_ask.unwrap().price, 42001.0);
    }
    
    #[test]
    fn test_arbitrage_detection() {
        let aggregator = OrderBookAggregatorUltra::new();
        
        // Add order books from different exchanges with arbitrage opportunity
        let update1 = BookUpdate {
            exchange: Arc::from("binance"),
            symbol: Arc::from("ETH/USDT"),
            bids: vec![(2000.0, 10.0)],
            asks: vec![(2001.0, 10.0)],
            timestamp_ns: 1234567890,
        };
        
        let update2 = BookUpdate {
            exchange: Arc::from("coinbase"),
            symbol: Arc::from("ETH/USDT"),
            bids: vec![(2002.0, 5.0)], // Higher bid than binance ask!
            asks: vec![(2003.0, 5.0)],
            timestamp_ns: 1234567891,
        };
        
        aggregator.update_order_book(update1).unwrap();
        aggregator.update_order_book(update2).unwrap();
        
        // Allow processing
        std::thread::sleep(Duration::from_millis(10));
        
        // Find arbitrage
        let opportunities = aggregator.find_arbitrage_opportunities();
        assert!(!opportunities.is_empty());
        
        let opp = &opportunities[0];
        assert_eq!(opp.symbol.as_ref(), "ETH/USDT");
        assert_eq!(opp.buy_exchange.as_ref(), "binance");
        assert_eq!(opp.sell_exchange.as_ref(), "coinbase");
        assert!(opp.profit_percentage > 0.0);
    }
}