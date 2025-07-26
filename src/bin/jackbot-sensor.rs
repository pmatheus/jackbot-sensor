//! High-Performance Jackbot Sensor Binary
//! 
//! Streams real-time market data from Binance to Kafka with ultra-low latency

use anyhow::{Context, Result};
use clap::Parser;
use futures_util::StreamExt;
use jackbot_data::{
    exchange::binance::spot::BinanceSpot,
    streams::{Streams, reconnect::stream::ReconnectingStream},
    subscription::{
        book::OrderBooksL2,
        trade::PublicTrades,
    },
    exchange::binance::book::l2::BinanceOrderBookL2,
    exchange::binance::trade::BinanceTrade,
};
use jackbot_instrument::{
    exchange::ExchangeId,
    instrument::market_data::kind::MarketDataInstrumentKind,
};
// Import from the correct module path
use jackbot_sensor::api::{OrderBookData, TradeData, TickerData};
use jackbot_sensor::kafka_producer::{KafkaProducer, ProducerConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::signal;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "jackbot-sensor")]
#[command(about = "High-performance market data sensor for Jackbot")]
#[command(version = "1.0.0")]
struct Args {
    /// Kafka broker addresses
    #[arg(long, env = "KAFKA_BROKERS", default_value = "localhost:9092")]
    kafka_brokers: String,

    /// Trading pairs to monitor (comma-separated)
    #[arg(short, long, default_value = "btc/usdt,eth/usdt")]
    pairs: String,

    /// Enable high-frequency pairs on separate connections
    #[arg(long)]
    high_frequency: bool,

    /// Maximum messages per second (0 = unlimited)
    #[arg(long, default_value = "0")]
    rate_limit: u32,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Batch size for Kafka producer
    #[arg(long, default_value = "16384")]
    batch_size: usize,

    /// Linger time in milliseconds
    #[arg(long, default_value = "0")]
    linger_ms: u64,

    /// Compression type (none, gzip, snappy, lz4, zstd)
    #[arg(long, default_value = "snappy")]
    compression: String,
}

/// Performance metrics for monitoring
#[derive(Debug, Default)]
struct SensorMetrics {
    pub orderbooks_processed: std::sync::atomic::AtomicU64,
    pub trades_processed: std::sync::atomic::AtomicU64,
    pub last_update_time: RwLock<Instant>,
    pub errors: std::sync::atomic::AtomicU64,
}

impl SensorMetrics {
    async fn update_last_time(&self) {
        *self.last_update_time.write().await = Instant::now();
    }

