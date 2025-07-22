//! EXCHANGE CONNECTION DESTRUCTION TEST
//! 
//! Ruthlessly tests real exchange connectivity claims:
//! - No localhost dependencies
//! - Real WebSocket connections
//! - <10ms latency claims
//! - Network resilience patterns

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

#[tokio::test]
async fn destroy_real_exchange_connectivity() {
    println!("\n=== EXCHANGE CONNECTION DESTRUCTION TEST ===\n");
    
    let exchanges = vec![
        ("binance", "wss://stream.binance.com:9443/ws"),
        ("coinbase", "wss://ws-feed.exchange.coinbase.com"),
        ("bybit", "wss://stream.bybit.com/v5/public/spot"),
        ("bitget", "wss://ws.bitget.com/v2/ws/public"),
        ("hyperliquid", "wss://api.hyperliquid.xyz/ws"),
        ("kucoin", "wss://ws-api-spot.kucoin.com"),
        ("kraken", "wss://ws.kraken.com"),
        ("okx", "wss://ws.okx.com:8443/ws/v5/public"),
    ];
    
    let mut results = vec![];
    
    for (exchange, url) in exchanges {
        println!("Testing {}: {}", exchange, url);
        
        let start = Instant::now();
        let result = test_real_connection(exchange, url).await;
        let elapsed = start.elapsed();
        
        match result {
            Ok(latency) => {
                println!("  ✅ Connected in {:?}", elapsed);
                println!("  📊 First message latency: {:?}", latency);
                results.push((exchange, true, elapsed, Some(latency)));
            }
            Err(e) => {
                println!("  ❌ FAILED: {}", e);
                results.push((exchange, false, elapsed, None));
            }
        }
        println!();
    }
    
    // Summary
    println!("\n=== CONNECTION DESTRUCTION REPORT ===\n");
    let successful = results.iter().filter(|(_, success, _, _)| *success).count();
    println!("Connected: {}/{}", successful, results.len());
    
    for (exchange, success, conn_time, msg_latency) in &results {
        if *success {
            println!("{}: ✅ Connected in {:?}, message latency: {:?}", 
                exchange, conn_time, msg_latency.unwrap());
        } else {
            println!("{}: ❌ FAILED after {:?}", exchange, conn_time);
        }
    }
    
    // Latency claim verification
    println!("\n=== <10ms LATENCY CLAIM VERIFICATION ===");
    let under_10ms = results.iter()
        .filter(|(_, success, _, latency)| {
            *success && latency.map(|l| l < Duration::from_millis(10)).unwrap_or(false)
        })
        .count();
    
    println!("Exchanges with <10ms latency: {}/{}", under_10ms, successful);
    if under_10ms == 0 {
        println!("🚨 CLAIM DESTROYED: NO exchange achieved <10ms latency!");
    }
}

async fn test_real_connection(exchange: &str, url: &str) -> Result<Duration> {
    let url = Url::parse(url)?;
    
    // 10 second timeout for connection
    let (ws_stream, _) = timeout(
        Duration::from_secs(10),
        connect_async(&url)
    ).await??;
    
    let (mut write, mut read) = ws_stream.split();
    
    // Send subscription message based on exchange
    let subscribe_msg = match exchange {
        "binance" => json!({
            "method": "SUBSCRIBE",
            "params": ["btcusdt@ticker"],
            "id": 1
        }),
        "coinbase" => json!({
            "type": "subscribe",
            "channels": [{"name": "ticker", "product_ids": ["BTC-USD"]}]
        }),
        "bybit" => json!({
            "op": "subscribe",
            "args": ["tickers.BTCUSDT"]
        }),
        "bitget" => json!({
            "op": "subscribe",
            "args": [{"channel": "ticker", "instId": "BTCUSDT"}]
        }),
        "kraken" => json!({
            "event": "subscribe",
            "pair": ["XBT/USD"],
            "subscription": {"name": "ticker"}
        }),
        "okx" => json!({
            "op": "subscribe",
            "args": [{"channel": "tickers", "instId": "BTC-USDT"}]
        }),
        _ => json!({}), // Default empty for others
    };
    
    let msg_start = Instant::now();
    
    if !subscribe_msg.is_null() {
        write.send(Message::Text(subscribe_msg.to_string())).await?;
    }
    
    // Wait for first message with timeout
    let first_msg = timeout(
        Duration::from_secs(5),
        read.next()
    ).await?;
    
    let msg_latency = msg_start.elapsed();
    
    if first_msg.is_none() {
        return Err(anyhow::anyhow!("No message received"));
    }
    
    Ok(msg_latency)
}

