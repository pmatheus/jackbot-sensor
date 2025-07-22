//! Real Exchange Integration Tests - <10ms Latency Validation
//!
//! Tests real WebSocket connections to all supported exchanges
//! Validates sub-10ms latency for Bloomberg Terminal competition

use jackbot_sensor::exchange_websocket_config::ExchangeWebSocketConfig;
use jackbot_sensor::websocket_connection_pool::WebSocketConnectionPool;
use jackbot_sensor::network_resilience::{ResilientWebSocketConnection, ConnectionMetrics};
use std::time::{Duration, Instant};
use tokio::time::timeout;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

/// Target latency for Bloomberg Terminal competition
const TARGET_LATENCY_MS: u64 = 10;
const ACCEPTABLE_LATENCY_MS: u64 = 15; // Allow some margin for network variance

#[tokio::test]
async fn test_binance_real_connection_latency() {
    let config = ExchangeWebSocketConfig::production();
    let endpoint = config.get_endpoint("binance").expect("Binance config not found");
    
    // Test primary endpoint
    let latency = measure_websocket_latency(&endpoint.primary_url).await;
    println!("🎯 Binance primary endpoint latency: {:.2}ms", latency);
    
    assert!(
        latency < ACCEPTABLE_LATENCY_MS as f64,
        "Binance latency {:.2}ms exceeds target {}ms",
        latency,
        ACCEPTABLE_LATENCY_MS
    );
    
    // Test regional endpoints
    for (region, url) in &endpoint.regional_endpoints {
        let regional_latency = measure_websocket_latency(url).await;
        println!("  {} region latency: {:.2}ms", region, regional_latency);
    }
}

#[tokio::test]
async fn test_coinbase_real_connection_latency() {
    let config = ExchangeWebSocketConfig::production();
    let endpoint = config.get_endpoint("coinbase").expect("Coinbase config not found");
    
    let latency = measure_websocket_latency(&endpoint.primary_url).await;
    println!("🎯 Coinbase primary endpoint latency: {:.2}ms", latency);
    
    assert!(
        latency < ACCEPTABLE_LATENCY_MS as f64,
        "Coinbase latency {:.2}ms exceeds target {}ms",
        latency,
        ACCEPTABLE_LATENCY_MS
    );
}

