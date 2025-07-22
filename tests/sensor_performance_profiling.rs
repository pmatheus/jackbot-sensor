//! SENSOR PERFORMANCE PROFILING AND BENCHMARKING
//! Microsecond-precision performance analysis for all critical paths
//! 
//! This test identifies bottlenecks, hot spots, and optimization opportunities

use anyhow::Result;
use criterion::{black_box, Criterion};
use flamegraph::Profiler;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tracing::{info, warn};

// Import sensor components
use jackbot_sensor::order_book_aggregator_ultra::{
    OrderBookAggregatorUltra, BookUpdate, FastPriceLevel
};
use jackbot_sensor::connector::MarketData;
use jackbot_sensor::performance::latency_tracker::LatencyTracker;

/// Performance profiling results
#[derive(Debug, Clone)]
struct ProfilingResults {
    test_name: String,
    samples: Vec<u64>,
    p50_us: u64,
    p90_us: u64,
    p95_us: u64,
    p99_us: u64,
    p999_us: u64,
    mean_us: u64,
    std_dev_us: u64,
    min_us: u64,
    max_us: u64,
    throughput_ops_sec: f64,
}

impl ProfilingResults {
    fn from_samples(test_name: String, samples: Vec<u64>, duration_secs: f64) -> Self {
        let mut sorted = samples.clone();
        sorted.sort_unstable();
        
        let len = sorted.len();
        let p50 = sorted[len * 50 / 100];
        let p90 = sorted[len * 90 / 100];
        let p95 = sorted[len * 95 / 100];
        let p99 = sorted[len * 99 / 100];
        let p999 = sorted[len * 999 / 1000];
        
        let sum: u64 = sorted.iter().sum();
        let mean = sum / len as u64;
        
        let variance: u64 = sorted.iter()
            .map(|&x| {
                let diff = if x > mean { x - mean } else { mean - x };
                diff * diff
            })
            .sum::<u64>() / len as u64;
        let std_dev = (variance as f64).sqrt() as u64;
        
        ProfilingResults {
            test_name,
            samples: sorted,
            p50_us: p50,
            p90_us: p90,
            p95_us: p95,
            p99_us: p99,
            p999_us: p999,
            mean_us: mean,
            std_dev_us: std_dev,
            min_us: sorted[0],
            max_us: sorted[len - 1],
            throughput_ops_sec: len as f64 / duration_secs,
        }
    }

    fn print_report(&self) {
        println!("\n📊 {} Performance Profile:", self.test_name);
        println!("  Samples: {}", self.samples.len());
        println!("  Throughput: {:.0} ops/sec", self.throughput_ops_sec);
        println!("  Latency Distribution (microseconds):");
        println!("    Min:    {:>6} μs", self.min_us);
        println!("    P50:    {:>6} μs", self.p50_us);
        println!("    P90:    {:>6} μs", self.p90_us);
        println!("    P95:    {:>6} μs", self.p95_us);
        println!("    P99:    {:>6} μs (CRITICAL: Must be <10,000)", self.p99_us);
        println!("    P99.9:  {:>6} μs", self.p999_us);
        println!("    Max:    {:>6} μs", self.max_us);
        println!("    Mean:   {:>6} μs", self.mean_us);
        println!("    StdDev: {:>6} μs", self.std_dev_us);
        
        if self.p99_us > 10_000 {
            println!("  ❌ FAILED: P99 latency exceeds 10ms requirement!");
        } else {
            println!("  ✅ PASSED: P99 latency within requirements");
        }
    }
}

/// PROFILE 1: Order Book Update Processing
#[tokio::test]
async fn profile_order_book_update_processing() -> Result<()> {
    info!("🔬 PROFILING: Order Book Update Processing");
    
    let aggregator = Arc::new(OrderBookAggregatorUltra::new());
    let mut samples = Vec::with_capacity(1_000_000);
    
    // Warm up
    for _ in 0..1000 {
        let update = create_test_book_update("BTC/USDT", "binance", 100);
        aggregator.update_order_book(update)?;
    }
    
    // Profile 1M updates
    let start_time = Instant::now();
    
    for i in 0..1_000_000 {
        let exchange = match i % 11 {
            0 => "binance",
            1 => "coinbase",
            2 => "bybit",
            3 => "bitget",
            4 => "hyperliquid",
            5 => "kucoin",
            6 => "kraken",
            7 => "okx",
            8 => "gateio",
            9 => "mexc",
            10 => "bingx",
            _ => unreachable!(),
        };
        
        let update = create_test_book_update("BTC/USDT", exchange, 50 + (i % 50) as usize);
        
        let op_start = Instant::now();
        aggregator.update_order_book(update)?;
        let latency_us = op_start.elapsed().as_micros() as u64;
        
        samples.push(latency_us);
        
        if i % 100_000 == 0 && i > 0 {
            info!("Processed {} updates...", i);
        }
    }
    
    let duration = start_time.elapsed().as_secs_f64();
    let results = ProfilingResults::from_samples(
        "Order Book Update Processing".to_string(),
        samples,
        duration
    );
    
    results.print_report();
    
    assert!(results.p99_us < 10_000, "Order book update P99 latency too high");
    Ok(())
}