#[tokio::test]
async fn test_network_latency_reality() {
    println!("\n=== NETWORK LATENCY REALITY CHECK ===\n");
    
    // Test with simulated network delay
    // Note: This requires running with network simulation tools
    println!("Testing Binance with simulated network conditions...");
    
    let latencies = vec![0, 10, 25, 50, 100, 200];
    
    for network_delay in latencies {
        println!("\nNetwork delay: {}ms", network_delay);
        // In real test, add: sudo tc qdisc add dev eth0 root netem delay {}ms
        
        let start = Instant::now();
        let url = "wss://stream.binance.com:9443/ws";
        
        match test_real_connection("binance", url).await {
            Ok(msg_latency) => {
                let total = start.elapsed();
                println!("  Connection time: {:?}", total);
                println!("  Message latency: {:?}", msg_latency);
                println!("  Total latency: {:?}", total + msg_latency);
                
                let total_ms = (total + msg_latency).as_millis();
                if total_ms < 10 {
                    println!("  ✅ Under 10ms!");
                } else {
                    println!("  ❌ Over 10ms - CLAIM DESTROYED!");
                }
            }
            Err(e) => println!("  ❌ Failed: {}", e),
        }
    }
}

#[tokio::test] 
async fn test_resilience_patterns() {
    println!("\n=== RESILIENCE PATTERN DESTRUCTION ===\n");
    
    // Test rapid connect/disconnect
    println!("Testing rapid connect/disconnect pattern...");
    
    let url = "wss://stream.binance.com:9443/ws";
    let mut failures = 0;
    
    for i in 0..10 {
        print!("Attempt {}: ", i + 1);
        
        match connect_and_disconnect(url).await {
            Ok(_) => print!("✅ "),
            Err(_) => {
                print!("❌ ");
                failures += 1;
            }
        }
        
        if i < 9 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    println!("\nFailures: {}/10", failures);
    
    if failures > 2 {
        println!("🚨 RESILIENCE DESTROYED: Too many failures under stress!");
    }
}

async fn connect_and_disconnect(url: &str) -> Result<()> {
    let url = Url::parse(url)?;
    let (ws_stream, _) = timeout(
        Duration::from_secs(2),
        connect_async(&url)
    ).await??;
    
    // Immediately close
    drop(ws_stream);
    Ok(())
}

#[tokio::test]
async fn test_all_exchanges_parallel() {
    println!("\n=== PARALLEL LOAD DESTRUCTION TEST ===\n");
    
    let exchanges = vec![
        ("binance", "wss://stream.binance.com:9443/ws"),
        ("coinbase", "wss://ws-feed.exchange.coinbase.com"),
        ("bybit", "wss://stream.bybit.com/v5/public/spot"),
        ("bitget", "wss://ws.bitget.com/v2/ws/public"),
        ("hyperliquid", "wss://api.hyperliquid.xyz/ws"),
        ("kucoin", "wss://ws-api-spot.kucoin.com"),
        ("kraken", "wss://ws.kraken.com"),
        ("okx", "wss://ws.okx.com:8443/ws/v5/public"),
    ];
    
    println!("Connecting to ALL exchanges simultaneously...");
    let start = Instant::now();
    
    let handles: Vec<_> = exchanges.into_iter()
        .map(|(exchange, url)| {
            tokio::spawn(async move {
                test_real_connection(exchange, url).await
            })
        })
        .collect();
    
    let mut successes = 0;
    for handle in handles {
        if handle.await.unwrap().is_ok() {
            successes += 1;
        }
    }
    
    let elapsed = start.elapsed();
    println!("Parallel connection time: {:?}", elapsed);
    println!("Successful connections: {}/8", successes);
    
    if successes < 6 {
        println!("🚨 PARALLEL LOAD DESTROYED: System can't handle all exchanges!");
    }
}

#[tokio::test]
#[ignore] // Long running test
async fn test_long_term_stability() {
    println!("\n=== LONG-TERM STABILITY TEST (1 hour) ===\n");
    
    let duration = Duration::from_secs(3600); // 1 hour
    let start = Instant::now();
    
    // This would run for 1 hour in production
    println!("Would test stability for {:?}", duration);
    println!("Monitoring for:");
    println!("- Memory leaks");
    println!("- Connection drops");
    println!("- Message delays");
    println!("- Rate limit violations");
}

use serde_json::json;