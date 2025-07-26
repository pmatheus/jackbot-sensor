//! Kafka Producer Integration Demo
//!
//! Demonstrates the Kafka producer integration with market data streaming
//! Run with: cargo run --example kafka_producer_demo

use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn, Level};
use tracing_subscriber;

// Mock data structures (normally from api.rs)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TickerData {
    pub symbol: String,
    pub exchange: String,
    pub price: f64,
    pub bid: f64,
    pub ask: f64,
    pub volume_24h: f64,
    pub change_24h: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrderBookData {
    pub symbol: String,
    pub exchange: String,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
    pub timestamp: i64,
    pub sequence_id: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TradeData {
    pub symbol: String,
    pub exchange: String,
    pub id: String,
    pub price: f64,
    pub quantity: f64,
    pub side: String,
    pub timestamp: i64,
    pub is_maker: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KlineData {
    pub symbol: String,
    pub exchange: String,
    pub interval: String,
    pub open_time: i64,
    pub close_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub quote_volume: Option<f64>,
    pub trade_count: Option<u32>,
}

// Simplified Kafka producer for demo
pub struct KafkaProducerDemo {
    brokers: String,
}

impl KafkaProducerDemo {
    pub async fn new(brokers: String) -> Result<Self> {
        info!("Creating Kafka producer demo with brokers: {}", brokers);
        
        // In real implementation, this would create rdkafka::FutureProducer
        // For demo, we just validate the configuration
        
        Ok(Self { brokers })
    }

    pub async fn publish_ticker(&self, ticker: &TickerData) -> Result<()> {
        let topic = format!("l2-data.{}.{}", 
            ticker.exchange.to_lowercase(), 
            ticker.symbol.replace('/', "").to_lowercase()
        );
        
        // In real implementation, this would:
        // 1. Serialize to Protocol Buffers
        // 2. Send to Kafka topic
        // 3. Handle delivery confirmation
        
        info!("📊 Publishing ticker to topic '{}': {} = ${:.2}", 
              topic, ticker.symbol, ticker.price);
        
        // Simulate network latency
        tokio::time::sleep(Duration::from_millis(5)).await;
        
        Ok(())
    }

    pub async fn publish_orderbook(&self, orderbook: &OrderBookData) -> Result<()> {
        let topic = format!("l2-data.{}.{}", 
            orderbook.exchange.to_lowercase(), 
            orderbook.symbol.replace('/', "").to_lowercase()
        );
        
        info!("📖 Publishing orderbook to topic '{}': {} (bid: ${:.2}, ask: ${:.2})", 
              topic, orderbook.symbol, 
              orderbook.bids.get(0).map(|b| b.0).unwrap_or(0.0),
              orderbook.asks.get(0).map(|a| a.0).unwrap_or(0.0));
        
        tokio::time::sleep(Duration::from_millis(3)).await;
        Ok(())
    }

    pub async fn publish_trade(&self, trade: &TradeData) -> Result<()> {
        let topic = format!("trades-data.{}.{}", 
            trade.exchange.to_lowercase(), 
            trade.symbol.replace('/', "").to_lowercase()
        );
        
        info!("💰 Publishing trade to topic '{}': {} {} {:.4} @ ${:.2}", 
              topic, trade.symbol, trade.side, trade.quantity, trade.price);
        
        tokio::time::sleep(Duration::from_millis(2)).await;
        Ok(())
    }

    pub async fn publish_kline(&self, kline: &KlineData) -> Result<()> {
        let topic = format!("klines-data.{}.{}", 
            kline.exchange.to_lowercase(), 
            kline.symbol.replace('/', "").to_lowercase()
        );
        
        info!("🕯️  Publishing kline to topic '{}': {} {} O:{:.2} H:{:.2} L:{:.2} C:{:.2}", 
              topic, kline.symbol, kline.interval, 
              kline.open, kline.high, kline.low, kline.close);
        
        tokio::time::sleep(Duration::from_millis(4)).await;
        Ok(())
    }

    pub fn get_metrics(&self) -> KafkaMetrics {
        // In real implementation, return actual metrics
        KafkaMetrics {
            messages_sent: 42,
            messages_failed: 0,
            avg_latency_ms: 15,
            throughput_msg_per_sec: 1250.0,
        }
    }
}

#[derive(Debug)]
pub struct KafkaMetrics {
    pub messages_sent: u64,
    pub messages_failed: u64,
    pub avg_latency_ms: u64,
    pub throughput_msg_per_sec: f64,
}

// Demo streaming manager
pub struct StreamingManagerDemo {
    kafka_producer: Option<Arc<KafkaProducerDemo>>,
}

impl StreamingManagerDemo {
    pub fn new_with_kafka(kafka_producer: Option<Arc<KafkaProducerDemo>>) -> Self {
        Self { kafka_producer }
    }

    pub async fn publish_ticker(&self, ticker: TickerData) -> Result<()> {
        // Simulate WebSocket publishing
        info!("🌐 WebSocket: Broadcasting ticker for {}", ticker.symbol);
        
        // Kafka publishing
        if let Some(ref kafka_producer) = self.kafka_producer {
            kafka_producer.publish_ticker(&ticker).await?;
        }
        
        Ok(())
    }

    pub async fn publish_orderbook(&self, orderbook: OrderBookData) -> Result<()> {
        info!("🌐 WebSocket: Broadcasting orderbook for {}", orderbook.symbol);
        
        if let Some(ref kafka_producer) = self.kafka_producer {
            kafka_producer.publish_orderbook(&orderbook).await?;
        }
        
        Ok(())
    }

    pub async fn publish_trade(&self, trade: TradeData) -> Result<()> {
        info!("🌐 WebSocket: Broadcasting trade for {}", trade.symbol);
        
        if let Some(ref kafka_producer) = self.kafka_producer {
            kafka_producer.publish_trade(&trade).await?;
        }
        
        Ok(())
    }

    pub async fn publish_kline(&self, kline: KlineData) -> Result<()> {
        info!("🌐 WebSocket: Broadcasting kline for {}", kline.symbol);
        
        if let Some(ref kafka_producer) = self.kafka_producer {
            kafka_producer.publish_kline(&kline).await?;
        }
        
        Ok(())
    }
}

// Mock data generators
fn generate_ticker_data(exchange: &str, symbol: &str, base_price: f64) -> TickerData {
    let price_change = (rand::random::<f64>() - 0.5) * base_price * 0.01; // ±1% change
    let current_price = base_price + price_change;
    
    TickerData {
        symbol: symbol.to_string(),
        exchange: exchange.to_string(),
        price: current_price,
        bid: current_price - 0.5,
        ask: current_price + 0.5,
        volume_24h: 1000.0 + rand::random::<f64>() * 5000.0,
        change_24h: (price_change / base_price) * 100.0,
        high_24h: current_price + rand::random::<f64>() * 100.0,
        low_24h: current_price - rand::random::<f64>() * 100.0,
        timestamp: Utc::now().timestamp_millis(),
    }
}

fn generate_orderbook_data(exchange: &str, symbol: &str, mid_price: f64) -> OrderBookData {
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    
    // Generate 5 bid levels
    for i in 0..5 {
        let price = mid_price - (i as f64 * 0.5) - 0.25;
        let quantity = 1.0 + rand::random::<f64>() * 10.0;
        bids.push((price, quantity));
    }
    
    // Generate 5 ask levels
    for i in 0..5 {
        let price = mid_price + (i as f64 * 0.5) + 0.25;
        let quantity = 1.0 + rand::random::<f64>() * 10.0;
        asks.push((price, quantity));
    }
    
    OrderBookData {
        symbol: symbol.to_string(),
        exchange: exchange.to_string(),
        bids,
        asks,
        timestamp: Utc::now().timestamp_millis(),
        sequence_id: Some(rand::random::<u64>()),
    }
}

fn generate_trade_data(exchange: &str, symbol: &str, price: f64) -> TradeData {
    TradeData {
        symbol: symbol.to_string(),
        exchange: exchange.to_string(),
        id: format!("trade_{}", uuid::Uuid::new_v4().simple()),
        price: price + (rand::random::<f64>() - 0.5) * 2.0,
        quantity: rand::random::<f64>() * 0.5 + 0.01,
        side: if rand::random::<bool>() { "buy" } else { "sell" }.to_string(),
        timestamp: Utc::now().timestamp_millis(),
        is_maker: rand::random::<bool>(),
    }
}

fn generate_kline_data(exchange: &str, symbol: &str, open_price: f64) -> KlineData {
    let close = open_price + (rand::random::<f64>() - 0.5) * 20.0;
    let high = f64::max(open_price, close) + rand::random::<f64>() * 10.0;
    let low = f64::min(open_price, close) - rand::random::<f64>() * 10.0;
    
    KlineData {
        symbol: symbol.to_string(),
        exchange: exchange.to_string(),
        interval: "1m".to_string(),
        open_time: Utc::now().timestamp_millis() - 60000,
        close_time: Utc::now().timestamp_millis(),
        open: open_price,
        high,
        low,
        close,
        volume: 50.0 + rand::random::<f64>() * 100.0,
        quote_volume: Some(2500000.0 + rand::random::<f64>() * 500000.0),
        trade_count: Some(100 + rand::random::<u32>() % 200),
    }
}

use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🚀 Starting Kafka Producer Integration Demo");

    // Configuration
    let kafka_brokers = std::env::var("KAFKA_BROKERS")
        .unwrap_or_else(|_| "localhost:9092".to_string());

    info!("📋 Configuration:");
    info!("  Kafka Brokers: {}", kafka_brokers);
    info!("  Demo Duration: 60 seconds");
    info!("  Publishing Interval: 2 seconds");

    // Create Kafka producer
    let kafka_producer = match KafkaProducerDemo::new(kafka_brokers).await {
        Ok(producer) => {
            info!("✅ Kafka producer created successfully");
            Some(Arc::new(producer))
        }
        Err(e) => {
            warn!("❌ Failed to create Kafka producer: {}", e);
            warn!("   Continuing with WebSocket-only mode");
            None
        }
    };

    // Create streaming manager with Kafka producer
    let streaming_manager = StreamingManagerDemo::new_with_kafka(kafka_producer.clone());

    info!("🌊 Starting market data simulation...");

    // Market data simulation
    let symbols = [
        ("binance", "BTC/USDT", 50000.0),
        ("coinbase", "ETH/USD", 3000.0),
        ("bybit", "SOL/USDT", 100.0),
    ];

    let mut ticker_interval = interval(Duration::from_secs(2));
    let mut orderbook_interval = interval(Duration::from_millis(500));
    let mut trade_interval = interval(Duration::from_millis(1000));
    let mut kline_interval = interval(Duration::from_secs(60));
    
    let mut counter = 0;
    let max_iterations = 30; // Run for about 60 seconds

    loop {
        tokio::select! {
            _ = ticker_interval.tick() => {
                for (exchange, symbol, base_price) in &symbols {
                    let ticker = generate_ticker_data(exchange, symbol, *base_price);
                    if let Err(e) = streaming_manager.publish_ticker(ticker).await {
                        warn!("Failed to publish ticker: {}", e);
                    }
                }
            }
            
            _ = orderbook_interval.tick() => {
                let (exchange, symbol, base_price) = symbols[counter % symbols.len()];
                let orderbook = generate_orderbook_data(exchange, symbol, base_price);
                if let Err(e) = streaming_manager.publish_orderbook(orderbook).await {
                    warn!("Failed to publish orderbook: {}", e);
                }
            }
            
            _ = trade_interval.tick() => {
                let (exchange, symbol, base_price) = symbols[counter % symbols.len()];
                let trade = generate_trade_data(exchange, symbol, base_price);
                if let Err(e) = streaming_manager.publish_trade(trade).await {
                    warn!("Failed to publish trade: {}", e);
                }
            }
            
            _ = kline_interval.tick() => {
                for (exchange, symbol, base_price) in &symbols {
                    let kline = generate_kline_data(exchange, symbol, *base_price);
                    if let Err(e) = streaming_manager.publish_kline(kline).await {
                        warn!("Failed to publish kline: {}", e);
                    }
                }
            }
        }

        counter += 1;
        if counter >= max_iterations {
            break;
        }

        // Show metrics every 10 iterations
        if counter % 10 == 0 {
            if let Some(ref producer) = kafka_producer {
                let metrics = producer.get_metrics();
                info!("📊 Metrics: {} sent, {} failed, {:.1}ms avg latency, {:.0} msgs/sec", 
                      metrics.messages_sent, metrics.messages_failed, 
                      metrics.avg_latency_ms, metrics.throughput_msg_per_sec);
            }
        }
    }

    info!("✅ Demo completed successfully!");
    info!("💡 In production, this would:");
    info!("   • Stream real market data from exchange APIs");
    info!("   • Use Protocol Buffer serialization for efficiency");
    info!("   • Maintain connection pools for reliability");
    info!("   • Achieve sub-50ms latency with 10K+ msgs/sec");
    info!("   • Handle failover and error recovery automatically");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kafka_producer_demo() {
        let producer = KafkaProducerDemo::new("localhost:9092".to_string()).await.unwrap();
        
        let ticker = generate_ticker_data("binance", "BTC/USDT", 50000.0);
        assert!(producer.publish_ticker(&ticker).await.is_ok());
        
        let metrics = producer.get_metrics();
        assert!(metrics.avg_latency_ms > 0);
    }
    
    #[tokio::test]
    async fn test_streaming_manager_demo() {
        let producer = Arc::new(KafkaProducerDemo::new("localhost:9092".to_string()).await.unwrap());
        let manager = StreamingManagerDemo::new_with_kafka(Some(producer));
        
        let ticker = generate_ticker_data("coinbase", "ETH/USD", 3000.0);
        assert!(manager.publish_ticker(ticker).await.is_ok());
    }
}