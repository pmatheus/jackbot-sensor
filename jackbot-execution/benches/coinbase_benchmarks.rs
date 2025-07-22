use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use jackbot_execution::client::coinbase::orderbook::{
    CoinbaseOrderBook, OrderBookSnapshot, OrderBookUpdate,
};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::runtime::Runtime;

/// Benchmark order book snapshot processing
fn bench_snapshot_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("orderbook_snapshot_small", |b| {
        b.iter(|| {
            rt.block_on(async {
                let orderbook = CoinbaseOrderBook::new("BTC-USD");
                let snapshot = create_snapshot(10); // 10 levels each side
                orderbook.apply_snapshot(black_box(snapshot)).await;
            });
        });
    });
    
    c.bench_function("orderbook_snapshot_medium", |b| {
        b.iter(|| {
            rt.block_on(async {
                let orderbook = CoinbaseOrderBook::new("BTC-USD");
                let snapshot = create_snapshot(100); // 100 levels each side
                orderbook.apply_snapshot(black_box(snapshot)).await;
            });
        });
    });
    
    c.bench_function("orderbook_snapshot_large", |b| {
        b.iter(|| {
            rt.block_on(async {
                let orderbook = CoinbaseOrderBook::new("BTC-USD");
                let snapshot = create_snapshot(1000); // 1000 levels each side
                orderbook.apply_snapshot(black_box(snapshot)).await;
            });
        });
    });
}

/// Benchmark incremental updates
fn bench_incremental_updates(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("orderbook_updates");
    
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_custom(|iters| {
                let orderbook = rt.block_on(async {
                    let ob = CoinbaseOrderBook::new("BTC-USD");
                    ob.apply_snapshot(create_snapshot(size)).await;
                    ob
                });
                
                let start = std::time::Instant::now();
                
                rt.block_on(async {
                    for i in 0..iters {
                        let update = OrderBookUpdate {
                            sequence: 1001 + i,
                            side: if i % 2 == 0 { "buy" } else { "sell" },
                            price: Decimal::from_str(&format!("{}.{:02}", 50000 + (i % 20), i % 100)).unwrap(),
                            size: if i % 10 == 0 { Decimal::ZERO } else { Decimal::from(i % 5 + 1) },
                        };
                        
                        let _ = orderbook.apply_update(black_box(update)).await;
                    }
                });
                
                start.elapsed()
            });
        });
    }
    
    group.finish();
}

/// Benchmark best bid/ask retrieval
fn bench_best_bid_ask(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("get_best_bid_ask", |b| {
        let orderbook = rt.block_on(async {
            let ob = CoinbaseOrderBook::new("BTC-USD");
            ob.apply_snapshot(create_snapshot(1000)).await;
            ob
        });
        
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(orderbook.get_best_bid_ask().await);
            });
        });
    });
}

/// Benchmark checksum calculation
fn bench_checksum_calculation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("checksum");
    
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let orderbook = rt.block_on(async {
                let ob = CoinbaseOrderBook::new("BTC-USD");
                ob.apply_snapshot(create_snapshot(size)).await;
                ob
            });
            
            b.iter(|| {
                rt.block_on(async {
                    let _ = black_box(orderbook.calculate_checksum().await);
                });
            });
        });
    }
    
    group.finish();
}

/// Benchmark WebSocket message parsing
fn bench_message_parsing(c: &mut Criterion) {
    use serde_json::json;
    
    let snapshot_json = json!({
        "type": "snapshot",
        "product_id": "BTC-USD",
        "bids": [["50000.00", "1.5"], ["49999.00", "2.0"]],
        "asks": [["50001.00", "1.2"], ["50002.00", "2.5"]]
    }).to_string();
    
    let update_json = json!({
        "type": "l2update",
        "product_id": "BTC-USD",
        "time": "2024-01-01T00:00:00.000Z",
        "changes": [
            ["buy", "50000.50", "1.0"],
            ["sell", "50001.50", "0"]
        ]
    }).to_string();
    
    let trade_json = json!({
        "type": "match",
        "trade_id": 12345,
        "sequence": 1234567890,
        "maker_order_id": "maker123",
        "taker_order_id": "taker456",
        "time": "2024-01-01T00:00:00.000Z",
        "product_id": "BTC-USD",
        "size": "0.1",
        "price": "50000.00",
        "side": "buy"
    }).to_string();
    
    c.bench_function("parse_snapshot_message", |b| {
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(&snapshot_json)).unwrap();
        });
    });
    
    c.bench_function("parse_update_message", |b| {
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(&update_json)).unwrap();
        });
    });
    
    c.bench_function("parse_trade_message", |b| {
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(&trade_json)).unwrap();
        });
    });
}

