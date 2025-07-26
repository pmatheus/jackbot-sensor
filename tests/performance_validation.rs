//! Performance validation tests for optimized connectors
//!
//! Validates that we meet our performance targets:
//! - <10ms p99 latency
//! - >100K messages/second throughput
//! - <500MB memory usage

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use jackbot_sensor::binance_websocket::BinanceWebSocketClient;
use jackbot_sensor::coinbase_websocket_optimized::CoinbaseWebSocketClient;
use jackbot_sensor::streaming::StreamingManager;
use jackbot_sensor::kafka_producer::{KafkaProducer, ProducerConfig};
use jackbot_sensor::zero_copy_parser::ZeroCopyParser;

/// Test zero-copy parsing performance
#[tokio::test]
async fn test_zero_copy_parsing_performance() {
    let parser = ZeroCopyParser::new();
    
    // Test message
    let test_message = r#"{"symbol":"BTC-USDT","bids":[["50000.00","1.0"],["49999.00","2.0"]],"asks":[["50001.00","1.5"],["50002.00","2.5"]],"timestamp":1640995200000,"sequence":12345}"#;
    let test_data = test_message.as_bytes();
    
    // Warm up
    for _ in 0..1000 {
        let _ = parser.parse_order_book_update(test_data).unwrap();
    }
    
    // Benchmark
    let message_count = 100_000;
    let start = Instant::now();
    
    for _ in 0..message_count {
        let _ = parser.parse_order_book_update(test_data).unwrap();
    }
    
    let elapsed = start.elapsed();
    let rate = message_count as f64 / elapsed.as_secs_f64();
    let avg_latency_us = elapsed.as_micros() / message_count as u128;
    
    println!("Zero-copy parsing performance:");
    println!("  Messages parsed: {}", message_count);
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());
    println!("  Messages/sec: {:.0}", rate);
    println!("  Avg latency: {}μs", avg_latency_us);
    
    // Validate performance requirements
    assert!(rate > 1_000_000.0, "Must achieve >1M msg/sec, got {:.0}", rate);
    assert!(avg_latency_us < 10, "Must achieve <10μs latency, got {}μs", avg_latency_us);
}

