//! Kafka Subscriber for Jackbot Sensor
//!
//! Listens to Kafka topics for order commands from the backend
//! and coordinates with the trading engine for execution

use anyhow::{Context, Result};
use futures::StreamExt;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use jackbot_sensor::api::{OrderResponse, BalanceData, PositionData};
use jackbot_sensor::streaming::StreamingManager;

/// Kafka topics (replacing Kafka channels)
pub mod topics {
    pub const ORDER_COMMANDS: &str = "user.orders";
    pub const STRATEGY_COMMANDS: &str = "strategy.execution";
    pub const PROPHETIC_ORDERS: &str = "user.prophetic.orders";
    pub const JACKPOT_ORDERS: &str = "user.jackpot.orders";
    pub const SYSTEM_CONTROL: &str = "system.control";
}

/// Kafka message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaMessage {
    pub id: String,
    pub timestamp: i64,
    pub message_type: String,
    pub payload: serde_json::Value,
    pub source: String,
    pub correlation_id: Option<String>,
}

/// Order command from backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCommand {
    pub user_id: String,
    pub order_id: String,
    pub exchange: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub quantity: f64,
    pub price: Option<f64>,
    pub stop_price: Option<f64>,
    pub time_in_force: Option<String>,
    pub reduce_only: Option<bool>,
    pub post_only: Option<bool>,
}

/// Strategy command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyCommand {
    pub user_id: String,
    pub strategy_id: String,
    pub action: String, // "start", "stop", "update"
    pub parameters: serde_json::Value,
}

/// Prophetic order (way out of money orders)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropheticOrder {
    pub id: String,
    pub user_id: String,
    pub symbol: String,
    pub side: String,
    pub price: f64,
    pub quantity: f64,
    pub trigger_price: f64,
    pub created_at: i64,
    pub status: String,
    pub exchange: String,
}

/// Jackpot order (high leverage gambling)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JackpotOrder {
    pub id: String,
    pub user_id: String,
    pub symbol: String,
    pub side: String,
    pub chip_size: f64, // Risk amount in USD
    pub leverage: f64, // 100x, 200x etc
    pub auto_exit_multiplier: Option<f64>, // 2x, 3x, 5x etc
    pub time_horizon: Option<i64>, // Seconds until expiry
    pub created_at: i64,
    pub status: String,
    pub exchange: String,
}

/// Kafka subscriber for sensor
pub struct KafkaSubscriber {
    consumer_group: String,
    brokers: String,
    streaming_manager: Arc<RwLock<StreamingManager>>,
    command_tx: mpsc::Sender<KafkaMessage>,
    command_rx: Arc<RwLock<mpsc::Receiver<KafkaMessage>>>,
    active: Arc<RwLock<bool>>,
}