/// Benchmark memory efficiency with concurrent updates
fn bench_concurrent_updates(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("concurrent_orderbook_updates", |b| {
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            
            rt.block_on(async {
                let orderbook = std::sync::Arc::new(CoinbaseOrderBook::new("BTC-USD"));
                orderbook.apply_snapshot(create_snapshot(100)).await;
                
                let mut handles = vec![];
                
                for thread_id in 0..4 {
                    let ob = orderbook.clone();
                    let handle = tokio::spawn(async move {
                        for i in 0..(iters / 4) {
                            let update = OrderBookUpdate {
                                sequence: 1001 + thread_id * 1000 + i,
                                side: if i % 2 == 0 { "buy" } else { "sell" },
                                price: Decimal::from_str(&format!("{}.{:02}", 50000 + (i % 10), i % 100)).unwrap(),
                                size: Decimal::from(i % 5 + 1),
                            };
                            
                            let _ = ob.apply_update(update).await;
                        }
                    });
                    handles.push(handle);
                }
                
                for handle in handles {
                    let _ = handle.await;
                }
            });
            
            start.elapsed()
        });
    });
}

/// Helper function to create test snapshots
fn create_snapshot(levels: usize) -> OrderBookSnapshot {
    let mut bids = Vec::with_capacity(levels);
    let mut asks = Vec::with_capacity(levels);
    
    for i in 0..levels {
        let bid_price = 50000.0 - (i as f64 * 0.01);
        let ask_price = 50001.0 + (i as f64 * 0.01);
        
        bids.push((
            Decimal::from_str(&format!("{:.2}", bid_price)).unwrap(),
            Decimal::from_str(&format!("{:.2}", 1.0 + (i as f64 * 0.1))).unwrap(),
        ));
        
        asks.push((
            Decimal::from_str(&format!("{:.2}", ask_price)).unwrap(),
            Decimal::from_str(&format!("{:.2}", 1.0 + (i as f64 * 0.1))).unwrap(),
        ));
    }
    
    OrderBookSnapshot {
        product_id: "BTC-USD".to_string(),
        sequence: 1000,
        bids,
        asks,
    }
}

// Performance validation tests
#[test]
fn validate_update_latency() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let orderbook = CoinbaseOrderBook::new("BTC-USD");
        orderbook.apply_snapshot(create_snapshot(1000)).await;
        
        let mut latencies = Vec::new();
        
        for i in 0..100 {
            let update = OrderBookUpdate {
                sequence: 1001 + i,
                side: "buy",
                price: Decimal::from_str(&format!("{}.00", 50000 + i)).unwrap(),
                size: Decimal::from(1),
            };
            
            let start = std::time::Instant::now();
            let _ = orderbook.apply_update(update).await;
            let latency = start.elapsed();
            
            latencies.push(latency);
        }
        
        let avg_latency = latencies.iter().sum::<std::time::Duration>() / latencies.len() as u32;
        let max_latency = latencies.iter().max().unwrap();
        
        println!("Average update latency: {:?}", avg_latency);
        println!("Max update latency: {:?}", max_latency);
        
        // Verify <10ms requirement
        assert!(avg_latency < std::time::Duration::from_millis(10), 
                "Average latency exceeds 10ms: {:?}", avg_latency);
        assert!(max_latency < std::time::Duration::from_millis(20), 
                "Max latency exceeds 20ms: {:?}", max_latency);
    });
}

criterion_group!(
    benches,
    bench_snapshot_processing,
    bench_incremental_updates,
    bench_best_bid_ask,
    bench_checksum_calculation,
    bench_message_parsing,
    bench_concurrent_updates,
);

criterion_main!(benches);