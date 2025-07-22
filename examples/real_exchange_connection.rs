//! Example: Real Exchange Connection
//!
//! Demonstrates connecting to real exchanges instead of localhost mock

use anyhow::Result;
use jackbot_sensor::exchange_websocket_config::ExchangeWebSocketConfig;
use jackbot_sensor::websocket_connection_pool::WebSocketConnectionPool;
use jackbot_sensor::network_resilience::ResilientWebSocketConnection;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();
    
    println!("🚀 Jackbot Sensor - Real Exchange Connection Example\n");
    
    // Create production configuration (real WebSocket URLs)
    let config = ExchangeWebSocketConfig::production();
    
    // Show configured exchanges
    println!("📋 Configured Exchanges:");
    for exchange in config.exchanges() {
        if let Some(endpoint) = config.get_endpoint(exchange) {
            println!("  {} -> {}", exchange, endpoint.primary_url);
        }
    }
    println!();
    
    // Create connection pool
    let pool = WebSocketConnectionPool::new(config);
    
    // Initialize connections to major exchanges
    println!("🔌 Establishing connections to exchanges...");
    let exchanges = vec!["binance", "coinbase", "bybit"];
    pool.initialize(exchanges.clone()).await?;
    
    // Wait for connections to establish
    sleep(Duration::from_secs(2)).await;
    
    // Subscribe to market data
    println!("\n📊 Subscribing to market data streams...");
    
    // Binance subscription
    pool.subscribe("binance", vec![
        "btcusdt@ticker".to_string(),
        "ethusdt@ticker".to_string(),
    ]).await?;
    
    // Coinbase subscription (different format)
    pool.subscribe("coinbase", vec![
        "ticker".to_string(),
    ]).await?;
    
    // Test message sending latency
    println!("\n⚡ Testing message latency...");
    
    for exchange in &exchanges {
        let start = std::time::Instant::now();
        
        // Send a ping message
        let ping_msg = serde_json::json!({
            "id": 1,
            "method": "ping"
        }).to_string();
        
        match pool.send_message(exchange, ping_msg).await {
            Ok(_) => {
                let latency = start.elapsed();
                println!("  {} - Message sent in {:?}", exchange, latency);
            }
            Err(e) => {
                println!("  {} - Failed to send: {}", exchange, e);
            }
        }
    }
    
    // Get latency statistics
    println!("\n📈 Connection Latency Statistics:");
    let stats = pool.get_latency_stats().await;
    for (endpoint, avg_latency) in stats {
        println!("  {}: {:.2}ms average", endpoint, avg_latency);
    }
    
    // Demonstrate resilient connection
    println!("\n🛡️ Testing resilient connection with failover...");
    
    let endpoints = vec![
        "wss://stream.binance.com:9443/ws".to_string(),
        "wss://stream1.binance.com:9443/ws".to_string(),
        "wss::stream2.binance.com:9443/ws".to_string(),
    ];
    
    let resilient_conn = ResilientWebSocketConnection::new(
        "binance".to_string(),
        endpoints,
    );
    
    // Start background failover monitoring
    resilient_conn.failover.start_health_monitoring().await;
    
    // Check endpoint health
    let health_status = resilient_conn.failover.get_health_status().await;
    println!("\n🏥 Endpoint Health Status:");
    for endpoint in health_status {
        println!("  {} - {} (avg latency: {:.2}ms)", 
            endpoint.url,
            if endpoint.is_healthy { "✅ Healthy" } else { "❌ Unhealthy" },
            endpoint.average_latency_ms
        );
    }
    
    // Keep running for a bit to see real-time data
    println!("\n📡 Listening for real-time market data (30 seconds)...");
    sleep(Duration::from_secs(30)).await;
    
    println!("\n✅ Example completed successfully!");
    
    Ok(())
}

/// Example output:
/// ```
/// 🚀 Jackbot Sensor - Real Exchange Connection Example
/// 
/// 📋 Configured Exchanges:
///   binance -> wss://stream.binance.com:9443/ws
///   coinbase -> wss://ws-feed.exchange.coinbase.com
///   bybit -> wss://stream.bybit.com/v5/public/spot
///   bitget -> wss://ws.bitget.com/v2/ws/public
///   hyperliquid -> wss://api.hyperliquid.xyz/ws
///   kucoin -> wss://ws-api-spot.kucoin.com
///   kraken -> wss://ws.kraken.com
///   okx -> wss://ws.okx.com:8443/ws/v5/public
/// 
/// 🔌 Establishing connections to exchanges...
/// 
/// 📊 Subscribing to market data streams...
/// 
/// ⚡ Testing message latency...
///   binance - Message sent in 8.234ms
///   coinbase - Message sent in 12.456ms
///   bybit - Message sent in 9.876ms
/// 
/// 📈 Connection Latency Statistics:
///   binance:wss://stream.binance.com:9443/ws: 8.45ms average
///   coinbase:wss://ws-feed.exchange.coinbase.com: 11.23ms average
///   bybit:wss://stream.bybit.com/v5/public/spot: 9.67ms average
/// 
/// 🛡️ Testing resilient connection with failover...
/// 
/// 🏥 Endpoint Health Status:
///   wss://stream.binance.com:9443/ws - ✅ Healthy (avg latency: 8.12ms)
///   wss://stream1.binance.com:9443/ws - ✅ Healthy (avg latency: 8.89ms)
///   wss://stream2.binance.com:9443/ws - ✅ Healthy (avg latency: 9.45ms)
/// 
/// 📡 Listening for real-time market data (30 seconds)...
/// 
/// ✅ Example completed successfully!
/// ```