impl KafkaSubscriber {
    /// Create new Kafka subscriber
    pub fn new(consumer_group: String, brokers: String, streaming_manager: Arc<RwLock<StreamingManager>>) -> Self {
        let (command_tx, command_rx) = mpsc::channel(1000);
        
        Self {
            consumer_group,
            brokers,
            streaming_manager,
            command_tx,
            command_rx: Arc::new(RwLock::new(command_rx)),
            active: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Start the subscriber
    pub async fn start(&self) -> Result<()> {
        let mut active = self.active.write().await;
        if *active {
            return Ok(());
        }
        *active = true;
        drop(active);
        
        info!("Starting Kafka subscriber with brokers: {}", self.brokers);
        
        // Start consumer tasks for different topics
        let consumer_group = self.consumer_group.clone();
        let brokers = self.brokers.clone();
        
        // Order commands consumer
        let order_consumer = self.create_consumer(&consumer_group, &brokers, topics::ORDER_COMMANDS).await?;
        let order_handle = self.spawn_consumer_task(order_consumer, "orders");
        
        // Strategy commands consumer
        let strategy_consumer = self.create_consumer(&consumer_group, &brokers, topics::STRATEGY_COMMANDS).await?;
        let strategy_handle = self.spawn_consumer_task(strategy_consumer, "strategies");
        
        // Prophetic orders consumer
        let prophetic_consumer = self.create_consumer(&consumer_group, &brokers, topics::PROPHETIC_ORDERS).await?;
        let prophetic_handle = self.spawn_consumer_task(prophetic_consumer, "prophetic");
        
        // Jackpot orders consumer
        let jackpot_consumer = self.create_consumer(&consumer_group, &brokers, topics::JACKPOT_ORDERS).await?;
        let jackpot_handle = self.spawn_consumer_task(jackpot_consumer, "jackpot");
        
        // Start command processor
        let processor_handle = self.spawn_processor_task();
        
        info!("Kafka subscriber started successfully");
        
        Ok(())
    }
    
    /// Create a Kafka consumer
    async fn create_consumer(&self, group_id: &str, brokers: &str, topic: &str) -> Result<StreamConsumer> {
        use rdkafka::config::ClientConfig;
        
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            .set("auto.offset.reset", "latest")
            .set("session.timeout.ms", "6000")
            .create()
            .context("Failed to create Kafka consumer")?;
        
        consumer
            .subscribe(&[topic])
            .context("Failed to subscribe to topic")?;
        
        Ok(consumer)
    }
    
    /// Spawn consumer task
    fn spawn_consumer_task(&self, consumer: StreamConsumer, consumer_type: &str) -> tokio::task::JoinHandle<()> {
        let command_tx = self.command_tx.clone();
        let active = self.active.clone();
        let consumer_type = consumer_type.to_string();
        
        tokio::spawn(async move {
            info!("Starting {} consumer task", consumer_type);
            
            loop {
                if !*active.read().await {
                    break;
                }
                
                match consumer.recv().await {
                    Ok(message) => {
                        if let Some(payload) = message.payload() {
                            match serde_json::from_slice::<KafkaMessage>(payload) {
                                Ok(msg) => {
                                    debug!("Received {} message: {:?}", consumer_type, msg.message_type);
                                    if let Err(e) = command_tx.send(msg).await {
                                        error!("Failed to forward message: {}", e);
                                    }
                                    
                                    // Commit offset
                                    if let Err(e) = consumer.commit_message(&message, rdkafka::consumer::CommitMode::Async) {
                                        warn!("Failed to commit offset: {}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to parse Kafka message: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Kafka consumer error: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            }
            
            info!("{} consumer task stopped", consumer_type);
        })
    }
    
    /// Spawn processor task
    fn spawn_processor_task(&self) -> tokio::task::JoinHandle<()> {
        let command_rx = self.command_rx.clone();
        let streaming_manager = self.streaming_manager.clone();
        let active = self.active.clone();
        
        tokio::spawn(async move {
            info!("Starting command processor task");
            let mut rx = command_rx.write().await;
            
            while let Some(msg) = rx.recv().await {
                if !*active.read().await {
                    break;
                }
                
                match msg.message_type.as_str() {
                    "order_command" => {
                        if let Ok(cmd) = serde_json::from_value::<OrderCommand>(msg.payload) {
                            Self::process_order_command(cmd, &streaming_manager).await;
                        }
                    }
                    "strategy_command" => {
                        if let Ok(cmd) = serde_json::from_value::<StrategyCommand>(msg.payload) {
                            Self::process_strategy_command(cmd, &streaming_manager).await;
                        }
                    }
                    "prophetic_order" => {
                        if let Ok(order) = serde_json::from_value::<PropheticOrder>(msg.payload) {
                            Self::process_prophetic_order(order, &streaming_manager).await;
                        }
                    }
                    "jackpot_order" => {
                        if let Ok(order) = serde_json::from_value::<JackpotOrder>(msg.payload) {
                            Self::process_jackpot_order(order, &streaming_manager).await;
                        }
                    }
                    _ => {
                        warn!("Unknown message type: {}", msg.message_type);
                    }
                }
            }
            
            info!("Command processor task stopped");
        })
    }
    
    /// Process order command
    async fn process_order_command(cmd: OrderCommand, streaming_manager: &Arc<RwLock<StreamingManager>>) {
        info!("Processing order command: {:?}", cmd);
        
        // Forward to streaming manager for execution
        let manager = streaming_manager.read().await;
        // Implementation would forward to appropriate exchange client
    }
    
    /// Process strategy command
    async fn process_strategy_command(cmd: StrategyCommand, streaming_manager: &Arc<RwLock<StreamingManager>>) {
        info!("Processing strategy command: {:?}", cmd);
        
        match cmd.action.as_str() {
            "start" => {
                // Start strategy execution
            }
            "stop" => {
                // Stop strategy execution
            }
            "update" => {
                // Update strategy parameters
            }
            _ => {
                warn!("Unknown strategy action: {}", cmd.action);
            }
        }
    }
    
    /// Process prophetic order
    async fn process_prophetic_order(order: PropheticOrder, streaming_manager: &Arc<RwLock<StreamingManager>>) {
        info!("Processing prophetic order: {:?}", order);
        
        // Store in prophetic order book for monitoring
        // When market price approaches trigger price, place the order
    }
    
    /// Process jackpot order
    async fn process_jackpot_order(order: JackpotOrder, streaming_manager: &Arc<RwLock<StreamingManager>>) {
        info!("Processing jackpot order: {:?}", order);
        
        // Calculate position size based on chip size and leverage
        let position_size = order.chip_size * order.leverage;
        
        // Place high-leverage order with appropriate risk controls
    }
    
    /// Stop the subscriber
    pub async fn stop(&self) -> Result<()> {
        let mut active = self.active.write().await;
        *active = false;
        
        info!("Kafka subscriber stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_topic_names() {
        assert_eq!(topics::ORDER_COMMANDS, "user.orders");
        assert_eq!(topics::PROPHETIC_ORDERS, "user.prophetic.orders");
        assert_eq!(topics::JACKPOT_ORDERS, "user.jackpot.orders");
    }
    
    #[tokio::test]
    async fn test_message_parsing() {
        let msg = KafkaMessage {
            id: "test-123".to_string(),
            timestamp: 1234567890,
            message_type: "order_command".to_string(),
            payload: serde_json::json!({
                "user_id": "user123",
                "order_id": "order456",
                "exchange": "binance",
                "symbol": "BTCUSDT",
                "side": "buy",
                "order_type": "limit",
                "quantity": 0.01,
                "price": 50000.0
            }),
            source: "backend".to_string(),
            correlation_id: None,
        };
        
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: KafkaMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "test-123");
    }
}