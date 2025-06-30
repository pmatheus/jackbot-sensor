//! MVP Sensor - Kafka-based Order Processing

use anyhow::Result;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::Message;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{error, info, warn};

use crate::kafka_subscriber::{OrderCommand, KafkaMessage};

#[derive(Debug, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub user: String,
    pub exchange: String,
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub price: Option<f64>,
    pub order_type: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderStatus {
    pub order_id: String,
    pub status: String,
    pub filled_qty: f64,
    pub avg_price: f64,
    pub timestamp: i64,
}

pub async fn run_mvp(kafka_brokers: &str) -> Result<()> {
    info!("🚀 MVP Sensor starting with Kafka integration...");
    info!("📡 Connecting to Kafka brokers: {}", kafka_brokers);
    
    // Create Kafka consumer for orders
    use rdkafka::config::ClientConfig;
    
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", kafka_brokers)
        .set("group.id", "mvp-sensor")
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "latest")
        .create()?;
    
    // Subscribe to order topics
    consumer.subscribe(&["jackbot.user.orders.*"])?;
    info!("✅ Subscribed to order topics");
    
    // Process orders from Kafka
    loop {
        match consumer.recv().await {
            Ok(message) => {
                if let Some(payload) = message.payload() {
                    match serde_json::from_slice::<KafkaMessage>(payload) {
                        Ok(kafka_msg) => {
                            if kafka_msg.message_type == "order_command" {
                                if let Ok(order_cmd) = serde_json::from_value::<OrderCommand>(kafka_msg.payload) {
                                    process_order(&order_cmd).await;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse Kafka message: {}", e);
                        }
                    }
                    
                    // Commit offset
                    if let Err(e) = consumer.commit_message(&message, rdkafka::consumer::CommitMode::Async) {
                        warn!("Failed to commit offset: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Kafka consumer error: {}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn process_order(order: &OrderCommand) {
    info!("📦 Processing order: {} | {} {} {} @ {:?}",
        order.order_id,
        order.side.to_uppercase(),
        order.quantity,
        order.symbol,
        order.price
    );
    
    // Simulate order processing
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Log result
    info!("✅ Order {} processed successfully", order.order_id);
    
    // In production, this would:
    // 1. Route to appropriate exchange
    // 2. Place actual order
    // 3. Update status in DynamoDB
    // 4. Publish updates back to Kafka
}