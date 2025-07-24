//! Order processor - Handles order execution through Kafka backbone

use anyhow::Result;
use futures_util::StreamExt;
use jackbot_data::kafka_store::KafkaClientStore;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::consumer::{Consumer, ConsumerContext, Rebalance};
// use rdkafka::error::KafkaResult; // Unused
use rdkafka::message::{Headers, Message};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::ClientContext;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

// Import connector system for real exchange integration
use crate::connector::{ConnectorManager, OrderRequest, OrderResponse, ConnectionStatus};
use crate::streaming::StreamingManager;
use jackbot_instrument::exchange::ExchangeId;

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

// Custom context for Kafka consumer
#[derive(Clone)]
pub struct OrderProcessorContext;

impl ClientContext for OrderProcessorContext {}

impl ConsumerContext for OrderProcessorContext {
    fn pre_rebalance(&self, rebalance: &Rebalance) {
        match rebalance {
            Rebalance::Assign(tpl) => {
                info!("🔄 Kafka consumer: Partitions assigned: {:?}", tpl);
            }
            Rebalance::Revoke(_) => {
                info!("🔄 Kafka consumer: Partitions revoked");
            }
            Rebalance::Error(err) => {
                error!("❌ Kafka consumer: Rebalance error: {:?}", err);
            }
        }
    }

    fn post_rebalance(&self, rebalance: &Rebalance) {
        match rebalance {
            Rebalance::Assign(tpl) => {
                info!("✅ Kafka consumer: Rebalance completed, assigned: {:?}", tpl);
            }
            Rebalance::Revoke(_) => {
                info!("✅ Kafka consumer: Rebalance completed, revoked");
            }
            Rebalance::Error(err) => {
                error!("❌ Kafka consumer: Post-rebalance error: {:?}", err);
            }
        }
    }
}

#[derive(Clone)]
pub struct OrderProcessor {
    kafka_store: Arc<KafkaClientStore>,
    running: Arc<AtomicBool>,
    consumer: Arc<StreamConsumer<OrderProcessorContext>>,
    producer: Arc<FutureProducer>,
    brokers: String,
    // Production exchange integration
    connector_manager: Arc<ConnectorManager>,
    streaming_manager: Arc<StreamingManager>,
}