    fn increment_orderbooks(&self) {
        self.orderbooks_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn increment_trades(&self) {
        self.trades_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn increment_errors(&self) {
        self.errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize logging
    init_logging(args.debug);
    
    info!("Starting Jackbot Sensor...");
    info!("Kafka brokers: {}", args.kafka_brokers);
    info!("Trading pairs: {}", args.pairs);
    
    // Create Kafka producer
    let producer_config = ProducerConfig {
        brokers: args.kafka_brokers.clone(),
        client_id: "jackbot-sensor".to_string(),
        batch_size: args.batch_size,
        linger_ms: args.linger_ms,
        compression_type: args.compression.clone(),
        ..Default::default()
    };
    
    let kafka_producer = Arc::new(
        KafkaProducer::new(producer_config).await
            .context("Failed to create Kafka producer")?
    );
    
    info!("Kafka producer initialized successfully");
    
    // Parse trading pairs
    let pairs: Vec<(String, String)> = args.pairs
        .split(',')
        .map(|pair| {
            let parts: Vec<&str> = pair.trim().split('/').collect();
            if parts.len() != 2 {
                error!("Invalid pair format: {}", pair);
                ("btc".to_string(), "usdt".to_string())
            } else {
                (parts[0].to_string(), parts[1].to_string())
            }
        })
        .collect();
    
    // Initialize metrics
    let metrics = Arc::new(SensorMetrics::default());
    
    // Start metrics reporting task
    let metrics_clone = metrics.clone();
    tokio::spawn(async move {
        report_metrics(metrics_clone).await;
    });
    
    // Create orderbook and trade streams
    let (orderbook_streams, trade_streams) = if args.high_frequency {
        create_high_frequency_streams(&pairs).await?
    } else {
        create_standard_streams(&pairs).await?
    };
    
    info!("WebSocket streams initialized");
    
    // Process streams concurrently
    let orderbook_handle = tokio::spawn(process_orderbook_stream(
        orderbook_streams,
        kafka_producer.clone(),
        metrics.clone(),
        args.rate_limit,
    ));
    
    let trade_handle = tokio::spawn(process_trade_stream(
        trade_streams,
        kafka_producer.clone(),
        metrics.clone(),
        args.rate_limit,
    ));
    
    info!("Sensor is running. Press Ctrl+C to stop.");
    
    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = orderbook_handle => {
            warn!("Orderbook processor terminated unexpectedly");
        }
        _ = trade_handle => {
            warn!("Trade processor terminated unexpectedly");
        }
    }
    
    // Graceful shutdown
    info!("Shutting down sensor...");
    kafka_producer.shutdown().await?;
    
    // Print final metrics
    let orderbooks = metrics.orderbooks_processed.load(std::sync::atomic::Ordering::Relaxed);
    let trades = metrics.trades_processed.load(std::sync::atomic::Ordering::Relaxed);
    let errors = metrics.errors.load(std::sync::atomic::Ordering::Relaxed);
    
    info!("Final metrics:");
    info!("  Orderbooks processed: {}", orderbooks);
    info!("  Trades processed: {}", trades);
    info!("  Errors: {}", errors);
    
    Ok(())
}

/// Create standard streams with shared connections for lower volume pairs
async fn create_standard_streams(pairs: &[(String, String)]) -> Result<(
    Streams<OrderBooksL2>,
    Streams<PublicTrades>,
)> {
    // Create orderbook streams
    let mut orderbook_builder = Streams::<OrderBooksL2>::builder();
    
    // Subscribe all pairs on a single connection
    let subscriptions: Vec<_> = pairs.iter()
        .map(|(base, quote)| {
            (BinanceSpot::default(), base.as_str(), quote.as_str(), 
             MarketDataInstrumentKind::Spot, OrderBooksL2)
        })
        .collect();
    
    orderbook_builder = orderbook_builder.subscribe(subscriptions);
    let orderbook_streams = orderbook_builder.init().await
        .context("Failed to initialize orderbook streams")?;
    
    // Create trade streams
    let mut trade_builder = Streams::<PublicTrades>::builder();
    
    let trade_subscriptions: Vec<_> = pairs.iter()
        .map(|(base, quote)| {
            (BinanceSpot::default(), base.as_str(), quote.as_str(), 
             MarketDataInstrumentKind::Spot, PublicTrades)
        })
        .collect();
    
    trade_builder = trade_builder.subscribe(trade_subscriptions);
    let trade_streams = trade_builder.init().await
        .context("Failed to initialize trade streams")?;
    
    Ok((orderbook_streams, trade_streams))
}

/// Create high-frequency streams with separate connections for high-volume pairs
async fn create_high_frequency_streams(pairs: &[(String, String)]) -> Result<(
    Streams<OrderBooksL2>,
    Streams<PublicTrades>,
)> {
    // Create orderbook streams with separate connections for each pair
    let mut orderbook_builder = Streams::<OrderBooksL2>::builder();
    
    for (base, quote) in pairs {
        orderbook_builder = orderbook_builder.subscribe([
            (BinanceSpot::default(), base.as_str(), quote.as_str(), 
             MarketDataInstrumentKind::Spot, OrderBooksL2)
        ]);
    }
    
    let orderbook_streams = orderbook_builder.init().await
        .context("Failed to initialize orderbook streams")?;
    
    // Create trade streams with separate connections for each pair
    let mut trade_builder = Streams::<PublicTrades>::builder();
    
    for (base, quote) in pairs {
        trade_builder = trade_builder.subscribe([
            (BinanceSpot::default(), base.as_str(), quote.as_str(), 
             MarketDataInstrumentKind::Spot, PublicTrades)
        ]);
    }
    
    let trade_streams = trade_builder.init().await
        .context("Failed to initialize trade streams")?;
    
    Ok((orderbook_streams, trade_streams))
}

/// Process orderbook stream and publish to Kafka
async fn process_orderbook_stream(
    mut streams: Streams<OrderBooksL2>,
    kafka_producer: Arc<KafkaProducer>,
    metrics: Arc<SensorMetrics>,
    rate_limit: u32,
) -> Result<()> {
    let mut stream = streams
        .select(ExchangeId::BinanceSpot)
        .context("Failed to select Binance stream")?
        .with_error_handler(|error| {
            warn!(?error, "Orderbook stream error");
        });
    
    let mut last_rate_check = Instant::now();
    let mut message_count = 0u32;
    
    while let Some(event) = stream.next().await {
        debug!("Received orderbook event: {:?}", event);
        
        // Rate limiting
        if rate_limit > 0 {
            message_count += 1;
            if last_rate_check.elapsed() >= Duration::from_secs(1) {
                if message_count > rate_limit {
                    warn!("Rate limit exceeded: {} > {}", message_count, rate_limit);
                }
                message_count = 0;
                last_rate_check = Instant::now();
            }
            
            if message_count > rate_limit {
                continue; // Skip this message
            }
        }
        
        // Convert to OrderBookData and publish
        match convert_orderbook_event(&event) {
            Ok(orderbook_data) => {
                if let Err(e) = kafka_producer.publish_orderbook(&orderbook_data).await {
                    error!("Failed to publish orderbook: {}", e);
                    metrics.increment_errors();
                } else {
                    metrics.increment_orderbooks();
                    metrics.update_last_time().await;
                }
            }
            Err(e) => {
                error!("Failed to convert orderbook event: {}", e);
                metrics.increment_errors();
            }
        }
    }
    
    Ok(())
}

/// Process trade stream and publish to Kafka
async fn process_trade_stream(
    mut streams: Streams<PublicTrades>,
    kafka_producer: Arc<KafkaProducer>,
    metrics: Arc<SensorMetrics>,
    rate_limit: u32,
) -> Result<()> {
    let mut stream = streams
        .select(ExchangeId::BinanceSpot)
        .context("Failed to select Binance stream")?
        .with_error_handler(|error| {
            warn!(?error, "Trade stream error");
        });
    
    let mut last_rate_check = Instant::now();
    let mut message_count = 0u32;
    
    while let Some(event) = stream.next().await {
        debug!("Received trade event: {:?}", event);
        
        // Rate limiting
        if rate_limit > 0 {
            message_count += 1;
            if last_rate_check.elapsed() >= Duration::from_secs(1) {
                if message_count > rate_limit {
                    warn!("Rate limit exceeded: {} > {}", message_count, rate_limit);
                }
                message_count = 0;
                last_rate_check = Instant::now();
            }
            
            if message_count > rate_limit {
                continue; // Skip this message
            }
        }
        
        // Convert to TradeData and publish
        match convert_trade_event(&event) {
            Ok(trade_data) => {
                if let Err(e) = kafka_producer.publish_trade(&trade_data).await {
                    error!("Failed to publish trade: {}", e);
                    metrics.increment_errors();
                } else {
                    metrics.increment_trades();
                    metrics.update_last_time().await;
                }
            }
            Err(e) => {
                error!("Failed to convert trade event: {}", e);
                metrics.increment_errors();
            }
        }
    }
    
    Ok(())
}

/// Convert Binance orderbook event to OrderBookData
fn convert_orderbook_event(event: &jackbot_data::event::MarketEvent<BinanceOrderBookL2>) -> Result<OrderBookData> {
    let orderbook = &event.inner;
    
    // Extract bids and asks in the format expected by the API
    let bids: Vec<[f64; 2]> = orderbook.bids.iter()
        .map(|level| [
            level.price.parse::<f64>().unwrap_or(0.0),
            level.amount.parse::<f64>().unwrap_or(0.0)
        ])
        .collect();
    
    let asks: Vec<[f64; 2]> = orderbook.asks.iter()
        .map(|level| [
            level.price.parse::<f64>().unwrap_or(0.0),
            level.amount.parse::<f64>().unwrap_or(0.0)
        ])
        .collect();
    
    Ok(OrderBookData {
        symbol: format!("{}/{}", orderbook.instrument.base, orderbook.instrument.quote),
        exchange: "binance".to_string(),
        bids,
        asks,
        timestamp: chrono::Utc::now().timestamp_millis(),
        sequence_id: Some(orderbook.last_update_id),
    })
}

/// Convert Binance trade event to TradeData
fn convert_trade_event(event: &jackbot_data::event::MarketEvent<BinanceTrade>) -> Result<TradeData> {
    let trade = &event.inner;
    
    Ok(TradeData {
        id: trade.id.to_string(),
        symbol: format!("{}/{}", trade.instrument.base, trade.instrument.quote),
        exchange: "binance".to_string(),
        price: trade.price.parse::<f64>().unwrap_or(0.0),
        quantity: trade.quantity.parse::<f64>().unwrap_or(0.0),
        side: if trade.buyer_maker { "sell" } else { "buy" }.to_string(),
        timestamp: trade.event_time,
    })
}

/// Report metrics periodically
async fn report_metrics(metrics: Arc<SensorMetrics>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    
    loop {
        interval.tick().await;
        
        let orderbooks = metrics.orderbooks_processed.load(std::sync::atomic::Ordering::Relaxed);
        let trades = metrics.trades_processed.load(std::sync::atomic::Ordering::Relaxed);
        let errors = metrics.errors.load(std::sync::atomic::Ordering::Relaxed);
        let last_update = *metrics.last_update_time.read().await;
        let elapsed = last_update.elapsed();
        
        info!("Metrics - Orderbooks: {}, Trades: {}, Errors: {}, Last update: {:?} ago",
              orderbooks, trades, errors, elapsed);
        
        // Get Kafka producer metrics
        // Note: This would need to be implemented in the actual integration
    }
}

/// Initialize logging
fn init_logging(debug: bool) {
    let filter_level = if debug {
        tracing_subscriber::filter::LevelFilter::DEBUG
    } else {
        tracing_subscriber::filter::LevelFilter::INFO
    };
    
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::filter::EnvFilter::builder()
                .with_default_directive(filter_level.into())
                .from_env_lossy(),
        )
        .with_ansi(true)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(debug)
        .with_line_number(debug)
        .json()
        .init();
}