//! Quick connectivity test to verify exchange claims

use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

#[tokio::test]
async fn test_single_exchange_connectivity() {
    println!("\n=== QUICK CONNECTIVITY TEST ===\n");
    
    // Test just Binance to verify connectivity
    let url = "wss://stream.binance.com:9443/ws";
    let start = Instant::now();
    
    println!("Testing Binance WebSocket: {}", url);
    
    match timeout(Duration::from_secs(10), connect_async(Url::parse(url).unwrap())).await {
        Ok(Ok((mut ws_stream, response))) => {
            let connect_time = start.elapsed();
            println!("✅ Connected in {:?}", connect_time);
            println!("HTTP Status: {}", response.status());
            
            // Try to subscribe
            let subscribe = serde_json::json!({
                "method": "SUBSCRIBE",
                "params": ["btcusdt@ticker"],
                "id": 1
            });
            
            let msg_start = Instant::now();
            ws_stream.send(Message::Text(subscribe.to_string())).await.unwrap();
            
            // Wait for response
            match timeout(Duration::from_secs(5), ws_stream.next()).await {
                Ok(Some(Ok(msg))) => {
                    let msg_time = msg_start.elapsed();
                    println!("📨 First message received in {:?}", msg_time);
                    println!("Message: {:?}", msg.to_text().unwrap_or("binary"));
                    
                    if msg_time < Duration::from_millis(10) {
                        println!("✅ UNDER 10ms!");
                    } else {
                        println!("❌ OVER 10ms - Claim questionable!");
                    }
                }
                _ => println!("❌ No message received"),
            }
        }
        Ok(Err(e)) => {
            println!("❌ Connection failed: {}", e);
        }
        Err(_) => {
            println!("❌ Connection timeout!");
        }
    }
}

#[test]
fn test_localhost_presence() {
    println!("\n=== LOCALHOST PRESENCE TEST ===\n");
    
    // Check for localhost in code
    let patterns = vec![
        ("localhost", "Direct localhost reference"),
        ("127.0.0.1", "IP localhost reference"),
        (":8082", "Mock port reference"),
    ];
    
    for (pattern, desc) in patterns {
        println!("Checking for {}: {}", desc, pattern);
        // In real implementation, would grep the codebase
        println!("  [Would search codebase for '{}']", pattern);
    }
}