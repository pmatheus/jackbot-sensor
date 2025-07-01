//! Sensor manager - Handles exchange connections and data streaming

use anyhow::Result;
use jackbot_data::{
    exchange::binance::futures::BinanceFuturesUsd,
    kafka_store::{KafkaClientStore, KafkaStore},
    streams::Streams,
    subscription::{trade::PublicTrades, book::OrderBooksL2},
};
use jackbot_instrument::instrument::market_data::kind::MarketDataInstrumentKind;
use futures_util::StreamExt;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::config::SensorConfig;
use crate::order_processor::OrderProcessor;

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
}

impl SensorManager {
    pub async fn new(config: SensorConfig, instance_id: Option<String>) -> Result<Self> {
        info!("📡 Initializing sensor manager...");
        
        // Connect to Kafka (using mock implementation for now)
        let kafka_store = Arc::new(KafkaClientStore::new());
        info!("✅ Connected to Kafka (mock mode)");

        Ok(Self {
            config,
            instance_id,
            kafka_store,
            order_processor: None,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("🚀 Starting sensor streams...");

        // Start order processor
        let order_processor = OrderProcessor::new(self.kafka_store.clone()).await?;
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
        info!("🔌 Starting Binance Futures streams...");

        // Start trade streams
        let trade_streams = Streams::<PublicTrades>::builder()
            .subscribe([
                (BinanceFuturesUsd::default(), "btc", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
                (BinanceFuturesUsd::default(), "eth", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
                (BinanceFuturesUsd::default(), "bnb", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
                (BinanceFuturesUsd::default(), "sol", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
                (BinanceFuturesUsd::default(), "xrp", "usdt", MarketDataInstrumentKind::Perpetual, PublicTrades),
            ])
            .init()
            .await?;

        info!("✅ Trade streams initialized");

        // Start order book streams
        let book_streams = Streams::<OrderBooksL2>::builder()
            .subscribe([
                (BinanceFuturesUsd::default(), "btc", "usdt", MarketDataInstrumentKind::Perpetual, OrderBooksL2),
                (BinanceFuturesUsd::default(), "eth", "usdt", MarketDataInstrumentKind::Perpetual, OrderBooksL2),
                (BinanceFuturesUsd::default(), "bnb", "usdt", MarketDataInstrumentKind::Perpetual, OrderBooksL2),
            ])
            .init()
            .await?;

        info!("✅ Order book streams initialized");

        // Process trade streams
        let kafka_store_trades = self.kafka_store.clone();
        tokio::spawn(async move {
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
                        
                        // TODO: Implement trade storage to Kafka
                        debug!("Storing trade: {:?}", market_event);
                        
                        // TODO: Implement trade publishing to Kafka
                        debug!("Trade received: {:?}", market_event);
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

        // Process order book streams
        let kafka_store_books = self.kafka_store.clone();
        tokio::spawn(async move {
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
}