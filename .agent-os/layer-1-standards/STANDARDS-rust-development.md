# Rust Development Standards
**Layer 1 - Global Standards**  
**Version**: 1.0.0  
**Last Updated**: 2025-07-26

## Overview

These standards apply to all Rust code across the Jackbot platform. They ensure consistency, maintainability, and performance across our distributed trading system.

## Code Style

### Formatting
- Use `rustfmt` with the project configuration
- Maximum line length: 100 characters
- Indent with 4 spaces
- Use trailing commas in multi-line constructs

### Naming Conventions
```rust
// Modules: snake_case
mod order_processor;

// Types: PascalCase
struct OrderBook;
enum OrderStatus;
trait MessageHandler;

// Functions/Methods: snake_case
fn process_order() {}

// Constants: SCREAMING_SNAKE_CASE
const MAX_RECONNECT_ATTEMPTS: u32 = 5;

// Variables: snake_case
let order_count = 0;
```

### Import Organization
```rust
// Standard library
use std::collections::HashMap;
use std::sync::Arc;

// External crates
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

// Internal crates
use jackbot_common::types::Order;

// Local modules
use crate::processor::OrderProcessor;
```

## Error Handling

### Error Types
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Connection error: {0}")]
    Connection(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Validation error: {message}")]
    Validation { message: String },
    
    #[error("Rate limit exceeded")]
    RateLimit,
}

// Result type alias
pub type Result<T> = std::result::Result<T, ServiceError>;
```

### Error Propagation
```rust
// Prefer ? operator
async fn fetch_order(id: &str) -> Result<Order> {
    let data = fetch_from_db(id).await?;
    let order = serde_json::from_str(&data)?;
    Ok(order)
}

// Add context when needed
async fn process_order(id: &str) -> Result<()> {
    fetch_order(id)
        .await
        .map_err(|e| ServiceError::Validation {
            message: format!("Failed to process order {}: {}", id, e)
        })?;
    Ok(())
}
```

## Type Safety

### No Primitive Obsession
```rust
// Bad
fn calculate_price(amount: f64, rate: f64) -> f64 { }

// Good
use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy)]
struct Price(Decimal);

#[derive(Debug, Clone, Copy)]
struct Quantity(Decimal);

#[derive(Debug, Clone, Copy)]
struct Rate(Decimal);

fn calculate_price(quantity: Quantity, rate: Rate) -> Price {
    Price(quantity.0 * rate.0)
}
```

### Builder Pattern for Complex Types
```rust
#[derive(Debug, Default)]
pub struct OrderBuilder {
    symbol: Option<String>,
    side: Option<OrderSide>,
    quantity: Option<Decimal>,
    price: Option<Decimal>,
}

impl OrderBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }
    
    pub fn build(self) -> Result<Order> {
        Ok(Order {
            symbol: self.symbol.ok_or(ServiceError::Validation {
                message: "Symbol is required".to_string()
            })?,
            // ... other fields
        })
    }
}
```

## Async Programming

### Tokio Runtime
```rust
// Main entry point
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize once
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .enable_all()
        .build()?;
        
    runtime.block_on(async {
        run_application().await
    })
}
```

### Cancellation Safety
```rust
use tokio_util::sync::CancellationToken;

pub struct Service {
    cancellation_token: CancellationToken,
}

impl Service {
    pub async fn run(&self) -> Result<()> {
        tokio::select! {
            result = self.process_messages() => result,
            _ = self.cancellation_token.cancelled() => {
                info!("Service cancelled");
                Ok(())
            }
        }
    }
}
```

### Channel Selection
```rust
// Bounded channels for backpressure
let (tx, rx) = tokio::sync::mpsc::channel::<Message>(1000);

// Unbounded for critical paths (use sparingly)
let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Alert>();

// Broadcast for multiple consumers
let (tx, mut rx1) = tokio::sync::broadcast::channel::<MarketData>(100);
let mut rx2 = tx.subscribe();
```

## Performance

### Zero-Copy Parsing
```rust
use nom::IResult;
use nom::bytes::complete::tag;
use nom::character::complete::digit1;

fn parse_price(input: &[u8]) -> IResult<&[u8], u64> {
    let (input, _) = tag(b"price:")(input)?;
    let (input, price_str) = digit1(input)?;
    let price = std::str::from_utf8(price_str)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    Ok((input, price))
}
```

### Memory Pools
```rust
use crossbeam::queue::ArrayQueue;

pub struct MessagePool {
    pool: ArrayQueue<Box<Message>>,
}

impl MessagePool {
    pub fn new(capacity: usize) -> Self {
        let pool = ArrayQueue::new(capacity);
        for _ in 0..capacity {
            let _ = pool.push(Box::new(Message::default()));
        }
        Self { pool }
    }
    
    pub fn acquire(&self) -> Box<Message> {
        self.pool.pop().unwrap_or_else(|| Box::new(Message::default()))
    }
    
