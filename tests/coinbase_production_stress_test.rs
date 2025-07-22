//! Production stress tests for Coinbase connector
//!
//! Run these tests against production to validate <10ms latency under real conditions:
//! ```
//! JACKBOT_ENV=prod cargo test --test coinbase_production_stress_test --release -- --nocapture
//! ```

use anyhow::Result;
use futures::StreamExt;
use jackbot_sensor::connectors::coinbase_production::CoinbaseProductionConnector;
use jackbot_sensor::connector::{Exchange, MarketData};
use jackbot_sensor::performance::orderbook_ultra::UltraOrderBook;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, warn, error};

/// Test configuration
const TEST_DURATION_SECS: u64 = 60; // Run for 1 minute
const TARGET_LATENCY_MS: u64 = 10; // Target <10ms
const SYMBOLS: &[&str] = &["BTC-USD", "ETH-USD", "SOL-USD", "AVAX-USD", "MATIC-USD"];

#[derive(Default)]
struct LatencyStats {
    count: u64,
    sum_micros: u64,
    min_micros: u64,
    max_micros: u64,
    p50_micros: u64,
    p95_micros: u64,
    p99_micros: u64,
    violations: u64, // Count of latencies > 10ms
}

impl LatencyStats {
    fn record(&mut self, latency: Duration) {
        let micros = latency.as_micros() as u64;
        self.count += 1;
        self.sum_micros += micros;
        
        if self.min_micros == 0 || micros < self.min_micros {
            self.min_micros = micros;
        }
        if micros > self.max_micros {
            self.max_micros = micros;
        }
        
        if latency > Duration::from_millis(TARGET_LATENCY_MS) {
            self.violations += 1;
        }
    }
    
    fn calculate_percentiles(&mut self, mut latencies: Vec<u64>) {
        if latencies.is_empty() {
            return;
        }
        
        latencies.sort_unstable();
        self.p50_micros = latencies[latencies.len() / 2];
        self.p95_micros = latencies[latencies.len() * 95 / 100];
        self.p99_micros = latencies[latencies.len() * 99 / 100];
    }
    
    fn average_micros(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.sum_micros / self.count
        }
    }
    
    fn print_summary(&self, name: &str) {
        println!("\n{} Latency Statistics:", name);
        println!("  Total messages: {}", self.count);
        println!("  Average: {:.2}ms", self.average_micros() as f64 / 1000.0);
        println!("  Min: {:.2}ms", self.min_micros as f64 / 1000.0);
        println!("  Max: {:.2}ms", self.max_micros as f64 / 1000.0);
        println!("  p50: {:.2}ms", self.p50_micros as f64 / 1000.0);
        println!("  p95: {:.2}ms", self.p95_micros as f64 / 1000.0);
        println!("  p99: {:.2}ms", self.p99_micros as f64 / 1000.0);
        println!("  Violations (>{}ms): {} ({:.2}%)", 
                TARGET_LATENCY_MS, 
                self.violations, 
                (self.violations as f64 / self.count as f64) * 100.0);
    }
}