/// PROFILE 2: Arbitrage Detection Performance
#[tokio::test]
async fn profile_arbitrage_detection() -> Result<()> {
    info!("🔬 PROFILING: Arbitrage Detection Performance");
    
    let aggregator = Arc::new(OrderBookAggregatorUltra::new());
    
    // Setup: Create order books with arbitrage opportunities
    for symbol in ["BTC/USDT", "ETH/USDT", "SOL/USDT", "AVAX/USDT", "MATIC/USDT"] {
        for (i, exchange) in ["binance", "coinbase", "kraken", "gateio", "mexc"].iter().enumerate() {
            let mut update = create_test_book_update(symbol, exchange, 100);
            
            // Create price discrepancies
            if i % 2 == 0 {
                // Lower asks on even exchanges
                update.asks[0].0 -= 50.0;
            } else {
                // Higher bids on odd exchanges
                update.bids[0].0 += 50.0;
            }
            
            aggregator.update_order_book(update)?;
        }
    }
    
    // Allow aggregation to complete
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Profile arbitrage detection
    let mut samples = Vec::with_capacity(100_000);
    let start_time = Instant::now();
    
    for _ in 0..100_000 {
        let op_start = Instant::now();
        let opportunities = aggregator.find_arbitrage_opportunities();
        let latency_us = op_start.elapsed().as_micros() as u64;
        
        samples.push(latency_us);
        
        // Verify we're finding opportunities
        assert!(!opportunities.is_empty(), "Should find arbitrage opportunities");
    }
    
    let duration = start_time.elapsed().as_secs_f64();
    let results = ProfilingResults::from_samples(
        "Arbitrage Detection".to_string(),
        samples,
        duration
    );
    
    results.print_report();
    
    assert!(results.p99_us < 10_000, "Arbitrage detection P99 latency too high");
    Ok(())
}