impl OrderProcessor {
    pub async fn new(
        kafka_store: Arc<KafkaClientStore>, 
        connector_manager: Option<Arc<ConnectorManager>>
    ) -> Result<Self> {
        let brokers = std::env::var("KAFKA_BROKERS")
            .unwrap_or_else(|_| "localhost:9092,localhost:9093,localhost:9094".to_string());
        
        // Create Kafka consumer
        let consumer: StreamConsumer<OrderProcessorContext> = ClientConfig::new()
            .set("group.id", "jackbot-order-processor")
            .set("bootstrap.servers", &brokers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "latest")
            .set("enable.auto.offset.store", "false")
            .create_with_context(OrderProcessorContext)?;

        // Create Kafka producer
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.messages", "10000")
            .set("queue.buffering.max.ms", "100")
            .set("batch.size", "16384")
            .create()?;

        // Initialize streaming manager for real-time data
        let streaming_manager = Arc::new(StreamingManager::new());
        
        // Initialize connector manager if not provided
        let connector_manager = connector_manager.unwrap_or_else(|| {
            Arc::new(ConnectorManager::new(
                streaming_manager.clone(),
                Duration::from_secs(30), // health check interval
            ))
        });

        info!("🔗 Production OrderProcessor initialized with brokers: {}", brokers);
        info!("🏭 Exchange connectors: Binance, Coinbase, Bybit, Bitget, Hyperliquid, Kucoin, Kraken, OKX");

        Ok(Self {
            kafka_store,
            running: Arc::new(AtomicBool::new(true)),
            consumer: Arc::new(consumer),
            producer: Arc::new(producer),
            brokers,
            connector_manager,
            streaming_manager,
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("🔄 Order processor starting with Kafka brokers: {}", self.brokers);

        // Subscribe to the orders topic
        self.consumer
            .subscribe(&["jb:orders:new"])
            .map_err(|e| anyhow::anyhow!("Failed to subscribe to Kafka topic: {}", e))?;
        
        info!("📡 Subscribed to Kafka topic: jb:orders:new");

        // Main processing loop
        while self.running.load(Ordering::Relaxed) {
            match self.consumer.recv().await {
                Ok(message) => {
                    if let Some(payload) = message.payload() {
                        match std::str::from_utf8(payload) {
                            Ok(payload_str) => {
                                match serde_json::from_str::<Order>(payload_str) {
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
                                                error!(
                                                    "❌ Failed to process order {}: {}",
                                                    order.id, e
                                                );
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

                                        // Publish status update to Kafka
                                        let status_topic = format!("jb:orders:status:{}", order.user);
                                        let status_json = serde_json::to_string(&status)?;
                                        
                                        let future_record = FutureRecord::to(&status_topic)
                                            .payload(&status_json)
                                            .key(&order.id);

                                        match self.producer.send(future_record, Duration::from_secs(5)).await {
                                            Ok(_) => {
                                                info!("📤 Published order status for {}", order.id);
                                            }
                                            Err((e, _)) => {
                                                error!("❌ Failed to publish order status: {}", e);
                                            }
                                        }

                                        // Commit the message
                                        if let Err(e) = self.consumer.commit_message(&message, rdkafka::consumer::CommitMode::Async) {
                                            error!("❌ Failed to commit Kafka message: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        error!("❌ Failed to parse order JSON: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("❌ Failed to decode message payload as UTF-8: {}", e);
                            }
                        }
                    } else {
                        warn!("⚠️ Received empty message payload");
                    }
                }
                Err(e) => {
                    error!("❌ Kafka consumer error: {}", e);
                    // Sleep briefly to avoid tight error loop
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                }
            }
        }

        info!("🛑 Order processor stopped");
        Ok(())
    }

    async fn process_order(&self, order: &Order) -> Result<OrderStatus> {
        let start_time = Instant::now();
        info!(
            "🚀 Processing REAL order {} on exchange {} (symbol: {}, side: {}, qty: {}, type: {})",
            order.id, order.exchange, order.symbol, order.side, order.qty, order.order_type
        );

        // 1. Validate the order
        if order.qty <= 0.0 {
            return Ok(OrderStatus {
                order_id: order.id.clone(),
                status: "REJECTED".to_string(),
                filled_qty: 0.0,
                avg_price: 0.0,
                timestamp: chrono::Utc::now().timestamp(),
                error: Some("Invalid quantity: must be > 0".to_string()),
            });
        }

        // 2. Parse exchange ID
        let exchange_id = match order.exchange.as_str() {
            "binance" => ExchangeId::BinanceSpot,
            "coinbase" => ExchangeId::Coinbase,
            "bybit" => ExchangeId::BybitPerpetualsUsd,
            "bitget" => ExchangeId::Bitget,
            "hyperliquid" => ExchangeId::Hyperliquid,
            "kucoin" => ExchangeId::Kucoin,
            "kraken" => ExchangeId::Kraken,
            "okx" => ExchangeId::Okx,
            _ => {
                error!("❌ Unsupported exchange: {}", order.exchange);
                return Ok(OrderStatus {
                    order_id: order.id.clone(),
                    status: "REJECTED".to_string(),
                    filled_qty: 0.0,
                    avg_price: 0.0,
                    timestamp: chrono::Utc::now().timestamp(),
                    error: Some(format!("Unsupported exchange: {}. Supported: binance, coinbase, bybit, bitget, hyperliquid, kucoin, kraken, okx", order.exchange)),
                });
            }
        };

        // 3. **PRODUCTION ORDER EXECUTION** - Use real exchange connectors
        let order_request = OrderRequest {
            exchange: order.exchange.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            order_type: order.order_type.clone(),
            price: order.price,
            quantity: order.qty,
            time_in_force: Some("GTC".to_string()), // Good Till Cancelled
            reduce_only: None,
            post_only: None,
        };

        // Execute order through the real exchange connector
        let result = self.connector_manager
            .place_order(exchange_id, order_request)
            .await;

        let elapsed_ms = start_time.elapsed().as_millis();

        match result {
            Ok(order_response) => {
                info!(
                    "✅ REAL order {} executed successfully on {} in {}ms: status={}, filled={}/{}, price={}, fees={}",
                    order.id, 
                    order.exchange,
                    elapsed_ms,
                    order_response.status,
                    order_response.filled,
                    order_response.quantity,
                    order_response.price,
                    order_response.fees
                );

                // Performance validation - target <50ms API response
                if elapsed_ms > 50 {
                    warn!(
                        "⚠️ Slow order execution: {}ms (target: <50ms) for order {} on {}",
                        elapsed_ms, order.id, order.exchange
                    );
                }

                Ok(OrderStatus {
                    order_id: order_response.id,
                    status: order_response.status,
                    filled_qty: order_response.filled,
                    avg_price: order_response.price,
                    timestamp: order_response.updated_at,
                    error: None,
                })
            }
            Err(e) => {
                error!(
                    "❌ REAL order {} FAILED on {} after {}ms: {}",
                    order.id, order.exchange, elapsed_ms, e
                );
                
                // Determine if it's a temporary or permanent failure
                let error_msg = e.to_string();
                let status = if error_msg.contains("rate limit") || error_msg.contains("timeout") {
                    "RETRY_LATER" // Temporary failure
                } else if error_msg.contains("insufficient") || error_msg.contains("balance") {
                    "REJECTED" // Permanent failure - insufficient funds
                } else if error_msg.contains("invalid") || error_msg.contains("symbol") {
                    "REJECTED" // Permanent failure - invalid parameters
                } else {
                    "FAILED" // General failure
                };
                
                Ok(OrderStatus {
                    order_id: order.id.clone(),
                    status: status.to_string(),
                    filled_qty: 0.0,
                    avg_price: 0.0,
                    timestamp: chrono::Utc::now().timestamp(),
                    error: Some(error_msg),
                })
            }
        }
    }

    pub async fn stop(&self) {
        info!("🛑 Stopping order processor...");
        self.running.store(false, Ordering::Relaxed);
    }
}