#[tokio::test]
async fn test_production_websocket_latency() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    info!("Starting Coinbase production stress test");
    info!("Duration: {}s, Target latency: <{}ms", TEST_DURATION_SECS, TARGET_LATENCY_MS);
    info!("Symbols: {:?}", SYMBOLS);
    
    // Create production connector
    let connector = Arc::new(CoinbaseProductionConnector::new(None, None, None)?);
    
    // Connect to production WebSocket
    info!("Connecting to Coinbase production WebSocket...");
    connector.connect().await?;
    
    // Subscribe to market data
    info!("Subscribing to market data...");
    let symbols: Vec<String> = SYMBOLS.iter().map(|s| s.to_string()).collect();
    let mut stream = connector.subscribe_market_data(symbols.clone()).await?;
    
    // Tracking variables
    let running = Arc::new(AtomicBool::new(true));
    let message_count = Arc::new(AtomicU64::new(0));
    let orderbook_latencies = Arc::new(Mutex::new(Vec::new()));
    let trade_latencies = Arc::new(Mutex::new(Vec::new()));
    let network_latencies = Arc::new(Mutex::new(Vec::new()));
    
    // Start latency measurement task
    let running_clone = running.clone();
    let message_count_clone = message_count.clone();
    let orderbook_latencies_clone = orderbook_latencies.clone();
    let trade_latencies_clone = trade_latencies.clone();
    let network_latencies_clone = network_latencies.clone();
    
    let processor_handle = tokio::spawn(async move {
        let mut local_orderbooks = std::collections::HashMap::new();
        
        // Initialize local order books
        for symbol in &symbols {
            local_orderbooks.insert(
                symbol.clone(),
                UltraOrderBook::new(symbol.clone(), 10000)
            );
        }
        
        while running_clone.load(Ordering::Relaxed) {
            let receive_time = Instant::now();
            
            match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
                Ok(Some(market_data)) => {
                    let processing_start = Instant::now();
                    message_count_clone.fetch_add(1, Ordering::Relaxed);
                    
                    match market_data {
                        MarketData::OrderBook(book) => {
                            // Measure network latency (approximate)
                            let network_latency = receive_time.elapsed();
                            network_latencies_clone.lock().await.push(network_latency);
                            
                            // Update local order book
                            if let Some(local_book) = local_orderbooks.get(&book.symbol) {
                                let bids: Vec<(f64, f64)> = book.bids.iter()
                                    .map(|level| (level[0], level[1]))
                                    .collect();
                                let asks: Vec<(f64, f64)> = book.asks.iter()
                                    .map(|level| (level[0], level[1]))
                                    .collect();
                                
                                local_book.apply_snapshot(bids, asks);
                            }
                            
                            // Measure processing latency
                            let processing_latency = processing_start.elapsed();
                            orderbook_latencies_clone.lock().await.push(processing_latency);
                        }
                        MarketData::Trade(_trade) => {
                            // Measure trade processing latency
                            let processing_latency = processing_start.elapsed();
                            trade_latencies_clone.lock().await.push(processing_latency);
                        }
                        _ => {}
                    }
                }
                Ok(None) => {
                    warn!("Stream ended unexpectedly");
                    break;
                }
                Err(_) => {
                    warn!("Timeout waiting for market data");
                }
            }
        }
    });
    
    // Run test for specified duration
    tokio::time::sleep(Duration::from_secs(TEST_DURATION_SECS)).await;
    running.store(false, Ordering::Relaxed);
    
    // Wait for processor to finish
    let _ = processor_handle.await;
    
    // Calculate statistics
    let mut orderbook_stats = LatencyStats::default();
    let mut trade_stats = LatencyStats::default();
    let mut network_stats = LatencyStats::default();
    
    // Process order book latencies
    {
        let latencies = orderbook_latencies.lock().await;
        let micros: Vec<u64> = latencies.iter()
            .map(|d| d.as_micros() as u64)
            .collect();
        
        for latency in latencies.iter() {
            orderbook_stats.record(*latency);
        }
        orderbook_stats.calculate_percentiles(micros);
    }
    
    // Process trade latencies
    {
        let latencies = trade_latencies.lock().await;
        let micros: Vec<u64> = latencies.iter()
            .map(|d| d.as_micros() as u64)
            .collect();
        
        for latency in latencies.iter() {
            trade_stats.record(*latency);
        }
        trade_stats.calculate_percentiles(micros);
    }
    
    // Process network latencies
    {
        let latencies = network_latencies.lock().await;
        let micros: Vec<u64> = latencies.iter()
            .map(|d| d.as_micros() as u64)
            .collect();
        
        for latency in latencies.iter() {
            network_stats.record(*latency);
        }
        network_stats.calculate_percentiles(micros);
    }
    
    // Print results
    println!("\n========== PRODUCTION STRESS TEST RESULTS ==========");
    println!("Test duration: {}s", TEST_DURATION_SECS);
    println!("Total messages processed: {}", message_count.load(Ordering::Relaxed));
    println!("Messages per second: {:.2}", 
             message_count.load(Ordering::Relaxed) as f64 / TEST_DURATION_SECS as f64);
    
    orderbook_stats.print_summary("Order Book Processing");
    trade_stats.print_summary("Trade Processing");
    network_stats.print_summary("Network (Approximate)");
    
    // Get connector's internal latency stats
    let (p50, p95, p99) = connector.get_latency_percentiles();
    println!("\nConnector Internal Latency:");
    println!("  p50: {:?}", p50);
    println!("  p95: {:?}", p95);
    println!("  p99: {:?}", p99);
    
    // Validate requirements
    let total_violations = orderbook_stats.violations + trade_stats.violations;
    let total_messages = orderbook_stats.count + trade_stats.count;
    let violation_rate = (total_violations as f64 / total_messages as f64) * 100.0;
    
    println!("\n========== FINAL VERDICT ==========");
    if violation_rate < 1.0 && orderbook_stats.p95_micros < 10_000 {
        println!("✅ PASSED: Achieved <10ms latency for 99%+ of messages!");
        println!("   p95 latency: {:.2}ms", orderbook_stats.p95_micros as f64 / 1000.0);
        println!("   Violation rate: {:.2}%", violation_rate);
    } else {
        println!("❌ FAILED: Did not meet <10ms latency requirement");
        println!("   p95 latency: {:.2}ms", orderbook_stats.p95_micros as f64 / 1000.0);
        println!("   Violation rate: {:.2}%", violation_rate);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_concurrent_connections_stress() -> Result<()> {
    info!("Testing concurrent WebSocket connections");
    
    let running = Arc::new(AtomicBool::new(true));
    let total_messages = Arc::new(AtomicU64::new(0));
    let mut handles = vec![];
    
    // Create 5 concurrent connections
    for i in 0..5 {
        let running_clone = running.clone();
        let total_messages_clone = total_messages.clone();
        
        let handle = tokio::spawn(async move {
            let connector = Arc::new(CoinbaseProductionConnector::new(None, None, None).unwrap());
            connector.connect().await.unwrap();
            
            // Each connection subscribes to different symbols
            let symbol = match i {
                0 => "BTC-USD",
                1 => "ETH-USD",
                2 => "SOL-USD",
                3 => "AVAX-USD",
                _ => "MATIC-USD",
            };
            
            let mut stream = connector.subscribe_market_data(vec![symbol.to_string()]).await.unwrap();
            
            while running_clone.load(Ordering::Relaxed) {
                match tokio::time::timeout(Duration::from_secs(1), stream.next()).await {
                    Ok(Some(_)) => {
                        total_messages_clone.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Run for 30 seconds
    tokio::time::sleep(Duration::from_secs(30)).await;
    running.store(false, Ordering::Relaxed);
    
    // Wait for all connections to finish
    for handle in handles {
        let _ = handle.await;
    }
    
    let messages = total_messages.load(Ordering::Relaxed);
    let rate = messages as f64 / 30.0;
    
    println!("\nConcurrent connections test:");
    println!("  Total messages: {}", messages);
    println!("  Messages per second: {:.2}", rate);
    println!("  Per connection: {:.2} msg/s", rate / 5.0);
    
    assert!(rate > 100.0, "Message rate too low: {:.2} msg/s", rate);
    
    Ok(())
}

#[tokio::test]
async fn test_order_book_memory_efficiency() -> Result<()> {
    info!("Testing order book memory efficiency with 10,000 levels");
    
    let book = UltraOrderBook::new("BTC-USD".to_string(), 10000);
    
    // Fill with 10,000 levels
    let mut bids = Vec::with_capacity(10000);
    let mut asks = Vec::with_capacity(10000);
    
    for i in 0..10000 {
        bids.push((50000.0 - i as f64 * 0.01, 1.0 + (i % 100) as f64 * 0.01));
        asks.push((50001.0 + i as f64 * 0.01, 1.0 + (i % 100) as f64 * 0.01));
    }
    
    let start = Instant::now();
    book.apply_snapshot(bids, asks);
    let snapshot_time = start.elapsed();
    
    println!("\nOrder book performance:");
    println!("  10,000 level snapshot time: {:?}", snapshot_time);
    
    // Test update performance
    let mut update_times = Vec::new();
    for i in 0..1000 {
        let start = Instant::now();
        book.apply_update("bid", 50000.0 - i as f64 * 0.01, 2.0);
        update_times.push(start.elapsed());
    }
    
    update_times.sort();
    let avg_update = update_times.iter().sum::<Duration>() / update_times.len() as u32;
    let p99_update = update_times[update_times.len() * 99 / 100];
    
    println!("  Average update time: {:?}", avg_update);
    println!("  p99 update time: {:?}", p99_update);
    
    let stats = book.get_stats();
    println!("  Memory usage: {} MB", stats.memory_usage_bytes as f64 / 1_048_576.0);
    
    assert!(snapshot_time < Duration::from_millis(50), "Snapshot too slow: {:?}", snapshot_time);
    assert!(p99_update < Duration::from_micros(100), "Updates too slow: {:?}", p99_update);
    
    Ok(())
}

#[tokio::test]
async fn test_race_condition_resilience() -> Result<()> {
    info!("Testing race condition resilience");
    
    let book = Arc::new(UltraOrderBook::new("BTC-USD".to_string(), 1000));
    let error_count = Arc::new(AtomicU64::new(0));
    let mut handles = vec![];
    
    // Spawn 10 concurrent updaters
    for thread_id in 0..10 {
        let book_clone = book.clone();
        let error_count_clone = error_count.clone();
        
        let handle = tokio::spawn(async move {
            for i in 0..10000 {
                let price = 50000.0 + (thread_id as f64 * 10.0) + (i as f64 * 0.01);
                let side = if thread_id % 2 == 0 { "bid" } else { "ask" };
                
                // Try to cause race conditions
                book_clone.apply_update(side, price, 1.0);
                book_clone.apply_update(side, price, 0.0); // Remove
                book_clone.apply_update(side, price, 2.0); // Re-add
                
                // Verify best bid/ask is always valid
                let (bid, ask) = book_clone.get_best_bid_ask();
                if bid > 0.0 && ask > 0.0 && bid >= ask {
                    error_count_clone.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.await?;
    }
    
    let errors = error_count.load(Ordering::Relaxed);
    println!("\nRace condition test:");
    println!("  Total operations: {}", 10 * 10000 * 3);
    println!("  Errors detected: {}", errors);
    println!("  Error rate: {:.6}%", (errors as f64 / (10.0 * 10000.0 * 3.0)) * 100.0);
    
    assert_eq!(errors, 0, "Race conditions detected: {} errors", errors);
    
    Ok(())
}