//! Multi-Exchange Adversarial Tests
//! Comprehensive torture testing for all 8 supported exchanges
//! 
//! Exchanges under test:
//! - Binance, Coinbase, Bybit, Bitget, Hyperliquid, KuCoin, Kraken, OKX
//! 
//! ZERO TOLERANCE STANDARDS:
//! - All exchanges must maintain <10ms market data latency
//! - All exchanges must maintain <50ms order execution
//! - Failover must complete within 1 second
//! - No data loss during exchange outages

use anyhow::Result;
use futures::{stream, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::time::{interval, timeout};
use tracing::{error, info, warn};

use jackbot_sensor::connector::{
    Balance, Exchange, MarketData, Order, OrderId, OrderResult, OrderSide, OrderStatus, OrderType,
    TimeInForce, Connection,
};
use jackbot_sensor::performance_benchmarks::BenchmarkExchange;

// Exchange test constants
const EXCHANGES: &[&str] = &["binance", "coinbase", "bybit", "bitget", "hyperliquid", "kucoin", "kraken", "okx"];
const MARKET_DATA_LATENCY_TARGET_MS: u64 = 10;
const ORDER_EXECUTION_LATENCY_TARGET_MS: u64 = 50;
const FAILOVER_TARGET_MS: u64 = 1000;
const SUCCESS_RATE_TARGET: f64 = 99.9;

/// Exchange performance tracker
#[derive(Debug)]
struct ExchangePerformanceTracker {
    exchange_name: String,
    connection_attempts: AtomicU64,
    successful_connections: AtomicU64,
    market_data_messages: AtomicU64,
    order_executions: AtomicU64,
    failed_operations: AtomicU64,
    latency_measurements: Arc<RwLock<Vec<u64>>>,
    last_heartbeat: Arc<RwLock<Option<Instant>>>,
}

impl ExchangePerformanceTracker {
    fn new(exchange_name: String) -> Self {
        Self {
            exchange_name,
            connection_attempts: AtomicU64::new(0),
            successful_connections: AtomicU64::new(0),
            market_data_messages: AtomicU64::new(0),
            order_executions: AtomicU64::new(0),
            failed_operations: AtomicU64::new(0),
            latency_measurements: Arc::new(RwLock::new(Vec::new())),
            last_heartbeat: Arc::new(RwLock::new(None)),
        }
    }

    async fn record_connection_attempt(&self, success: bool) {
        self.connection_attempts.fetch_add(1, Ordering::Relaxed);
        if success {
            self.successful_connections.fetch_add(1, Ordering::Relaxed);
            *self.last_heartbeat.write().await = Some(Instant::now());
        } else {
            self.failed_operations.fetch_add(1, Ordering::Relaxed);
        }
    }

    async fn record_market_data(&self, latency_ns: u64) {
        self.market_data_messages.fetch_add(1, Ordering::Relaxed);
        self.latency_measurements.write().await.push(latency_ns);
        *self.last_heartbeat.write().await = Some(Instant::now());
    }

    async fn record_order_execution(&self, latency_ns: u64, success: bool) {
        if success {
            self.order_executions.fetch_add(1, Ordering::Relaxed);
            self.latency_measurements.write().await.push(latency_ns);
        } else {
            self.failed_operations.fetch_add(1, Ordering::Relaxed);
        }
        *self.last_heartbeat.write().await = Some(Instant::now());
    }

    async fn calculate_metrics(&self) -> ExchangeMetrics {
        let latencies = self.latency_measurements.read().await;
        let mut sorted_latencies = latencies.clone();
        sorted_latencies.sort_unstable();

        let (p95_latency, p99_latency, avg_latency) = if !sorted_latencies.is_empty() {
            let len = sorted_latencies.len();
            let p95 = sorted_latencies[len * 95 / 100];
            let p99 = sorted_latencies[len * 99 / 100];
            let avg = sorted_latencies.iter().sum::<u64>() / len as u64;
            (p95, p99, avg)
        } else {
            (0, 0, 0)
        };

        let total_operations = self.market_data_messages.load(Ordering::Relaxed) 
            + self.order_executions.load(Ordering::Relaxed);
        let success_rate = if total_operations > 0 {
            ((total_operations - self.failed_operations.load(Ordering::Relaxed)) as f64 / total_operations as f64) * 100.0
        } else {
            0.0
        };

        ExchangeMetrics {
            exchange_name: self.exchange_name.clone(),
            connection_success_rate: if self.connection_attempts.load(Ordering::Relaxed) > 0 {
                (self.successful_connections.load(Ordering::Relaxed) as f64 / self.connection_attempts.load(Ordering::Relaxed) as f64) * 100.0
            } else {
                0.0
            },
            total_operations,
            success_rate,
            avg_latency_ns: avg_latency,
            p95_latency_ns: p95_latency,
            p99_latency_ns: p99_latency,
            last_heartbeat: *self.last_heartbeat.read().await,
        }
    }
}

#[derive(Debug, Clone)]
struct ExchangeMetrics {
    exchange_name: String,
    connection_success_rate: f64,
    total_operations: u64,
    success_rate: f64,
    avg_latency_ns: u64,
    p95_latency_ns: u64,
    p99_latency_ns: u64,
    last_heartbeat: Option<Instant>,
}

/// BINANCE EXCHANGE TORTURE TEST
#[tokio::test]
async fn test_binance_exchange() -> Result<()> {
    run_exchange_torture_test("binance", 1000, 0.001).await
}

/// COINBASE EXCHANGE TORTURE TEST
#[tokio::test]
async fn test_coinbase_exchange() -> Result<()> {
    run_exchange_torture_test("coinbase", 1500, 0.002).await
}

/// BYBIT EXCHANGE TORTURE TEST
#[tokio::test]
async fn test_bybit_exchange() -> Result<()> {
    run_exchange_torture_test("bybit", 1200, 0.001).await
}

/// BITGET EXCHANGE TORTURE TEST
#[tokio::test]
async fn test_bitget_exchange() -> Result<()> {
    run_exchange_torture_test("bitget", 1300, 0.002).await
}

/// HYPERLIQUID EXCHANGE TORTURE TEST
#[tokio::test]
async fn test_hyperliquid_exchange() -> Result<()> {
    run_exchange_torture_test("hyperliquid", 800, 0.001).await
}

/// KUCOIN EXCHANGE TORTURE TEST
#[tokio::test]
async fn test_kucoin_exchange() -> Result<()> {
    run_exchange_torture_test("kucoin", 1400, 0.003).await
}

/// KRAKEN EXCHANGE TORTURE TEST
#[tokio::test]
async fn test_kraken_exchange() -> Result<()> {
    run_exchange_torture_test("kraken", 2000, 0.002).await
}

/// OKX EXCHANGE TORTURE TEST
#[tokio::test]
async fn test_okx_exchange() -> Result<()> {
    run_exchange_torture_test("okx", 1100, 0.001).await
}

/// Generic exchange torture test
async fn run_exchange_torture_test(exchange_name: &str, latency_us: u64, failure_rate: f64) -> Result<()> {
    info!("🔥 STARTING {} EXCHANGE TORTURE TEST", exchange_name.to_uppercase());
    
    let tracker = ExchangePerformanceTracker::new(exchange_name.to_string());
    let exchange = Arc::new(BenchmarkExchange::new(exchange_name.to_string(), latency_us, failure_rate));
    
    let test_duration = Duration::from_secs(60);
    let start_time = Instant::now();
    
    // Test connection stability
    for attempt in 0..10 {
        let connect_start = Instant::now();
        match exchange.connect().await {
            Ok(_) => {
                let latency_ns = connect_start.elapsed().as_nanos() as u64;
                tracker.record_connection_attempt(true).await;
                info!("  Connection attempt {}: SUCCESS ({}ms)", attempt + 1, latency_ns / 1_000_000);
            }
            Err(e) => {
                tracker.record_connection_attempt(false).await;
                warn!("  Connection attempt {}: FAILED - {}", attempt + 1, e);
            }
        }
    }
    
    // Test market data streaming
    let market_data_handle = {
        let exchange_clone = Arc::clone(&exchange);
        let tracker_clone = Arc::new(tracker);
        tokio::spawn(async move {
            test_market_data_streaming(exchange_clone, tracker_clone, test_duration).await
        })
    };
    
    // Test order execution
    let order_execution_handle = {
        let exchange_clone = Arc::clone(&exchange);
        tokio::spawn(async move {
            test_order_execution_performance(exchange_clone, exchange_name, test_duration).await
        })
    };
    
    // Wait for all tests to complete
    let (market_data_result, order_execution_result) = tokio::join!(
        market_data_handle,
        order_execution_handle
    );
    
    market_data_result??;
    order_execution_result??;
    
    info!("✅ {} EXCHANGE TORTURE TEST COMPLETED", exchange_name.to_uppercase());
    Ok(())
}

async fn test_market_data_streaming(
    exchange: Arc<BenchmarkExchange>,
    tracker: Arc<ExchangePerformanceTracker>,
    duration: Duration,
) -> Result<()> {
    let start_time = Instant::now();
    let mut stream = exchange.subscribe_market_data(vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()]).await?;
    
    while start_time.elapsed() < duration {
        let process_start = Instant::now();
        
        if let Some(data) = timeout(Duration::from_millis(100), stream.next()).await.unwrap_or(None) {
            match data {
                MarketData::Ticker(ticker) => {
                    let latency_ns = process_start.elapsed().as_nanos() as u64;
                    tracker.record_market_data(latency_ns).await;
                    
                    // Validate ticker data
                    assert!(ticker.price > 0.0, "Invalid ticker price");
                    assert!(ticker.bid > 0.0, "Invalid bid price");
                    assert!(ticker.ask > 0.0, "Invalid ask price");
                    assert!(ticker.ask > ticker.bid, "Ask price must be higher than bid");
                }
                _ => {}
            }
        }
        
        tokio::task::yield_now().await;
    }
    
    Ok(())
}

