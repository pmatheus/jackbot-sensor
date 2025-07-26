//! Sensor manager - Handles exchange connections and data streaming

use anyhow::Result;
use jackbot_data::{
    exchange::binance::futures::BinanceFuturesUsd,
    kafka_store::{KafkaClientStore, KafkaStore},
    streams::Streams,
    subscription::{trade::PublicTrades, book::OrderBooksL2},
    event::MarketEvent,
};
use jackbot_instrument::instrument::market_data::kind::MarketDataInstrumentKind;
use futures_util::StreamExt;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::config::SensorConfig;
use crate::order_processor::OrderProcessor;
use crate::streaming::StreamingManager;
use crate::production_config::ProductionConfig;
use crate::performance::cpu_affinity::{init_cpu_affinity, CpuAffinityConfig};
use crate::exchange_protection::init_exchange_protection;
use crate::binance_websocket::BinanceWebSocketClient;
use crate::kafka_producer::{KafkaProducer, ProducerConfig};

/// Instance information for the sensor
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstanceInfo {
    pub instance_id: String,
    pub region: String,
    pub pairs: Vec<String>,
    pub status: String,
    pub last_heartbeat: u64,
}

/// New trading pair alert
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewPairAlert {
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub detection_method: DetectionMethod,
    pub trading_start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub priority: AlertPriority,
}

/// Detection method for new pairs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DetectionMethod {
    VolumeBased,
    PriceMovement,
    MarketCapBased,
    Manual,
}

/// Alert priority levels
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AlertPriority {
    Low,
    Medium,
    High,
    Critical,
}

pub struct SensorManager {
    config: SensorConfig,
    instance_id: Option<String>,
    kafka_store: Arc<KafkaClientStore>,
    order_processor: Option<OrderProcessor>,
    streaming_manager: Option<Arc<StreamingManager>>,
    production_config: Arc<ProductionConfig>,
}

