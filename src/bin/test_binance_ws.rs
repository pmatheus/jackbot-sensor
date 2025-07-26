//! Test Binance WebSocket connectivity
//!
//! Simple test program to verify Binance testnet WebSocket connection
//! and order book streaming for BTC/USDT

use anyhow::Result;
use jackbot_sensor::{
    binance_websocket::BinanceWebSocketClient,
    kafka_producer::{KafkaProducer, ProducerConfig},
    streaming::StreamingManager,
};
use std::sync::Arc;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::filter::EnvFilter::from_default_env()
                .add_directive("jackbot_sensor=debug".parse()?)
                .add_directive("test_binance_ws=info".parse()?),
        )
        .init();
    
    info!("🚀 Starting Binance WebSocket test");
    
    // Create Kafka producer
    let kafka_config = ProducerConfig {
        brokers: std::env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string()),
        ..Default::default()
    };
    
    let kafka_producer = match KafkaProducer::new(kafka_config).await {
        Ok(producer) => {
            info!("✅ Kafka producer connected");
            Some(Arc::new(producer))
        }
        Err(e) => {
            error!("❌ Failed to create Kafka producer: {}", e);
            info!("⚠️  Continuing without Kafka - data will only be logged");
            None
        }
    };
    
    // Create streaming manager with Kafka producer
    let streaming_manager = Arc::new(StreamingManager::new_with_kafka(kafka_producer.clone()));
    
    // Create Binance WebSocket client for TESTNET
    let binance_client = BinanceWebSocketClient::new(
        streaming_manager.clone(),
        kafka_producer,
        true, // Use testnet
    )?;
    
    info!("📊 Subscribing to BTC/USDT order book stream");
    
    // Subscribe to order book stream
    binance_client.subscribe_orderbook("BTC/USDT").await?;
    
    info!("📈 Subscribing to BTC/USDT ticker stream");
    
    // Subscribe to ticker stream
    binance_client.subscribe_ticker("BTC/USDT").await?;
    
    info!("💹 Subscribing to BTC/USDT trades stream");
    
    // Subscribe to trades stream
    binance_client.subscribe_trades("BTC/USDT").await?;
    
    info!("✅ All streams subscribed - receiving real-time data");
    info!("🔍 Monitor Kafka with: kcat -C -b localhost:9092 -t 'l2-data.binance.btcusdt' -f '%T | %s\\n'");
    
    // Print stats periodically
    let stats_client = binance_client.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let stats = stats_client.get_stats().await;
            info!("📊 Connection stats: {}", serde_json::to_string_pretty(&stats).unwrap());
        }
    });
    
    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    
    info!("🛑 Shutting down...");
    binance_client.shutdown().await?;
    
    info!("✅ Test completed");
    Ok(())
}