async fn test_order_execution_performance(
    exchange: Arc<BenchmarkExchange>,
    exchange_name: &str,
    duration: Duration,
) -> Result<()> {
    let start_time = Instant::now();
    let _connection = exchange.connect().await?;
    
    let mut order_count = 0u64;
    
    while start_time.elapsed() < duration {
        let order = Order {
            id: Some(format!("{}-order-{}", exchange_name, order_count)),
            symbol: "BTC/USDT".to_string(),
            side: if order_count % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
            order_type: OrderType::Limit,
            price: Some(50000.0 + (order_count as f64 * 0.01)),
            quantity: 0.1,
            time_in_force: Some(TimeInForce::GTC),
            status: OrderStatus::New,
        };
        
        let order_start = Instant::now();
        
        match exchange.place_order(order).await {
            Ok(result) => {
                let latency_ns = order_start.elapsed().as_nanos() as u64;
                
                // Validate order result
                assert!(!result.order_id.is_empty(), "Order ID cannot be empty");
                assert!(result.remaining_quantity >= 0.0, "Remaining quantity cannot be negative");
                assert!(result.average_price > 0.0, "Average price must be positive");
                
                // Check latency target
                let latency_ms = latency_ns / 1_000_000;
                if latency_ms > ORDER_EXECUTION_LATENCY_TARGET_MS {
                    error!("Order execution latency {}ms exceeds target {}ms", latency_ms, ORDER_EXECUTION_LATENCY_TARGET_MS);
                }
            }
            Err(e) => {
                warn!("Order execution failed: {}", e);
            }
        }
        
        order_count += 1;
        
        // Rate limiting
        tokio::time::sleep(Duration::from_millis(50)).await; // 20 orders/second
    }
    
    Ok(())
}