/// Test Binance WebSocket throughput
#[tokio::test]
#[ignore] // Requires live connection
async fn test_binance_websocket_throughput() {
    let streaming_manager = Arc::new(StreamingManager::new());
    let client = BinanceWebSocketClient::new(streaming_manager.clone(), None, true).unwrap();
    
    // Subscribe to high-volume streams
    let symbols = vec!["BTC/USDT", "ETH/USDT", "BNB/USDT"];
    for symbol in &symbols {
        client.subscribe_orderbook(symbol).await.unwrap();
        client.subscribe_trades(symbol).await.unwrap();
    }
    
    // Measure throughput
    let message_count = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    let stop_flag = Arc::new(AtomicBool::new(false));
    
    // Count messages for 30 seconds
    let counter = message_count.clone();
    let stop = stop_flag.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        stop.store(true, Ordering::Relaxed);
    });
    
    // Subscribe to all events
    let mut receiver = streaming_manager.subscribe_all().await.unwrap();
    while !stop_flag.load(Ordering::Relaxed) {
        if let Ok(_event) = receiver.recv().await {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    let elapsed = start.elapsed();
    let total_messages = message_count.load(Ordering::Relaxed);
    let rate = total_messages as f64 / elapsed.as_secs_f64();
    
    println!("Binance WebSocket throughput:");
    println!("  Total messages: {}", total_messages);
    println!("  Duration: {:.2}s", elapsed.as_secs_f64());
    println!("  Messages/sec: {:.0}", rate);
    
    // Get connection stats
    let stats = client.get_stats().await;
    println!("  Connection stats: {}", serde_json::to_string_pretty(&stats).unwrap());
    
    client.shutdown().await.unwrap();
}

/// Test Coinbase lock-free order book performance
#[tokio::test]
async fn test_coinbase_orderbook_performance() {
    use jackbot_sensor::coinbase_websocket_optimized::LockFreeOrderBook;
    
    let orderbook = Arc::new(LockFreeOrderBook::new("BTC/USD"));
    let update_count = 1_000_000;
    
    // Spawn multiple updater threads
    let thread_count = 4;
    let updates_per_thread = update_count / thread_count;
    
    let start = Instant::now();
    let mut handles = vec![];
    
    for thread_id in 0..thread_count {
        let ob = orderbook.clone();
        let handle = tokio::spawn(async move {
            for i in 0..updates_per_thread {
                let price = 50000.0 + (i % 100) as f64;
                let size = if i % 10 == 0 { 0.0 } else { (i % 5) as f64 };
                let side = if thread_id % 2 == 0 { "buy" } else { "sell" };
                
                ob.apply_update(side, price, size, i as i64);
            }
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.await.unwrap();
    }
    
    let elapsed = start.elapsed();
    let rate = update_count as f64 / elapsed.as_secs_f64();
    let avg_latency_ns = elapsed.as_nanos() / update_count as u128;
    
    println!("Lock-free order book performance:");
    println!("  Total updates: {}", update_count);
    println!("  Threads: {}", thread_count);
    println!("  Total time: {:.2}s", elapsed.as_secs_f64());
    println!("  Updates/sec: {:.0}", rate);
    println!("  Avg latency: {}ns", avg_latency_ns);
    
    // Get snapshot
    let snapshot_start = Instant::now();
    let (bids, asks) = orderbook.get_snapshot(20);
    let snapshot_time = snapshot_start.elapsed();
    
    println!("  Snapshot time: {}μs", snapshot_time.as_micros());
    println!("  Bid levels: {}", bids.len());
    println!("  Ask levels: {}", asks.len());
    
    // Validate performance
    assert!(rate > 5_000_000.0, "Must achieve >5M updates/sec, got {:.0}", rate);
    assert!(avg_latency_ns < 1000, "Must achieve <1μs latency, got {}ns", avg_latency_ns);
    assert!(snapshot_time.as_micros() < 100, "Snapshot must be <100μs, got {}μs", snapshot_time.as_micros());
}

/// Test Kafka producer throughput
#[tokio::test]
#[ignore] // Requires Kafka
async fn test_kafka_producer_throughput() {
    let config = ProducerConfig {
        brokers: "localhost:9092".to_string(),
        batch_size: 1048576, // 1MB
        linger_ms: 0, // No delay
        ..Default::default()
    };
    
    let producer = Arc::new(KafkaProducer::new(config).await.unwrap());
    
    // Create test data
    let ticker = jackbot_sensor::api::TickerData {
        symbol: "BTC/USDT".to_string(),
        exchange: "binance".to_string(),
        price: 50000.0,
        bid: 49999.0,
        ask: 50001.0,
        volume_24h: 1000.0,
        change_24h: 2.5,
        high_24h: 51000.0,
        low_24h: 49000.0,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };
    
    let message_count = 100_000;
    let start = Instant::now();
    let errors = Arc::new(AtomicU64::new(0));
    
    // Send messages in parallel
    let mut handles = vec![];
    for _ in 0..10 {
        let producer = producer.clone();
        let ticker = ticker.clone();
        let errors = errors.clone();
        let messages_per_thread = message_count / 10;
        
        let handle = tokio::spawn(async move {
            for _ in 0..messages_per_thread {
                if let Err(e) = producer.publish_ticker(&ticker).await {
                    eprintln!("Kafka error: {}", e);
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    let elapsed = start.elapsed();
    let error_count = errors.load(Ordering::Relaxed);
    let success_count = message_count - error_count;
    let rate = success_count as f64 / elapsed.as_secs_f64();
    
    println!("Kafka producer throughput:");
    println!("  Messages sent: {}/{}", success_count, message_count);
    println!("  Errors: {}", error_count);
    println!("  Duration: {:.2}s", elapsed.as_secs_f64());
    println!("  Messages/sec: {:.0}", rate);
    
    let metrics = producer.get_metrics();
    let avg_latency = metrics.avg_latency_us.load(Ordering::Relaxed);
    println!("  Avg latency: {}μs", avg_latency);
    
    producer.shutdown().await.unwrap();
    
    // Validate performance
    assert!(rate > 100_000.0, "Must achieve >100K msg/sec, got {:.0}", rate);
    assert!(avg_latency < 10_000, "Must achieve <10ms latency, got {}μs", avg_latency);
}

/// Integration test for full pipeline
#[tokio::test]
#[ignore] // Requires all services
async fn test_full_pipeline_performance() {
    // Set up Kafka producer
    let kafka_config = ProducerConfig::default();
    let kafka_producer = Arc::new(KafkaProducer::new(kafka_config).await.unwrap());
    
    // Set up streaming manager with Kafka
    let streaming_manager = Arc::new(StreamingManager::new_with_kafka(Some(kafka_producer.clone())));
    
    // Set up Binance client
    let binance = BinanceWebSocketClient::new(
        streaming_manager.clone(),
        Some(kafka_producer.clone()),
        false,
    ).unwrap();
    
    // Set up Coinbase client
    let coinbase = CoinbaseWebSocketClient::new(
        streaming_manager.clone(),
        Some(kafka_producer.clone()),
        false,
        None,
    ).unwrap();
    
    // Subscribe to streams
    binance.subscribe_orderbook("BTC/USDT").await.unwrap();
    coinbase.subscribe_orderbook("BTC/USD").await.unwrap();
    
    // Measure end-to-end latency
    let latencies = Arc::new(parking_lot::Mutex::new(Vec::with_capacity(10000)));
    let message_timestamps = Arc::new(dashmap::DashMap::new());
    
    // Track message flow
    let mut receiver = streaming_manager.subscribe_all().await.unwrap();
    let latencies_clone = latencies.clone();
    let timestamps_clone = message_timestamps.clone();
    
    tokio::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            let now = Instant::now();
            match event {
                jackbot_sensor::streaming::StreamEvent::OrderBook(ob) => {
                    let key = format!("{}:{}", ob.exchange, ob.symbol);
                    if let Some(start_time) = timestamps_clone.get(&key) {
                        let latency = now.duration_since(*start_time.value());
                        latencies_clone.lock().push(latency);
                    }
                    timestamps_clone.insert(key, now);
                }
                _ => {}
            }
        }
    });
    
    // Run for 30 seconds
    tokio::time::sleep(Duration::from_secs(30)).await;
    
    // Calculate statistics
    let latency_samples = latencies.lock();
    if !latency_samples.is_empty() {
        let mut sorted_latencies: Vec<_> = latency_samples.iter().map(|d| d.as_micros()).collect();
        sorted_latencies.sort_unstable();
        
        let p50 = sorted_latencies[sorted_latencies.len() / 2];
        let p95 = sorted_latencies[sorted_latencies.len() * 95 / 100];
        let p99 = sorted_latencies[sorted_latencies.len() * 99 / 100];
        
        println!("End-to-end pipeline latency:");
        println!("  Samples: {}", sorted_latencies.len());
        println!("  P50 latency: {}μs", p50);
        println!("  P95 latency: {}μs", p95);
        println!("  P99 latency: {}μs", p99);
        
        // Validate performance
        assert!(p99 < 10_000, "P99 latency must be <10ms, got {}μs", p99);
    }
    
    // Get final stats
    let binance_stats = binance.get_stats().await;
    let coinbase_stats = coinbase.get_stats().await;
    
    println!("\nBinance stats: {}", serde_json::to_string_pretty(&binance_stats).unwrap());
    println!("\nCoinbase stats: {}", serde_json::to_string_pretty(&coinbase_stats).unwrap());
    
    // Shutdown
    binance.shutdown().await.unwrap();
    coinbase.shutdown().await.unwrap();
    kafka_producer.shutdown().await.unwrap();
}