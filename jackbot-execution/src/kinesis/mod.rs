use crate::{
    data_gathering::MarketDataCollector, strategy::event_driven::EventDrivenStrategy,
    testing::TestOrderExecutionEngine,
};
use aws_sdk_kinesis::Client as KinesisClient;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, RwLock},
    time::interval,
};
use tracing::{debug, error, info, warn};

pub mod consumer;
pub mod processor;
pub mod types;

pub use consumer::KinesisMessageConsumer;
pub use processor::MessageProcessor;
pub use types::*;

/// Kinesis integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KinesisConfig {
    /// AWS region
    pub region: String,
    /// Stream names to consume from
    pub streams: StreamConfig,
    /// Consumer configuration
    pub consumer: ConsumerConfig,
    /// Processing configuration
    pub processing: ProcessingConfig,
}

/// Stream configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Order execution stream
    pub order_execution_stream: String,
    /// Strategy execution stream
    pub strategy_execution_stream: String,
    /// Risk alerts stream
    pub risk_alerts_stream: String,
    /// Market data stream (for supplementary data)
    pub market_data_stream: String,
}

/// Consumer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerConfig {
    /// Application name for Kinesis Consumer Library
    pub application_name: String,
    /// Consumer group name
    pub consumer_group: String,
    /// Shard iterator type
    pub shard_iterator_type: String,
    /// Max records per batch
    pub max_records_per_batch: u32,
    /// Polling interval in milliseconds
    pub polling_interval_ms: u64,
    /// Retry configuration
    pub retry_config: RetryConfig,
}

/// Processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    /// Max concurrent message processing
    pub max_concurrent_processing: usize,
    /// Message timeout in seconds
    pub message_timeout_seconds: u64,
    /// Enable dead letter queue
    pub enable_dead_letter_queue: bool,
    /// Max processing attempts
    pub max_processing_attempts: u32,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Max retry attempts
    pub max_attempts: u32,
    /// Base delay in milliseconds
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for KinesisConfig {
    fn default() -> Self {
        Self {
            region: "us-east-1".to_string(),
            streams: StreamConfig {
                order_execution_stream: "jackbot-order-execution".to_string(),
                strategy_execution_stream: "jackbot-strategy-execution".to_string(),
                risk_alerts_stream: "jackbot-risk-alerts".to_string(),
                market_data_stream: "jackbot-market-data".to_string(),
            },
            consumer: ConsumerConfig {
                application_name: "jackbot-sensor".to_string(),
                consumer_group: "sensor-consumers".to_string(),
                shard_iterator_type: "LATEST".to_string(),
                max_records_per_batch: 100,
                polling_interval_ms: 1000,
                retry_config: RetryConfig {
                    max_attempts: 3,
                    base_delay_ms: 1000,
                    max_delay_ms: 30000,
                    backoff_multiplier: 2.0,
                },
            },
            processing: ProcessingConfig {
                max_concurrent_processing: 10,
                message_timeout_seconds: 30,
                enable_dead_letter_queue: true,
                max_processing_attempts: 3,
            },
        }
    }
}

/// Kinesis integration manager
#[derive(Debug)]
pub struct KinesisIntegration {
    config: KinesisConfig,
    client: KinesisClient,
    consumer: Arc<Mutex<KinesisMessageConsumer>>,
    processor: Arc<MessageProcessor>,
    execution_engine: Arc<TestOrderExecutionEngine>,
    market_data_collector: Arc<MarketDataCollector>,
    active_strategies: Arc<RwLock<HashMap<String, Arc<RwLock<EventDrivenStrategy>>>>>,
    metrics: Arc<RwLock<IntegrationMetrics>>,
}

/// Integration metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntegrationMetrics {
    /// Total messages received
    pub messages_received: u64,
    /// Messages processed successfully
    pub messages_processed: u64,
    /// Messages failed
    pub messages_failed: u64,
    /// Average processing time (ms)
    pub avg_processing_time_ms: f64,
    /// Last processed timestamp
    pub last_processed_at: Option<DateTime<Utc>>,
    /// Per-stream metrics
    pub stream_metrics: HashMap<String, StreamMetrics>,
}

/// Per-stream processing metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamMetrics {
    pub received: u64,
    pub processed: u64,
    pub failed: u64,
    pub avg_size_bytes: f64,
    pub last_received: Option<DateTime<Utc>>,
}