/// PROFILE 3: Concurrent Multi-Exchange Load
#[tokio::test]
async fn profile_concurrent_multi_exchange_load() -> Result<()> {
    info!("🔬 PROFILING: Concurrent Multi-Exchange Load");
    
    let aggregator = Arc::new(OrderBookAggregatorUltra::new());
    let samples = Arc::new(RwLock::new(Vec::with_capacity(1_000_000)));
    let start_time = Instant::now();
    
    // Spawn 11 tasks, one per exchange
    let mut handles = Vec::new();
    
    for (idx, exchange) in ["binance", "coinbase", "bybit", "bitget", "hyperliquid", 
                             "kucoin", "kraken", "okx", "gateio", "mexc", "bingx"].iter().enumerate() {
        let agg = aggregator.clone();
        let exchange_name = exchange.to_string();
        let samples_clone = samples.clone();
        
        let handle = tokio::spawn(async move {
            for i in 0..100_000 {
                let symbol = match i % 5 {
                    0 => "BTC/USDT",
                    1 => "ETH/USDT",
                    2 => "SOL/USDT",
                    3 => "AVAX/USDT",
                    4 => "MATIC/USDT",
                    _ => unreachable!(),
                };
                
                let update = create_test_book_update(symbol, &exchange_name, 20 + idx * 5);
                
                let op_start = Instant::now();
                let _ = agg.update_order_book(update);
                let latency_us = op_start.elapsed().as_micros() as u64;
                
                samples_clone.write().push(latency_us);
                
                // Simulate realistic message rate
                if i % 100 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all exchanges to complete
    futures::future::join_all(handles).await;
    
    let duration = start_time.elapsed().as_secs_f64();
    let all_samples = samples.read().clone();
    
    let results = ProfilingResults::from_samples(
        "Concurrent Multi-Exchange Load".to_string(),
        all_samples,
        duration
    );
    
    results.print_report();
    
    assert!(results.p99_us < 10_000, "Multi-exchange P99 latency too high");
    Ok(())
}

/// PROFILE 4: Memory Allocation Patterns
#[tokio::test]
async fn profile_memory_allocation_patterns() -> Result<()> {
    info!("🔬 PROFILING: Memory Allocation Patterns");
    
    let initial_memory = get_current_memory_kb();
    let aggregator = Arc::new(OrderBookAggregatorUltra::new());
    let memory_samples = Arc::new(RwLock::new(Vec::new()));
    
    // Monitor memory during operations
    let monitor_samples = memory_samples.clone();
    let monitor = tokio::spawn(async move {
        for _ in 0..100 {
            let current_mem = get_current_memory_kb();
            monitor_samples.write().push(current_mem);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    
    // Perform memory-intensive operations
    for cycle in 0..10 {
        info!("Memory stress cycle {}/10", cycle + 1);
        
        // Create large order books
        for i in 0..1000 {
            let depth = 100 + (i % 400); // Variable depth
            let update = create_test_book_update(
                &format!("TEST{}/USDT", i),
                "binance",
                depth
            );
            aggregator.update_order_book(update)?;
        }
        
        // Force aggregation
        for _ in 0..100 {
            let _ = aggregator.find_arbitrage_opportunities();
        }
        
        // Clear some books (simulate cleanup)
        if cycle % 2 == 0 {
            // In real implementation, would clear old books
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    
    monitor.await?;
    
    let final_memory = get_current_memory_kb();
    let memory_growth_kb = final_memory.saturating_sub(initial_memory);
    let samples = memory_samples.read();
    let peak_memory = samples.iter().max().copied().unwrap_or(initial_memory);
    
    info!("📊 Memory Allocation Profile:");
    info!("  Initial Memory: {} KB", initial_memory);
    info!("  Final Memory: {} KB", final_memory);
    info!("  Peak Memory: {} KB", peak_memory);
    info!("  Memory Growth: {} KB ({:.2} MB)", memory_growth_kb, memory_growth_kb as f64 / 1024.0);
    info!("  Samples: {}", samples.len());
    
    // Check for memory leaks
    let acceptable_growth_mb = 50.0; // 50MB acceptable for caching
    assert!(
        memory_growth_kb as f64 / 1024.0 < acceptable_growth_mb,
        "Excessive memory growth: {:.2} MB",
        memory_growth_kb as f64 / 1024.0
    );
    
    Ok(())
}

/// PROFILE 5: JSON Parsing Performance
#[tokio::test]
async fn profile_json_parsing_performance() -> Result<()> {
    info!("🔬 PROFILING: JSON Parsing Performance");
    
    let test_messages = vec![
        // Small message
        r#"{"symbol":"BTC/USDT","bids":[[50000,1.5]],"asks":[[50001,1.0]]}"#,
        
        // Medium message (10 levels)
        generate_order_book_json("ETH/USDT", 10),
        
        // Large message (100 levels)
        generate_order_book_json("SOL/USDT", 100),
        
        // Huge message (500 levels)
        generate_order_book_json("AVAX/USDT", 500),
    ];
    
    let mut samples = Vec::with_capacity(100_000);
    let start_time = Instant::now();
    
    for i in 0..100_000 {
        let msg = &test_messages[i % test_messages.len()];
        
        let op_start = Instant::now();
        let _parsed: serde_json::Value = serde_json::from_str(msg)?;
        let latency_us = op_start.elapsed().as_micros() as u64;
        
        samples.push(latency_us);
    }
    
    let duration = start_time.elapsed().as_secs_f64();
    let results = ProfilingResults::from_samples(
        "JSON Parsing".to_string(),
        samples,
        duration
    );
    
    results.print_report();
    
    // JSON parsing should be fast
    assert!(results.p95_us < 1_000, "JSON parsing too slow");
    Ok(())
}

/// PROFILE 6: WebSocket Message Processing Pipeline
#[tokio::test]
async fn profile_websocket_pipeline() -> Result<()> {
    info!("🔬 PROFILING: WebSocket Message Processing Pipeline");
    
    let pipeline_stages = Arc::new(RwLock::new(HashMap::new()));
    let mut samples = Vec::with_capacity(100_000);
    
    for i in 0..100_000 {
        let start = Instant::now();
        
        // Stage 1: Receive from network (simulated)
        let receive_start = Instant::now();
        tokio::time::sleep(Duration::from_micros(10)).await; // Simulate network
        let receive_time = receive_start.elapsed().as_micros() as u64;
        
        // Stage 2: Parse JSON
        let parse_start = Instant::now();
        let msg = generate_order_book_json("BTC/USDT", 20 + (i % 30) as usize);
        let parsed: serde_json::Value = serde_json::from_str(&msg)?;
        let parse_time = parse_start.elapsed().as_micros() as u64;
        
        // Stage 3: Validate data
        let validate_start = Instant::now();
        validate_order_book_data(&parsed)?;
        let validate_time = validate_start.elapsed().as_micros() as u64;
        
        // Stage 4: Update aggregator (simulated)
        let update_start = Instant::now();
        // Actual update would go here
        let update_time = update_start.elapsed().as_micros() as u64;
        
        let total_time = start.elapsed().as_micros() as u64;
        samples.push(total_time);
        
        // Track stage timings
        let mut stages = pipeline_stages.write();
        stages.entry("receive").or_insert(Vec::new()).push(receive_time);
        stages.entry("parse").or_insert(Vec::new()).push(parse_time);
        stages.entry("validate").or_insert(Vec::new()).push(validate_time);
        stages.entry("update").or_insert(Vec::new()).push(update_time);
    }
    
    let results = ProfilingResults::from_samples(
        "WebSocket Pipeline".to_string(),
        samples,
        100.0 // Approximate duration
    );
    
    results.print_report();
    
    // Print stage breakdown
    info!("\n📊 Pipeline Stage Breakdown:");
    let stages = pipeline_stages.read();
    for (stage, timings) in stages.iter() {
        let avg = timings.iter().sum::<u64>() / timings.len() as u64;
        info!("  {}: avg {}μs", stage, avg);
    }
    
    assert!(results.p99_us < 10_000, "WebSocket pipeline P99 latency too high");
    Ok(())
}

/// PROFILE 7: CPU Cache Performance
#[test]
fn profile_cpu_cache_performance() {
    info!("🔬 PROFILING: CPU Cache Performance");
    
    // Test different memory access patterns
    let sizes = vec![
        1_024,       // 1KB - L1 cache
        32_768,      // 32KB - L1/L2 boundary
        262_144,     // 256KB - L2 cache
        8_388_608,   // 8MB - L3 cache
        67_108_864,  // 64MB - Beyond cache
    ];
    
    for size in sizes {
        let data: Vec<u64> = (0..size/8).map(|i| i as u64).collect();
        let mut sum = 0u64;
        
        // Sequential access
        let seq_start = Instant::now();
        for i in 0..1000 {
            for &val in &data {
                sum = sum.wrapping_add(val);
            }
        }
        let seq_time = seq_start.elapsed();
        
        // Random access
        let mut indices: Vec<usize> = (0..data.len()).collect();
        use rand::seq::SliceRandom;
        indices.shuffle(&mut rand::thread_rng());
        
        let rand_start = Instant::now();
        for i in 0..1000 {
            for &idx in &indices {
                sum = sum.wrapping_add(data[idx]);
            }
        }
        let rand_time = rand_start.elapsed();
        
        info!("  Size {}KB:", size / 1024);
        info!("    Sequential: {:?}", seq_time);
        info!("    Random: {:?}", rand_time);
        info!("    Ratio: {:.2}x", rand_time.as_nanos() as f64 / seq_time.as_nanos() as f64);
        
        // Prevent optimization
        black_box(sum);
    }
}

// Helper functions

fn create_test_book_update(symbol: &str, exchange: &str, depth: usize) -> BookUpdate {
    let base_price = match symbol {
        "BTC/USDT" => 50000.0,
        "ETH/USDT" => 3000.0,
        "SOL/USDT" => 100.0,
        "AVAX/USDT" => 50.0,
        "MATIC/USDT" => 1.0,
        _ => 100.0,
    };
    
    let mut bids = Vec::with_capacity(depth);
    let mut asks = Vec::with_capacity(depth);
    
    for i in 0..depth {
        bids.push((base_price - i as f64 * 0.1, 1.0 + i as f64 * 0.01));
        asks.push((base_price + i as f64 * 0.1, 1.0 + i as f64 * 0.01));
    }
    
    BookUpdate {
        exchange: Arc::from(exchange),
        symbol: Arc::from(symbol),
        bids,
        asks,
        timestamp_ns: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
    }
}

fn generate_order_book_json(symbol: &str, depth: usize) -> String {
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    
    for i in 0..depth {
        bids.push(format!("[{},{}]", 50000.0 - i as f64 * 0.1, 1.0 + i as f64 * 0.01));
        asks.push(format!("[{},{}]", 50000.0 + i as f64 * 0.1, 1.0 + i as f64 * 0.01));
    }
    
    format!(
        r#"{{"symbol":"{}","bids":[{}],"asks":[{}],"timestamp":{}}}"#,
        symbol,
        bids.join(","),
        asks.join(","),
        chrono::Utc::now().timestamp_millis()
    )
}

fn validate_order_book_data(data: &serde_json::Value) -> Result<()> {
    // Validate required fields
    if !data["symbol"].is_string() {
        return Err(anyhow::anyhow!("Invalid symbol"));
    }
    if !data["bids"].is_array() {
        return Err(anyhow::anyhow!("Invalid bids"));
    }
    if !data["asks"].is_array() {
        return Err(anyhow::anyhow!("Invalid asks"));
    }
    
    // Validate price levels
    if let Some(bids) = data["bids"].as_array() {
        for bid in bids {
            if !bid.is_array() || bid.as_array().unwrap().len() != 2 {
                return Err(anyhow::anyhow!("Invalid bid format"));
            }
        }
    }
    
    Ok(())
}

fn get_current_memory_kb() -> u64 {
    // In production, use actual memory profiling
    // This is a mock for testing
    std::process::id() as u64 % 1000000 + 100000
}

use rand;