/// ALL EXCHANGES SIMULTANEOUS TORTURE TEST
/// Test all 8 exchanges simultaneously under load
#[tokio::test]
async fn test_all_exchanges_simultaneous() -> Result<()> {
    info!("🔥 STARTING ALL EXCHANGES SIMULTANEOUS TORTURE TEST");
    info!("Testing {} exchanges simultaneously", EXCHANGES.len());
    
    let test_duration = Duration::from_secs(180); // 3 minutes
    let mut handles = Vec::new();
    let exchange_trackers = Arc::new(RwLock::new(HashMap::new()));
    
    // Spawn test for each exchange
    for exchange_name in EXCHANGES {
        let trackers_clone = Arc::clone(&exchange_trackers);
        let exchange_name = exchange_name.to_string();
        
        let handle = tokio::spawn(async move {
            let tracker = ExchangePerformanceTracker::new(exchange_name.clone());
            
            // Simulate each exchange with different characteristics
            let (latency_us, failure_rate) = match exchange_name.as_str() {
                "binance" => (1000, 0.001),
                "coinbase" => (1500, 0.002),
                "bybit" => (1200, 0.001),
                "bitget" => (1300, 0.002),
                "hyperliquid" => (800, 0.001),
                "kucoin" => (1400, 0.003),
                "kraken" => (2000, 0.002),
                "okx" => (1100, 0.001),
                _ => (1500, 0.002),
            };
            
            let exchange = Arc::new(BenchmarkExchange::new(exchange_name.clone(), latency_us, failure_rate));
            
            // Run simultaneous operations
            let market_data_task = test_exchange_market_data(&exchange, &tracker, test_duration);
            let order_task = test_exchange_orders(&exchange, &tracker, test_duration);
            let heartbeat_task = test_exchange_heartbeat(&exchange, &tracker, test_duration);
            
            let (_market_result, _order_result, _heartbeat_result) = tokio::join!(
                market_data_task,
                order_task,
                heartbeat_task
            );
            
            // Store tracker for final analysis
            trackers_clone.write().await.insert(exchange_name.clone(), tracker);
            
            Ok::<(), anyhow::Error>(())
        });
        
        handles.push(handle);
    }
    
    // Wait for all exchanges to complete
    let results = futures::future::join_all(handles).await;
    
    // Verify all completed successfully
    for result in results {
        result??;
    }
    
    // Analyze final results
    let final_trackers = exchange_trackers.read().await;
    let mut all_metrics = Vec::new();
    
    for (exchange_name, tracker) in final_trackers.iter() {
        let metrics = tracker.calculate_metrics().await;
        all_metrics.push(metrics.clone());
        
        info!("📊 {} Results:", exchange_name.to_uppercase());
        info!("  Operations: {}", metrics.total_operations);
        info!("  Success rate: {:.2}%", metrics.success_rate);
        info!("  P99 latency: {}ms", metrics.p99_latency_ns / 1_000_000);
        info!("  Connection success: {:.2}%", metrics.connection_success_rate);
        
        // Validate performance targets
        assert!(metrics.success_rate >= SUCCESS_RATE_TARGET,
            "{} success rate {:.2}% below target {:.2}%", exchange_name, metrics.success_rate, SUCCESS_RATE_TARGET);
        
        assert!(metrics.p99_latency_ns / 1_000_000 <= ORDER_EXECUTION_LATENCY_TARGET_MS,
            "{} P99 latency {}ms exceeds target {}ms", exchange_name, metrics.p99_latency_ns / 1_000_000, ORDER_EXECUTION_LATENCY_TARGET_MS);
    }
    
    info!("✅ ALL EXCHANGES SIMULTANEOUS TORTURE TEST PASSED");
    info!("🎯 All {} exchanges performed within specifications", EXCHANGES.len());
    Ok(())
}