impl KinesisIntegration {
    /// Create new Kinesis integration
    pub async fn new(
        config: KinesisConfig,
        execution_engine: Arc<TestOrderExecutionEngine>,
        market_data_collector: Arc<MarketDataCollector>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = KinesisClient::new(&aws_config);

        let consumer = Arc::new(Mutex::new(KinesisMessageConsumer::new(
            client.clone(),
            config.consumer.clone(),
        )?));

        let processor = Arc::new(MessageProcessor::new(
            execution_engine.clone(),
            market_data_collector.clone(),
            config.processing.clone(),
        ));

        Ok(Self {
            config,
            client,
            consumer,
            processor,
            execution_engine,
            market_data_collector,
            active_strategies: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(IntegrationMetrics::default())),
        })
    }

    /// Start consuming from all configured streams
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Starting Kinesis integration with streams: {:?}",
            self.config.streams
        );

        // Start consuming from each stream
        let streams = vec![
            (
                &self.config.streams.order_execution_stream,
                StreamType::OrderExecution,
            ),
            (
                &self.config.streams.strategy_execution_stream,
                StreamType::StrategyExecution,
            ),
            (
                &self.config.streams.risk_alerts_stream,
                StreamType::RiskAlerts,
            ),
            (
                &self.config.streams.market_data_stream,
                StreamType::MarketData,
            ),
        ];

        for (stream_name, stream_type) in streams {
            let consumer = Arc::clone(&self.consumer);
            let processor = Arc::clone(&self.processor);
            let metrics = Arc::clone(&self.metrics);
            let stream_name = stream_name.clone();
            let config = self.config.clone();

            tokio::spawn(async move {
                Self::consume_stream(
                    consumer,
                    processor,
                    metrics,
                    stream_name,
                    stream_type,
                    config,
                )
                .await;
            });
        }

        // Start metrics collection
        self.start_metrics_collection().await;

        info!("Kinesis integration started successfully");
        Ok(())
    }

    /// Consume from a specific stream
    async fn consume_stream(
        consumer: Arc<Mutex<KinesisMessageConsumer>>,
        processor: Arc<MessageProcessor>,
        metrics: Arc<RwLock<IntegrationMetrics>>,
        stream_name: String,
        stream_type: StreamType,
        config: KinesisConfig,
    ) {
        let mut interval = interval(Duration::from_millis(config.consumer.polling_interval_ms));

        loop {
            interval.tick().await;

            let records_result = {
                let mut consumer_guard = consumer.lock().await;
                let consume_result = consumer_guard
                    .consume_records(&stream_name, config.consumer.max_records_per_batch)
                    .await;
                drop(consumer_guard);
                consume_result
            };

            match records_result {
                Ok(records) => {
                    if !records.is_empty() {
                        debug!(
                            "Received {} records from stream {}",
                            records.len(),
                            stream_name
                        );

                        // Update metrics
                        {
                            let mut metrics_guard = metrics.write().await;
                            metrics_guard.messages_received += records.len() as u64;

                            let stream_metrics = metrics_guard
                                .stream_metrics
                                .entry(stream_name.clone())
                                .or_insert_with(StreamMetrics::default);
                            stream_metrics.received += records.len() as u64;
                            stream_metrics.last_received = Some(Utc::now());
                        }

                        // Process records
                        for record in records {
                            let processor = Arc::clone(&processor);
                            let metrics = Arc::clone(&metrics);
                            let stream_name = stream_name.clone();

                            tokio::spawn(async move {
                                let start_time = std::time::Instant::now();

                                match processor.process_message(record, stream_type).await {
                                    Ok(_) => {
                                        let processing_time =
                                            start_time.elapsed().as_millis() as f64;

                                        let mut metrics_guard = metrics.write().await;
                                        metrics_guard.messages_processed += 1;
                                        metrics_guard.last_processed_at = Some(Utc::now());

                                        // Update average processing time
                                        let total_processed =
                                            metrics_guard.messages_processed as f64;
                                        metrics_guard.avg_processing_time_ms = (metrics_guard
                                            .avg_processing_time_ms
                                            * (total_processed - 1.0)
                                            + processing_time)
                                            / total_processed;

                                        let stream_metrics = metrics_guard
                                            .stream_metrics
                                            .entry(stream_name)
                                            .or_insert_with(StreamMetrics::default);
                                        stream_metrics.processed += 1;
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to process message from stream {}: {}",
                                            stream_name, e
                                        );

                                        let mut metrics_guard = metrics.write().await;
                                        metrics_guard.messages_failed += 1;

                                        let stream_metrics = metrics_guard
                                            .stream_metrics
                                            .entry(stream_name)
                                            .or_insert_with(StreamMetrics::default);
                                        stream_metrics.failed += 1;
                                    }
                                }
                            });
                        }
                    }
                }
                Err(e) => {
                    error!("Error consuming from stream {}: {}", stream_name, e);
                    tokio::time::sleep(Duration::from_millis(
                        config.consumer.retry_config.base_delay_ms,
                    ))
                    .await;
                }
            }
        }
    }

    /// Start metrics collection and reporting
    async fn start_metrics_collection(&self) {
        let metrics = Arc::clone(&self.metrics);
        let mut interval = interval(Duration::from_secs(60)); // Report every minute

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                let metrics_guard = metrics.read().await;
                info!(
                    "Kinesis Integration Metrics - Received: {}, Processed: {}, Failed: {}, Avg Processing Time: {:.2}ms",
                    metrics_guard.messages_received,
                    metrics_guard.messages_processed,
                    metrics_guard.messages_failed,
                    metrics_guard.avg_processing_time_ms
                );

                for (stream_name, stream_metrics) in &metrics_guard.stream_metrics {
                    debug!(
                        "Stream {} - Received: {}, Processed: {}, Failed: {}",
                        stream_name,
                        stream_metrics.received,
                        stream_metrics.processed,
                        stream_metrics.failed
                    );
                }
            }
        });
    }

    /// Add a new strategy to be managed
    pub async fn add_strategy(&self, strategy_id: String, strategy: EventDrivenStrategy) {
        let mut strategies = self.active_strategies.write().await;
        strategies.insert(strategy_id, Arc::new(RwLock::new(strategy)));
    }

    /// Remove a strategy from management
    pub async fn remove_strategy(
        &self,
        strategy_id: &str,
    ) -> Option<Arc<RwLock<EventDrivenStrategy>>> {
        let mut strategies = self.active_strategies.write().await;
        strategies.remove(strategy_id)
    }

    /// Get current integration metrics
    pub async fn get_metrics(&self) -> IntegrationMetrics {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    /// Stop Kinesis integration
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Stopping Kinesis integration");

        // Stop all active strategies
        let strategies = self.active_strategies.read().await;
        for (strategy_id, strategy) in strategies.iter() {
            info!("Stopping strategy: {}", strategy_id);
            let mut strategy_guard = strategy.write().await;
            let stop_result = strategy_guard.stop().await;
            if let Err(e) = stop_result {
                warn!("Error stopping strategy {}: {}", strategy_id, e);
            }
        }

        info!("Kinesis integration stopped");
        Ok(())
    }
}
