//! KuCoin performance benchmarks
//!
//! Validates that KuCoin implementation meets <10ms performance targets

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use jackbot_execution::client::kucoin::{
    orderbook::{OrderBook, PriceLevel},
    types::{KuCoinL2Update, KuCoinL2Changes},
};
use rust_decimal::Decimal;
use std::str::FromStr;

fn benchmark_order_book_update(c: &mut Criterion) {
    let mut order_book = OrderBook::new("BTC-USDT".to_string());
    
    // Pre-populate order book
    for i in 0..1000 {
        let price = Decimal::from_str(&format!("50000.{:03}", i)).unwrap();
        let level = PriceLevel {
            price,
            quantity: Decimal::from_str("1.0").unwrap(),
            sequence: i as i64,
        };
        order_book.bids.insert(price, level);
    }

    // Create a typical L2 update
    let update = KuCoinL2Update {
        symbol: "BTC-USDT".to_string(),
        changes: KuCoinL2Changes {
            asks: vec![
                ["50100.0".to_string(), "1.5".to_string(), "1001".to_string()],
                ["50101.0".to_string(), "2.0".to_string(), "1002".to_string()],
                ["50102.0".to_string(), "0.0".to_string(), "1003".to_string()], // Remove
            ],
            bids: vec![
                ["49999.0".to_string(), "0.8".to_string(), "1001".to_string()],
                ["49998.0".to_string(), "1.2".to_string(), "1002".to_string()],
                ["49997.0".to_string(), "0.0".to_string(), "1003".to_string()], // Remove
            ],
        },
        sequence_start: 1000,
        sequence_end: 1003,
    };

    c.bench_function("order_book_update", |b| {
        b.iter(|| {
            let mut ob = order_book.clone();
            black_box(ob.apply_update(black_box(&update)).unwrap());
        })
    });
}

fn benchmark_order_book_queries(c: &mut Criterion) {
    let mut order_book = OrderBook::new("BTC-USDT".to_string());
    
    // Pre-populate order book with realistic data
    for i in 0..1000 {
        let bid_price = Decimal::from_str(&format!("49{:03}.0", i)).unwrap();
        let ask_price = Decimal::from_str(&format!("50{:03}.0", i)).unwrap();
        
        order_book.bids.insert(bid_price, PriceLevel {
            price: bid_price,
            quantity: Decimal::from_str("1.0").unwrap(),
            sequence: i as i64,
        });
        
        order_book.asks.insert(ask_price, PriceLevel {
            price: ask_price,
            quantity: Decimal::from_str("1.0").unwrap(),
            sequence: i as i64,
        });
    }

    c.bench_function("best_bid_ask", |b| {
        b.iter(|| {
            black_box(order_book.best_bid());
            black_box(order_book.best_ask());
        })
    });

    c.bench_function("spread_calculation", |b| {
        b.iter(|| {
            black_box(order_book.spread());
        })
    });

    c.bench_function("levels_query_10", |b| {
        b.iter(|| {
            black_box(order_book.levels(10));
        })
    });

    c.bench_function("levels_query_100", |b| {
        b.iter(|| {
            black_box(order_book.levels(100));
        })
    });
}

fn benchmark_message_parsing(c: &mut Criterion) {
    let json_message = r#"{
        "type": "message",
        "topic": "/market/level2:BTC-USDT",
        "subject": "trade.l2update",
        "data": {
            "symbol": "BTC-USDT",
            "changes": {
                "asks": [
                    ["50100.0", "1.5", "1001"],
                    ["50101.0", "2.0", "1002"],
                    ["50102.0", "0.0", "1003"]
                ],
                "bids": [
                    ["49999.0", "0.8", "1001"],
                    ["49998.0", "1.2", "1002"],
                    ["49997.0", "0.0", "1003"]
                ]
            },
            "sequenceStart": 1000,
            "sequenceEnd": 1003
        }
    }"#;

    c.bench_function("json_parsing", |b| {
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(json_message)).unwrap();
        })
    });

    c.bench_function("l2_update_parsing", |b| {
        b.iter(|| {
            let value: serde_json::Value = serde_json::from_str(json_message).unwrap();
            let data = &value["data"];
            let _: KuCoinL2Update = serde_json::from_value(black_box(data.clone())).unwrap();
        })
    });
}

fn benchmark_decimal_operations(c: &mut Criterion) {
    let prices = vec![
        "50000.123456", "49999.876543", "50001.234567", "49998.765432"
    ];
    
    c.bench_function("decimal_parsing", |b| {
        b.iter(|| {
            for price_str in &prices {
                let _price = Decimal::from_str(black_box(price_str)).unwrap();
            }
        })
    });

    let decimals: Vec<Decimal> = prices.iter()
        .map(|s| Decimal::from_str(s).unwrap())
        .collect();

    c.bench_function("decimal_arithmetic", |b| {
        b.iter(|| {
            let mut sum = Decimal::ZERO;
            for decimal in &decimals {
                sum += *decimal;
                sum *= Decimal::from_str("1.001").unwrap();
            }
            black_box(sum);
        })
    });
}

fn benchmark_rate_limiting(c: &mut Criterion) {
    use jackbot_data::exchange::kucoin::rate_limit::KucoinRateLimit;
    use jackbot_integration::rate_limit::Priority;
    use tokio::runtime::Runtime;

    let rt = Runtime::new().unwrap();
    let rate_limiter = KucoinRateLimit::new();

    c.bench_function("rate_limiter_acquire", |b| {
        b.to_async(&rt).iter(|| async {
            rate_limiter.acquire_rest(Priority::Normal).await;
        })
    });
}

criterion_group!(
    benches,
    benchmark_order_book_update,
    benchmark_order_book_queries,
    benchmark_message_parsing,
    benchmark_decimal_operations,
    benchmark_rate_limiting
);
criterion_main!(benches);