    pub fn release(&self, mut msg: Box<Message>) {
        msg.clear();
        let _ = self.pool.push(msg);
    }
}
```

## Testing

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_order_creation() {
        let order = OrderBuilder::new()
            .symbol("BTCUSDT")
            .side(OrderSide::Buy)
            .quantity(Decimal::from(1))
            .build()
            .unwrap();
            
        assert_eq!(order.symbol, "BTCUSDT");
    }
    
    #[tokio::test]
    async fn test_async_processing() {
        let service = Service::new();
        let result = service.process_order("123").await;
        assert!(result.is_ok());
    }
}
```

### Property-Based Testing
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_price_calculation(
        quantity in 0.0001f64..10000.0,
        rate in 0.01f64..100000.0
    ) {
        let price = calculate_price(
            Quantity(Decimal::from_f64(quantity).unwrap()),
            Rate(Decimal::from_f64(rate).unwrap())
        );
        prop_assert!(price.0 > Decimal::ZERO);
    }
}
```

### Benchmarks
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_order_parsing(c: &mut Criterion) {
    let order_json = r#"{"symbol":"BTCUSDT","side":"buy","quantity":1.0}"#;
    
    c.bench_function("parse_order", |b| {
        b.iter(|| {
            let order: Order = serde_json::from_str(black_box(order_json)).unwrap();
            order
        })
    });
}

criterion_group!(benches, benchmark_order_parsing);
criterion_main!(benches);
```

## Documentation

### Module Documentation
```rust
//! Order processing module
//! 
//! This module handles the complete lifecycle of orders from creation
//! to execution. It provides:
//! 
//! - Order validation and normalization
//! - Exchange routing logic
//! - Execution tracking and reporting
//! 
//! # Examples
//! 
//! ```
//! use order_processor::OrderProcessor;
//! 
//! let processor = OrderProcessor::new();
//! let order = create_test_order();
//! processor.process(order).await?;
//! ```

/// Processes orders with automatic retry logic
/// 
/// # Arguments
/// 
/// * `order` - The order to process
/// 
/// # Returns
/// 
/// * `Ok(OrderResult)` - Successfully processed order
/// * `Err(ServiceError)` - Processing failed
/// 
/// # Errors
/// 
/// This function will return an error if:
/// - The order validation fails
/// - The exchange rejects the order
/// - Network timeout occurs
pub async fn process_order(order: Order) -> Result<OrderResult> {
    // Implementation
}
```

## Dependencies

### Cargo.toml Best Practices
```toml
[dependencies]
# Core async runtime
tokio = { version = "1.40", features = ["full"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Error handling
thiserror = "2.0"
anyhow = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Testing
[dev-dependencies]
proptest = "1.0"
criterion = { version = "0.5", features = ["async_tokio"] }
tokio-test = "0.4"

# Benchmarking
[[bench]]
name = "performance"
harness = false
```

### Version Management
- Pin major versions for stability
- Use workspace dependencies for consistency
- Run `cargo update` weekly
- Security audit before deployment

## Security

### No Unsafe Code
- Avoid `unsafe` blocks unless absolutely necessary
- If required, document safety invariants
- Prefer safe abstractions

### Input Validation
```rust
use validator::Validate;

#[derive(Debug, Validate)]
pub struct OrderRequest {
    #[validate(length(min = 1, max = 20))]
    pub symbol: String,
    
    #[validate(range(min = 0.0001, max = 100000.0))]
    pub quantity: f64,
    
    #[validate(custom = "validate_price")]
    pub price: Option<f64>,
}

fn validate_price(price: &Option<f64>) -> Result<(), validator::ValidationError> {
    if let Some(p) = price {
        if *p <= 0.0 {
            return Err(validator::ValidationError::new("price_must_be_positive"));
        }
    }
    Ok(())
}
```

## Monitoring

### Structured Logging
```rust
use tracing::{info, warn, error, instrument};

#[instrument(skip(order), fields(order_id = %order.id))]
pub async fn process_order(order: Order) -> Result<()> {
    info!("Processing order");
    
    match validate_order(&order) {
        Ok(_) => info!("Order validated"),
        Err(e) => {
            error!(error = %e, "Order validation failed");
            return Err(e);
        }
    }
    
    Ok(())
}
```

### Metrics
```rust
use prometheus::{register_counter, register_histogram};

lazy_static! {
    static ref ORDER_COUNTER: Counter = register_counter!(
        "orders_processed_total",
        "Total number of orders processed"
    ).unwrap();
    
    static ref ORDER_LATENCY: Histogram = register_histogram!(
        "order_processing_duration_seconds",
        "Order processing latency"
    ).unwrap();
}

pub async fn process_with_metrics(order: Order) -> Result<()> {
    let timer = ORDER_LATENCY.start_timer();
    let result = process_order(order).await;
    timer.observe_duration();
    
    if result.is_ok() {
        ORDER_COUNTER.inc();
    }
    
    result
}
```

## Code Review Checklist

- [ ] No compiler warnings
- [ ] All public items documented
- [ ] Error handling uses Result types
- [ ] No unwrap() in production code
- [ ] Tests cover happy and error paths
- [ ] Benchmarks for performance-critical code
- [ ] Dependencies are necessary and up-to-date
- [ ] No sensitive data in logs
- [ ] Metrics and tracing added
- [ ] Resource cleanup in Drop implementations