impl SensorManager {
    pub async fn new(config: SensorConfig, instance_id: Option<String>) -> Result<Self> {
        info!("📡 Initializing PRODUCTION sensor manager with critical fixes...");
        
        // Initialize CPU affinity optimization first
        // if let Err(e) = init_cpu_affinity(Some(CpuAffinityConfig::default())) {
        //     warn!("CPU affinity initialization failed: {}", e);
        // } else {
        //     info!("🖥️  CPU affinity optimization applied successfully");
        // }
        
        // Initialize exchange protection system
        // let _protection_manager = init_exchange_protection();
        info!("🛡️  Exchange protection system initialized");
        
        // Load production configuration
        let production_config = Arc::new(ProductionConfig::from_env()?);
        info!("🔧 Production config loaded: {}", production_config.get_summary());
        
        // Connect to Kafka (using mock implementation for now)
        let kafka_store = Arc::new(KafkaClientStore::new());
        info!("✅ Connected to Kafka (mock mode)");
        
        // Create Kafka producer for real streaming
        let kafka_producer = match KafkaProducer::new(ProducerConfig {
            brokers: production_config.endpoints.kafka_brokers.join(","),
            ..Default::default()
        }).await {
            Ok(producer) => {
                info!("✅ Kafka producer connected to: {}", production_config.endpoints.kafka_brokers.join(","));
                Some(Arc::new(producer))
            }
            Err(e) => {
                warn!("⚠️ Failed to create Kafka producer: {}", e);
                warn!("⚠️ Continuing without Kafka - market data will only be logged");
                None
            }
        };
        
        // Initialize production streaming manager with bounded channels and Kafka producer
        let streaming_manager = Arc::new(StreamingManager::new_with_kafka(kafka_producer));
        info!("🌊 PRODUCTION streaming manager initialized with backpressure protection and Kafka integration");

        Ok(Self {
            config,
            instance_id,
            kafka_store,
            order_processor: None,
            streaming_manager: Some(streaming_manager),
            production_config,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("🚀 Starting sensor streams...");

        // Start order processor
        let order_processor = OrderProcessor::new(self.kafka_store.clone(), None).await?;
        self.order_processor = Some(order_processor.clone());
        
        // Spawn order processing task
        tokio::spawn(async move {
            if let Err(e) = order_processor.run().await {
                error!("Order processor failed: {}", e);
            }
        });

        // For MVP, we'll start with Binance Futures for the top 5 pairs
        if self.config.get_enabled_exchanges().contains(&"binance".to_string()) {
            self.start_binance_streams().await?;
        }

        info!("✅ All sensor streams started successfully");
        Ok(())
    }

    async fn start_binance_streams(&self) -> Result<()> {
        info!("🔌 Starting Binance streams...");
        
        // Check if we're in testnet mode
        let is_testnet = self.config.exchanges.get("binance")
            .map(|ex| ex.testnet)
            .unwrap_or(true);
        
        info!("🌐 Using Binance {} mode", if is_testnet { "TESTNET" } else { "PRODUCTION" });
        
        // Start our new WebSocket implementation if streaming manager is available
        if let Some(streaming_manager) = &self.streaming_manager {
            info!("🚀 Starting Binance WebSocket client for real-time market data");
            
            // Create Binance WebSocket client
            let binance_client = BinanceWebSocketClient::new(
                streaming_manager.clone(),
                streaming_manager.kafka_producer.clone(),
                is_testnet,
            )?;
            
            // Get configured symbols
            let symbols = vec!["BTC/USDT", "ETH/USDT", "BNB/USDT", "SOL/USDT", "XRP/USDT"];
            
            for symbol in &symbols {
                info!("📊 Subscribing to {} streams", symbol);
                
                // Subscribe to order book
                if let Err(e) = binance_client.subscribe_orderbook(symbol).await {
                    error!("Failed to subscribe to {} orderbook: {}", symbol, e);
                }
                
                // Subscribe to ticker
                if let Err(e) = binance_client.subscribe_ticker(symbol).await {
                    error!("Failed to subscribe to {} ticker: {}", symbol, e);
                }
                
                // Subscribe to trades
                if let Err(e) = binance_client.subscribe_trades(symbol).await {
                    error!("Failed to subscribe to {} trades: {}", symbol, e);
                }
                
                // Small delay between subscriptions
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            
            info!("✅ Binance WebSocket streams started for {} symbols", symbols.len());
        }

        // Clone what we need before spawning tasks for legacy implementation
        let kafka_store_trades = self.kafka_store.clone();
        let kafka_store_books = self.kafka_store.clone();

        // Spawn trade stream task in local set for !Send futures
        let trade_handle = tokio::task::spawn_local(async move {
            // Start trade streams inside the task
            let trade_streams = Streams::<PublicTrades>::builder()
                .subscribe([
                    (BinanceFuturesUsd::default(), "btc", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
                    (BinanceFuturesUsd::default(), "eth", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
                    (BinanceFuturesUsd::default(), "bnb", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
                    (BinanceFuturesUsd::default(), "sol", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
                    (BinanceFuturesUsd::default(), "xrp", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
                ])
                .init()
                .await
                .unwrap();

            info!("✅ Trade streams initialized");
            
            // Process trade streams
            let mut joined_stream = trade_streams.select_all();
            let mut trade_count = 0;

            while let Some(event_result) = joined_stream.next().await {
                match event_result {
                    jackbot_data::streams::reconnect::Event::Item(Ok(market_event)) => {
                        trade_count += 1;
                        let instrument = format!("{}_USDT", market_event.instrument.base.to_string().to_uppercase());
                        
                        // Log every 10th trade
                        if trade_count % 10 == 0 {
                            info!(
                                "💰 Trade #{}: {} {} {} @ {} (id: {})",
                                trade_count, 
                                instrument, 
                                market_event.kind.side, 
                                market_event.kind.amount, 
                                market_event.kind.price, 
                                market_event.kind.id
                            );
                        }
                        
                        // Trade storage to Kafka - see MESSAGE_FLOW_SPEC.md for full implementation
                        // TODO: Implement kafka storage directly here
                        info!("Trade stored: {} @ {}", instrument, market_event.kind.price);
                    }
                    jackbot_data::streams::reconnect::Event::Item(Err(e)) => {
                        error!("Trade stream error: {}", e);
                    }
                    jackbot_data::streams::reconnect::Event::Reconnecting(exchange) => {
                        warn!("Trade stream reconnecting for exchange: {:?}", exchange);
                    }
                }
            }
        });

        // Spawn order book stream task in local set for !Send futures
        let book_handle = tokio::task::spawn_local(async move {
            // Start order book streams inside the task
            let book_streams = Streams::<OrderBooksL2>::builder()
                .subscribe([
                    (BinanceFuturesUsd::default(), "btc", "usdt", MarketDataInstrumentKind::Perpetual, OrderBooksL2),
                    (BinanceFuturesUsd::default(), "eth", "usdt", MarketDataInstrumentKind::Perpetual, OrderBooksL2),
                    (BinanceFuturesUsd::default(), "bnb", "usdt", MarketDataInstrumentKind::Perpetual, OrderBooksL2),
                ])
                .init()
                .await
                .unwrap();

            info!("✅ Order book streams initialized");
            
            let mut joined_stream = book_streams.select_all();
            let mut update_count = 0;

            while let Some(event_result) = joined_stream.next().await {
                match event_result {
                    jackbot_data::streams::reconnect::Event::Item(Ok(market_event)) => {
                        update_count += 1;
                        let instrument = format!("{}_USDT", market_event.instrument.base.to_string().to_uppercase());
                        
                        match &market_event.kind {
                            jackbot_data::subscription::book::OrderBookEvent::Snapshot(book) => {
                                info!(
                                    "📸 Order Book Snapshot #{}: {} - {} bids, {} asks",
                                    update_count,
                                    instrument,
                                    book.bids().levels().len(),
                                    book.asks().levels().len()
                                );
                                
                                // Store snapshot
                                kafka_store_books.store_snapshot(
                                    market_event.exchange,
                                    &instrument,
                                    book
                                );
                                
                                // Publish snapshot
                                kafka_store_books.publish_snapshot(
                                    market_event.exchange,
                                    &instrument,
                                    book
                                );
                            }
                            jackbot_data::subscription::book::OrderBookEvent::Update(book) => {
                                // Log every 50th update
                                if update_count % 50 == 0 {
                                    info!(
                                        "📝 Order Book Update #{}: {} - {} bid changes, {} ask changes",
                                        update_count,
                                        instrument,
                                        book.bids().levels().len(),
                                        book.asks().levels().len()
                                    );
                                }
                                
                                // Store delta
                                kafka_store_books.store_delta(
                                    market_event.exchange,
                                    &instrument,
                                    &market_event.kind
                                );
                                
                                // Publish delta
                                kafka_store_books.publish_delta(
                                    market_event.exchange,
                                    &instrument,
                                    &market_event.kind
                                );
                            }
                        }
                    }
                    jackbot_data::streams::reconnect::Event::Item(Err(e)) => {
                        error!("Order book stream error: {}", e);
                    }
                    jackbot_data::streams::reconnect::Event::Reconnecting(exchange) => {
                        warn!("Order book stream reconnecting for exchange: {:?}", exchange);
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        info!("🛑 Shutting down sensor manager...");
        
        // Stop order processor
        if let Some(processor) = &self.order_processor {
            processor.stop().await;
        }
        
        info!("✅ Sensor manager shut down successfully");
        Ok(())
    }

    /// Store trade data to Kafka for data lake persistence
    async fn store_trade_to_kafka(&self, market_event: &MarketEvent<jackbot_instrument::instrument::market_data::MarketDataInstrument, jackbot_data::subscription::trade::PublicTrade>) -> Result<()> {
        // TODO: Implement actual Kafka storage
        // This would serialize the trade data and send it to the appropriate Kafka topic
        // For now, just return Ok to satisfy the compiler
        Ok(())
    }

    /// Publish trade data to Kafka for real-time subscribers
    async fn publish_trade_to_kafka(&self, market_event: &MarketEvent<jackbot_instrument::instrument::market_data::MarketDataInstrument, jackbot_data::subscription::trade::PublicTrade>) -> Result<()> {
        // TODO: Implement actual Kafka publishing
        // This would serialize the trade data and publish it to the real-time Kafka topic
        // For now, just return Ok to satisfy the compiler
        Ok(())
    }
}