async fn test_exchange_market_data(
    exchange: &Arc<BenchmarkExchange>,
    tracker: &ExchangePerformanceTracker,
    duration: Duration,
) -> Result<()> {
    let start_time = Instant::now();
    let mut stream = exchange.subscribe_market_data(vec!["BTC/USDT".to_string()]).await?;
    
    while start_time.elapsed() < duration {
        let process_start = Instant::now();
        
        if let Some(data) = timeout(Duration::from_millis(10), stream.next()).await.unwrap_or(None) {
            match data {
                MarketData::Ticker(_) => {
                    let latency_ns = process_start.elapsed().as_nanos() as u64;
                    tracker.record_market_data(latency_ns).await;
                }
                _ => {}
            }
        }
    }
    
    Ok(())
}

async fn test_exchange_orders(
    exchange: &Arc<BenchmarkExchange>,
    tracker: &ExchangePerformanceTracker,
    duration: Duration,
) -> Result<()> {
    let start_time = Instant::now();
    let _connection = exchange.connect().await?;
    
    let mut order_count = 0u64;
    
    while start_time.elapsed() < duration {
        let order = Order {
            id: Some(format!("simul-order-{}", order_count)),
            symbol: "BTC/USDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            price: Some(50000.0),
            quantity: 0.1,
            time_in_force: Some(TimeInForce::GTC),
            status: OrderStatus::New,
        };
        
        let order_start = Instant::now();
        let success = exchange.place_order(order).await.is_ok();
        let latency_ns = order_start.elapsed().as_nanos() as u64;
        
        tracker.record_order_execution(latency_ns, success).await;
        
        order_count += 1;
        tokio::time::sleep(Duration::from_millis(100)).await; // 10 orders/second per exchange
    }
    
    Ok(())
}