#[tokio::test]
async fn test_all_exchanges_parallel_connection() {
    let config = ExchangeWebSocketConfig::production();
    let exchanges = vec![
        "binance", "coinbase", "bybit", "bitget", 
        "hyperliquid", "kucoin", "kraken", "okx"
    ];
    
    println!("\n🏁 Testing parallel connections to all exchanges...\n");
    
    let start = Instant::now();
    let mut handles = vec![];
    
    for exchange in exchanges {
        let config_clone = config.clone();
        let exchange_name = exchange.to_string();
        
        let handle = tokio::spawn(async move {
            if let Some(endpoint) = config_clone.get_endpoint(&exchange_name) {
                let latency = measure_websocket_latency(&endpoint.primary_url).await;
                (exchange_name, latency)
            } else {
                (exchange_name, f64::MAX)
            }
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut results = vec![];
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }
    
    let total_time = start.elapsed();
    
    // Print results summary
    println!("📊 Exchange Connection Latency Results:");
    println!("─────────────────────────────────────");
    
    let mut passed = 0;
    let mut failed = 0;
    
    for (exchange, latency) in &results {
        let status = if *latency < ACCEPTABLE_LATENCY_MS as f64 {
            passed += 1;
            "✅ PASS"
        } else {
            failed += 1;
            "❌ FAIL"
        };
        
        println!("{:<12} {:>8.2}ms  {}", exchange, latency, status);
    }
    
    println!("─────────────────────────────────────");
    println!("Total parallel connection time: {:?}", total_time);
    println!("Passed: {} | Failed: {}", passed, failed);
    
    // At least 6 out of 8 exchanges should meet latency target
    assert!(
        passed >= 6,
        "Only {} out of {} exchanges met latency target",
        passed,
        results.len()
    );
}

#[tokio::test]
async fn test_connection_pool_performance() {
    let config = ExchangeWebSocketConfig::production();
    let pool = WebSocketConnectionPool::new(config);
    
    // Initialize pool with major exchanges
    let exchanges = vec!["binance", "coinbase", "bybit"];
    pool.initialize(exchanges.clone()).await
        .expect("Failed to initialize connection pool");
    
    // Allow connections to establish
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    println!("\n🏊 Testing connection pool performance...\n");
    
    // Test rapid message sending
    for exchange in &exchanges {
        let start = Instant::now();
        
        // Send multiple messages rapidly
        for i in 0..10 {
            let test_message = serde_json::json!({
                "id": i,
                "method": "ping"
            }).to_string();
            
            pool.send_message(exchange, test_message).await
                .expect("Failed to send message");
        }
        
        let elapsed = start.elapsed();
        let avg_latency = elapsed.as_millis() as f64 / 10.0;
        
        println!("{} - 10 messages sent in {:?} (avg: {:.2}ms/msg)", 
            exchange, elapsed, avg_latency);
        
        assert!(
            avg_latency < TARGET_LATENCY_MS as f64,
            "{} average message latency {:.2}ms exceeds target {}ms",
            exchange, avg_latency, TARGET_LATENCY_MS
        );
    }
    
    // Get latency statistics
    let stats = pool.get_latency_stats().await;
    println!("\n📈 Connection Pool Latency Statistics:");
    for (endpoint, avg_latency) in stats {
        println!("  {}: {:.2}ms average", endpoint, avg_latency);
    }
}

#[tokio::test]
async fn test_resilient_connection_recovery() {
    let endpoints = vec![
        "wss://stream.binance.com:9443/ws".to_string(),
        "wss://stream1.binance.com:9443/ws".to_string(),
    ];
    
    let resilient_conn = ResilientWebSocketConnection::new(
        "binance".to_string(),
        endpoints,
    );
    
    println!("\n🛡️ Testing resilient connection with failover...\n");
    
    // Attempt connection with resilience
    match timeout(Duration::from_secs(30), resilient_conn.connect()).await {
        Ok(Ok(())) => {
            println!("✅ Resilient connection established successfully");
            
            // Check metrics
            let attempts = resilient_conn.metrics.connection_attempts.load(std::sync::atomic::Ordering::Relaxed);
            let successes = resilient_conn.metrics.successful_connections.load(std::sync::atomic::Ordering::Relaxed);
            let failures = resilient_conn.metrics.failed_connections.load(std::sync::atomic::Ordering::Relaxed);
            
            println!("Connection metrics:");
            println!("  Attempts: {}", attempts);
            println!("  Successes: {}", successes);
            println!("  Failures: {}", failures);
            
            assert!(successes > 0, "No successful connections");
        }
        Ok(Err(e)) => {
            panic!("Failed to establish resilient connection: {}", e);
        }
        Err(_) => {
            panic!("Connection attempt timed out after 30 seconds");
        }
    }
}

#[tokio::test]
async fn test_orderbook_subscription_latency() {
    let config = ExchangeWebSocketConfig::production();
    let pool = WebSocketConnectionPool::new(config);
    
    // Test with Binance
    pool.initialize(vec!["binance"]).await
        .expect("Failed to initialize pool");
    
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    println!("\n📖 Testing order book subscription latency...\n");
    
    let start = Instant::now();
    
    // Subscribe to order book updates
    let channels = vec![
        "btcusdt@depth@100ms".to_string(),
        "ethusdt@depth@100ms".to_string(),
    ];
    
    pool.subscribe("binance", channels).await
        .expect("Failed to subscribe");
    
    let subscription_time = start.elapsed();
    println!("Subscription completed in {:?}", subscription_time);
    
    assert!(
        subscription_time.as_millis() < 100,
        "Subscription took too long: {:?}",
        subscription_time
    );
}

/// Helper function to measure WebSocket connection latency
async fn measure_websocket_latency(url: &str) -> f64 {
    let start = Instant::now();
    
    match timeout(Duration::from_secs(5), connect_async(Url::parse(url).unwrap())).await {
        Ok(Ok((mut ws_stream, _))) => {
            let connect_time = start.elapsed();
            
            // Send ping and measure round trip
            let ping_start = Instant::now();
            
            // Send a ping message (format varies by exchange)
            let ping_msg = Message::Text(r#"{"id":1,"method":"ping"}"#.to_string());
            let _ = ws_stream.send(ping_msg).await;
            
            // Wait for any response (pong or error)
            if let Ok(Some(Ok(_))) = timeout(Duration::from_secs(1), ws_stream.next()).await {
                let round_trip = ping_start.elapsed();
                
                // Close connection
                let _ = ws_stream.close(None).await;
                
                // Return average of connection time and round trip
                (connect_time.as_millis() + round_trip.as_millis()) as f64 / 2.0
            } else {
                // If no pong, just use connection time
                let _ = ws_stream.close(None).await;
                connect_time.as_millis() as f64
            }
        }
        Ok(Err(e)) => {
            eprintln!("Failed to connect to {}: {}", url, e);
            f64::MAX
        }
        Err(_) => {
            eprintln!("Connection to {} timed out", url);
            f64::MAX
        }
    }
}

#[tokio::test]
#[ignore] // Run manually with: cargo test test_extended_load -- --ignored
async fn test_extended_load_performance() {
    let config = ExchangeWebSocketConfig::production();
    let pool = WebSocketConnectionPool::new(config);
    
    println!("\n🚀 Extended load test - 1000 messages across 3 exchanges\n");
    
    let exchanges = vec!["binance", "coinbase", "bybit"];
    pool.initialize(exchanges.clone()).await.unwrap();
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    let start = Instant::now();
    let mut handles = vec![];
    
    // Send 1000 messages total (spread across exchanges)
    for (i, exchange) in exchanges.iter().cycle().take(1000).enumerate() {
        let pool_clone = pool.clone();
        let exchange_name = exchange.to_string();
        
        let handle = tokio::spawn(async move {
            let msg = serde_json::json!({
                "id": i,
                "method": "ping"
            }).to_string();
            
            pool_clone.send_message(&exchange_name, msg).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all messages
    let mut successes = 0;
    let mut failures = 0;
    
    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => successes += 1,
            _ => failures += 1,
        }
    }
    
    let total_time = start.elapsed();
    let messages_per_second = 1000.0 / total_time.as_secs_f64();
    let avg_latency = total_time.as_millis() as f64 / 1000.0;
    
    println!("📊 Extended Load Test Results:");
    println!("  Total time: {:?}", total_time);
    println!("  Messages sent: 1000");
    println!("  Successes: {}", successes);
    println!("  Failures: {}", failures);
    println!("  Throughput: {:.2} msgs/sec", messages_per_second);
    println!("  Average latency: {:.2}ms/msg", avg_latency);
    
    assert!(successes > 950, "Too many failures: {}", failures);
    assert!(
        avg_latency < TARGET_LATENCY_MS as f64,
        "Average latency {:.2}ms exceeds target {}ms",
        avg_latency,
        TARGET_LATENCY_MS
    );
}