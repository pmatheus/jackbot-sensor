//! Order processor - Handles order execution through Redis pub/sub

use anyhow::Result;
use jackbot_data::redis_store::RedisClientStore;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{error, info, warn};
use futures_util::StreamExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct OrderProcessor {
    redis_store: Arc<RedisClientStore>,
    running: Arc<AtomicBool>,
}

impl OrderProcessor {
    pub async fn new(redis_store: Arc<RedisClientStore>) -> Result<Self> {
        Ok(Self {
            redis_store,
            running: Arc::new(AtomicBool::new(true)),
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("🔄 Order processor starting...");
        
        // Get Redis connection for pub/sub
        // For now, use the default Redis URL
        let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let client = Client::open(redis_url)?;
        
        // Get connection for pub/sub
        let pubsub_con = client.get_async_connection().await?;
        let mut pubsub = pubsub_con.into_pubsub();
        
        // Subscribe to order channel
        pubsub.subscribe("jb:orders:new").await?;
        info!("📡 Subscribed to order channel: jb:orders:new");
        
        // Get connection for publishing status updates
        let mut con = client.get_multiplexed_async_connection().await?;
        
        // Process orders
        while self.running.load(Ordering::Relaxed) {
            match pubsub.on_message().next().await {
                Some(msg) => {
                    match msg.get_payload::<String>() {
                        Ok(payload) => {
                            match serde_json::from_str::<Order>(&payload) {
                                Ok(order) => {
                                    info!(
                                        "📋 Received order: {} {} {} {} @ {:?}", 
                                        order.id, order.side, order.qty, order.symbol, order.price
                                    );
                                    
                                    // Process the order
                                    let status = match self.process_order(&order).await {
                                        Ok(status) => {
                                            info!("✅ Order {} processed successfully", order.id);
                                            status
                                        }
                                        Err(e) => {
                                            error!("❌ Failed to process order {}: {}", order.id, e);
                                            OrderStatus {
                                                order_id: order.id.clone(),
                                                status: "FAILED".to_string(),
                                                filled_qty: 0.0,
                                                avg_price: 0.0,
                                                timestamp: chrono::Utc::now().timestamp(),
                                                error: Some(e.to_string()),
                                            }
                                        }
                                    };
                                    
                                    // Publish status update
                                    let status_json = serde_json::to_string(&status)?;
                                    let _: () = con.publish(
                                        format!("jb:orders:status:{}", order.user),
                                        status_json
                                    ).await?;
                                }
                                Err(e) => {
                                    error!("Failed to parse order: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to get message payload: {}", e);
                        }
                    }
                }
                None => {
                    if self.running.load(Ordering::Relaxed) {
                        warn!("No message received, connection may be closed");
                    }
                    break;
                }
            }
        }
        
        info!("🛑 Order processor stopped");
        Ok(())
    }

    async fn process_order(&self, order: &Order) -> Result<OrderStatus> {
        // TODO: Implement actual order execution
        // For now, this is a placeholder that simulates successful execution
        
        info!("🔄 Processing order {} on exchange {}", order.id, order.exchange);
        
        // In production, this would:
        // 1. Validate the order
        // 2. Check user balance/margin requirements
        // 3. Send order to the appropriate exchange
        // 4. Handle the exchange response
        // 5. Update order status in database
        
        // Simulate processing delay
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Return simulated success
        Ok(OrderStatus {
            order_id: order.id.clone(),
            status: "FILLED".to_string(),
            filled_qty: order.qty,
            avg_price: order.price.unwrap_or(0.0),
            timestamp: chrono::Utc::now().timestamp(),
            error: None,
        })
    }

    pub async fn stop(&self) {
        info!("🛑 Stopping order processor...");
        self.running.store(false, Ordering::Relaxed);
    }
}