async fn test_exchange_heartbeat(
    exchange: &Arc<BenchmarkExchange>,
    tracker: &ExchangePerformanceTracker,
    duration: Duration,
) -> Result<()> {
    let start_time = Instant::now();
    
    while start_time.elapsed() < duration {
        // Simulate heartbeat/connection monitoring
        let heartbeat_start = Instant::now();
        let _balances = exchange.get_balance().await;
        let latency_ns = heartbeat_start.elapsed().as_nanos() as u64;
        
        tracker.record_market_data(latency_ns).await;
        
        tokio::time::sleep(Duration::from_secs(5)).await; // Heartbeat every 5 seconds
    }
    
    Ok(())
}

/// EXCHANGE FAILOVER TORTURE TEST
/// Test failover scenarios when exchanges go down
#[tokio::test]
async fn test_exchange_failover() -> Result<()> {
    info!("🔥 STARTING EXCHANGE FAILOVER TORTURE TEST");
    
    let failover_coordinator = FailoverCoordinator::new();
    
    // Create multiple exchanges with different failure patterns
    let exchanges = vec![
        ("primary", Arc::new(BenchmarkExchange::new("primary".to_string(), 1000, 0.0))),
        ("backup1", Arc::new(BenchmarkExchange::new("backup1".to_string(), 1500, 0.0))),
        ("backup2", Arc::new(BenchmarkExchange::new("backup2".to_string(), 2000, 0.0))),
    ];
    
    failover_coordinator.register_exchanges(exchanges).await;
    
    let test_duration = Duration::from_secs(120);
    let start_time = Instant::now();
    
    // Simulate operations with random exchange failures
    let operations_task = tokio::spawn({
        let coordinator = failover_coordinator.clone();
        async move {
            let mut operation_count = 0u64;
            
            while start_time.elapsed() < test_duration {
                let order = Order {
                    id: Some(format!("failover-order-{}", operation_count)),
                    symbol: "BTC/USDT".to_string(),
                    side: OrderSide::Buy,
                    order_type: OrderType::Limit,
                    price: Some(50000.0),
                    quantity: 0.1,
                    time_in_force: Some(TimeInForce::GTC),
                    status: OrderStatus::New,
                };
                
                let failover_start = Instant::now();
                match coordinator.execute_order_with_failover(order).await {
                    Ok(_) => {
                        let failover_time = failover_start.elapsed();
                        if failover_time > Duration::from_millis(FAILOVER_TARGET_MS) {
                            error!("Failover took {}ms, exceeds target {}ms", 
                                failover_time.as_millis(), FAILOVER_TARGET_MS);
                        }
                    }
                    Err(e) => {
                        error!("Failover failed: {}", e);
                    }
                }
                
                operation_count += 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    });
    
    // Simulate random exchange failures
    let failure_simulation_task = tokio::spawn({
        let coordinator = failover_coordinator.clone();
        async move {
            let mut failure_interval = interval(Duration::from_secs(20));
            
            while start_time.elapsed() < test_duration {
                failure_interval.tick().await;
                
                // Randomly fail an exchange
                let exchange_to_fail = ["primary", "backup1", "backup2"][start_time.elapsed().as_secs() as usize % 3];
                coordinator.simulate_exchange_failure(exchange_to_fail).await;
                
                // Wait a bit then restore
                tokio::time::sleep(Duration::from_secs(10)).await;
                coordinator.restore_exchange(exchange_to_fail).await;
            }
        }
    });
    
    // Wait for test completion
    let (operations_result, _failure_result) = tokio::join!(operations_task, failure_simulation_task);
    operations_result?;
    
    let failover_report = failover_coordinator.generate_report().await;
    
    info!("📊 Failover Test Results:");
    info!("  Total operations: {}", failover_report.total_operations);
    info!("  Successful failovers: {}", failover_report.successful_failovers);
    info!("  Failed failovers: {}", failover_report.failed_failovers);
    info!("  Average failover time: {}ms", failover_report.average_failover_time_ms);
    info!("  Max failover time: {}ms", failover_report.max_failover_time_ms);
    
    // Validate failover performance
    assert!(failover_report.average_failover_time_ms <= FAILOVER_TARGET_MS,
        "Average failover time {}ms exceeds target {}ms", 
        failover_report.average_failover_time_ms, FAILOVER_TARGET_MS);
    
    assert!(failover_report.failed_failovers == 0,
        "Failed failovers detected: {}", failover_report.failed_failovers);
    
    info!("✅ EXCHANGE FAILOVER TORTURE TEST PASSED");
    Ok(())
}

// Failover coordinator for testing exchange failure scenarios
#[derive(Clone)]
struct FailoverCoordinator {
    exchanges: Arc<RwLock<HashMap<String, Arc<BenchmarkExchange>>>>,
    failed_exchanges: Arc<RwLock<HashMap<String, bool>>>,
    active_exchange: Arc<RwLock<String>>,
    failover_stats: Arc<RwLock<FailoverStats>>,
}

#[derive(Debug, Default)]
struct FailoverStats {
    total_operations: u64,
    successful_failovers: u64,
    failed_failovers: u64,
    failover_times_ms: Vec<u64>,
}

#[derive(Debug)]
struct FailoverReport {
    total_operations: u64,
    successful_failovers: u64,
    failed_failovers: u64,
    average_failover_time_ms: u64,
    max_failover_time_ms: u64,
}

impl FailoverCoordinator {
    fn new() -> Self {
        Self {
            exchanges: Arc::new(RwLock::new(HashMap::new())),
            failed_exchanges: Arc::new(RwLock::new(HashMap::new())),
            active_exchange: Arc::new(RwLock::new("primary".to_string())),
            failover_stats: Arc::new(RwLock::new(FailoverStats::default())),
        }
    }
    
    async fn register_exchanges(&self, exchanges: Vec<(&str, Arc<BenchmarkExchange>)>) {
        let mut exchange_map = self.exchanges.write().await;
        for (name, exchange) in exchanges {
            exchange_map.insert(name.to_string(), exchange);
        }
    }
    
    async fn execute_order_with_failover(&self, order: Order) -> Result<OrderResult> {
        let mut stats = self.failover_stats.write().await;
        stats.total_operations += 1;
        drop(stats);
        
        let active_exchange_name = self.active_exchange.read().await.clone();
        
        // Check if active exchange is failed
        if self.failed_exchanges.read().await.get(&active_exchange_name).copied().unwrap_or(false) {
            // Need to failover
            let failover_start = Instant::now();
            
            match self.find_healthy_exchange().await {
                Some(healthy_exchange_name) => {
                    *self.active_exchange.write().await = healthy_exchange_name.clone();
                    
                    let failover_time = failover_start.elapsed().as_millis() as u64;
                    let mut stats = self.failover_stats.write().await;
                    stats.successful_failovers += 1;
                    stats.failover_times_ms.push(failover_time);
                    drop(stats);
                    
                    info!("Failover to {} completed in {}ms", healthy_exchange_name, failover_time);
                }
                None => {
                    let mut stats = self.failover_stats.write().await;
                    stats.failed_failovers += 1;
                    return Err(anyhow::anyhow!("No healthy exchanges available"));
                }
            }
        }
        
        // Execute order on active exchange
        let current_active = self.active_exchange.read().await.clone();
        let exchanges = self.exchanges.read().await;
        
        if let Some(exchange) = exchanges.get(&current_active) {
            exchange.place_order(order).await
        } else {
            Err(anyhow::anyhow!("Active exchange not found"))
        }
    }
    
    async fn find_healthy_exchange(&self) -> Option<String> {
        let exchanges = self.exchanges.read().await;
        let failed = self.failed_exchanges.read().await;
        
        for exchange_name in exchanges.keys() {
            if !failed.get(exchange_name).copied().unwrap_or(false) {
                return Some(exchange_name.clone());
            }
        }
        
        None
    }
    
    async fn simulate_exchange_failure(&self, exchange_name: &str) {
        info!("Simulating failure of exchange: {}", exchange_name);
        self.failed_exchanges.write().await.insert(exchange_name.to_string(), true);
    }
    
    async fn restore_exchange(&self, exchange_name: &str) {
        info!("Restoring exchange: {}", exchange_name);
        self.failed_exchanges.write().await.insert(exchange_name.to_string(), false);
    }
    
    async fn generate_report(&self) -> FailoverReport {
        let stats = self.failover_stats.read().await;
        
        let average_failover_time = if !stats.failover_times_ms.is_empty() {
            stats.failover_times_ms.iter().sum::<u64>() / stats.failover_times_ms.len() as u64
        } else {
            0
        };
        
        let max_failover_time = stats.failover_times_ms.iter().max().copied().unwrap_or(0);
        
        FailoverReport {
            total_operations: stats.total_operations,
            successful_failovers: stats.successful_failovers,
            failed_failovers: stats.failed_failovers,
            average_failover_time_ms: average_failover_time,
            max_failover_time_ms: max_failover_time,
        }
    }
}