//! Simple Binance WebSocket test without full sensor dependencies
//!
//! This is a minimal test to verify Binance WebSocket connectivity

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();
    
    info!("🚀 Starting simple Binance WebSocket test");
    
    // Connect to Binance testnet
    let symbol = "btcusdt";
    let stream = format!("{}@depth@100ms", symbol); // Order book stream
    let url = format!("wss://testnet.binance.vision/ws/{}", stream);
    
    info!("🔌 Connecting to: {}", url);
    
    let (ws_stream, response) = connect_async(Url::parse(&url)?).await?;
    info!("✅ Connected! Response: {:?}", response);
    
    let (mut tx, mut rx) = ws_stream.split();
    
    // Spawn ping task
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if tx.send(Message::Ping(vec![])).await.is_err() {
                break;
            }
        }
    });
    
    // Process messages
    let mut message_count = 0;
    info!("📊 Listening for order book updates...");
    
    while let Some(msg) = rx.next().await {
        match msg? {
            Message::Text(text) => {
                message_count += 1;
                
                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    // Extract order book data
                    let bids = data["b"].as_array().map(|a| a.len()).unwrap_or(0);
                    let asks = data["a"].as_array().map(|a| a.len()).unwrap_or(0);
                    let event_time = data["E"].as_i64().unwrap_or(0);
                    
                    // Log every 10th message
                    if message_count % 10 == 0 {
                        info!(
                            "📖 Order book update #{}: {} bids, {} asks (time: {})",
                            message_count, bids, asks, event_time
                        );
                        
                        // Show best bid/ask
                        if let (Some(bid_array), Some(ask_array)) = (data["b"].as_array(), data["a"].as_array()) {
                            if let (Some(best_bid), Some(best_ask)) = (bid_array.first(), ask_array.first()) {
                                if let (Some(bid_price), Some(ask_price)) = (
                                    best_bid[0].as_str(),
                                    best_ask[0].as_str()
                                ) {
                                    info!("💰 Best bid: {} | Best ask: {}", bid_price, ask_price);
                                }
                            }
                        }
                    }
                }
            }
            Message::Close(_) => {
                warn!("WebSocket closed");
                break;
            }
            Message::Pong(_) => {
                info!("🏓 Received pong");
            }
            _ => {}
        }
    }
    
    info!("✅ Test completed. Received {} messages", message_count);
    Ok(())
}