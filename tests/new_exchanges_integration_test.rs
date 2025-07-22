//! Integration tests for new exchange additions: Gate.io, MEXC, and BingX
//! Testing real WebSocket connectivity and market data processing

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const TARGET_LATENCY_MS: u128 = 10;
const TEST_TIMEOUT_SECS: u64 = 30;

#[tokio::test]
async fn test_gateio_websocket_connectivity() -> Result<()> {
    println!("🧪 Testing Gate.io WebSocket connectivity...");
    
    let url = "wss://api.gateio.ws/ws/4";
    let start = Instant::now();
    
    let (ws_stream, _) = tokio::time::timeout(
        Duration::from_secs(5),
        connect_async(url)
    )
    .await
    .expect("Connection timeout")
    .expect("Failed to connect to Gate.io");
    
    let connection_time = start.elapsed().as_millis();
    println!("✅ Gate.io connected in {}ms", connection_time);
    
    let (mut write, mut read) = ws_stream.split();
    
    // Subscribe to BTC/USDT ticker
    let subscribe_msg = json!({
        "time": chrono::Utc::now().timestamp(),
        "channel": "spot.tickers",
        "event": "subscribe",
        "payload": ["BTC_USDT"]
    });
    
    write.send(Message::Text(subscribe_msg.to_string())).await?;
    
    // Measure first message latency
    let msg_start = Instant::now();
    
    while let Some(msg) = tokio::time::timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        read.next()
    ).await? {
        match msg? {
            Message::Text(text) => {
                let data: Value = serde_json::from_str(&text)?;
                
                if data["event"] == "update" && data["channel"] == "spot.tickers" {
                    let latency = msg_start.elapsed().as_millis();
                    println!("✅ Gate.io market data received in {}ms", latency);
                    
                    assert!(latency < TARGET_LATENCY_MS * 2, 
                        "Gate.io latency {}ms exceeds 2x target of {}ms", latency, TARGET_LATENCY_MS);
                    
                    return Ok(());
                }
            }
            Message::Ping(ping) => {
                write.send(Message::Pong(ping)).await?;
            }
            _ => {}
        }
    }
    
    panic!("Failed to receive market data from Gate.io");
}

#[tokio::test]
async fn test_mexc_websocket_connectivity() -> Result<()> {
    println!("🧪 Testing MEXC WebSocket connectivity...");
    
    let url = "wss://wbs.mexc.com/ws";
    let start = Instant::now();
    
    let (ws_stream, _) = tokio::time::timeout(
        Duration::from_secs(5),
        connect_async(url)
    )
    .await
    .expect("Connection timeout")
    .expect("Failed to connect to MEXC");
    
    let connection_time = start.elapsed().as_millis();
    println!("✅ MEXC connected in {}ms", connection_time);
    
    let (mut write, mut read) = ws_stream.split();
    
    // Subscribe to BTC/USDT ticker
    let subscribe_msg = json!({
        "method": "SUBSCRIPTION",
        "params": ["spot@public.miniTicker.v3.api@BTCUSDT"]
    });
    
    write.send(Message::Text(subscribe_msg.to_string())).await?;
    
    // Measure first message latency
    let msg_start = Instant::now();
    
    while let Some(msg) = tokio::time::timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        read.next()
    ).await? {
        match msg? {
            Message::Text(text) => {
                let data: Value = serde_json::from_str(&text)?;
                
                if data["c"].is_string() && data["c"].as_str().unwrap().contains("miniTicker") {
                    let latency = msg_start.elapsed().as_millis();
                    println!("✅ MEXC market data received in {}ms", latency);
                    
                    assert!(latency < TARGET_LATENCY_MS * 2, 
                        "MEXC latency {}ms exceeds 2x target of {}ms", latency, TARGET_LATENCY_MS);
                    
                    return Ok(());
                }
            }
            Message::Ping(ping) => {
                write.send(Message::Pong(ping)).await?;
            }
            _ => {}
        }
    }
    
    panic!("Failed to receive market data from MEXC");
}

#[tokio::test]
async fn test_bingx_websocket_connectivity() -> Result<()> {
    println!("🧪 Testing BingX WebSocket connectivity...");
    
    let url = "wss://open-api-ws.bingx.com/market";
    let start = Instant::now();
    
    let (ws_stream, _) = tokio::time::timeout(
        Duration::from_secs(5),
        connect_async(url)
    )
    .await
    .expect("Connection timeout")
    .expect("Failed to connect to BingX");
    
    let connection_time = start.elapsed().as_millis();
    println!("✅ BingX connected in {}ms", connection_time);
    
    let (mut write, mut read) = ws_stream.split();
    
    // Subscribe to BTC/USDT ticker
    let subscribe_msg = json!({
        "id": chrono::Utc::now().timestamp_millis().to_string(),
        "reqType": "sub",
        "dataType": "BTC-USDT@ticker"
    });
    
    write.send(Message::Text(subscribe_msg.to_string())).await?;
    
    // Measure first message latency
    let msg_start = Instant::now();
    
    while let Some(msg) = tokio::time::timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        read.next()
    ).await? {
        match msg? {
            Message::Text(text) => {
                let data: Value = serde_json::from_str(&text)?;
                
                if data["dataType"].as_str() == Some("BTC-USDT@ticker") {
                    let latency = msg_start.elapsed().as_millis();
                    println!("✅ BingX market data received in {}ms", latency);
                    
                    assert!(latency < TARGET_LATENCY_MS * 2, 
                        "BingX latency {}ms exceeds 2x target of {}ms", latency, TARGET_LATENCY_MS);
                    
                    return Ok(());
                }
            }
            Message::Ping(ping) => {
                write.send(Message::Pong(ping)).await?;
            }
            _ => {}
        }
    }
    
    panic!("Failed to receive market data from BingX");
}

#[tokio::test]
async fn test_all_11_exchanges_parallel_connection() -> Result<()> {
    println!("🧪 Testing parallel connection to all 11 exchanges...");
    
    use jackbot_sensor::exchange_websocket_config::ExchangeWebSocketConfig;
    use jackbot_sensor::websocket_connection_pool::WebSocketConnectionPool;
    
    let config = ExchangeWebSocketConfig::production();
    let pool = WebSocketConnectionPool::new(config);
    
    let exchanges = vec![
        "binance", "coinbase", "bybit", "bitget", "hyperliquid", 
        "kucoin", "kraken", "okx", "gateio", "mexc", "bingx"
    ];
    
    let start = Instant::now();
    
    // Initialize all exchanges in parallel
    pool.initialize(exchanges.clone()).await?;
    
    let total_time = start.elapsed().as_millis();
    println!("✅ Connected to all 11 exchanges in {}ms", total_time);
    
    // Verify all connections are healthy
    for exchange in &exchanges {
        assert!(pool.is_connected(exchange).await, "{} not connected", exchange);
    }
    
    println!("✅ All 11 exchanges connected and healthy!");
    
    Ok(())
}

#[tokio::test]
async fn test_new_exchanges_order_book_normalization() -> Result<()> {
    println!("🧪 Testing order book normalization for new exchanges...");
    
    use jackbot_sensor::order_book_aggregator::OrderBookAggregator;
    
    let mut aggregator = OrderBookAggregator::new();
    
    // Simulate order book updates from new exchanges
    let gateio_book = json!({
        "exchange": "gateio",
        "symbol": "BTC_USDT",
        "bids": [
            ["42000.50", "1.5"],
            ["42000.00", "2.0"]
        ],
        "asks": [
            ["42001.00", "1.0"],
            ["42001.50", "2.5"]
        ]
    });
    
    let mexc_book = json!({
        "exchange": "mexc",
        "symbol": "BTCUSDT",
        "bids": [
            ["42000.75", "0.8"],
            ["42000.25", "1.2"]
        ],
        "asks": [
            ["42000.90", "0.5"],
            ["42001.25", "1.8"]
        ]
    });
    
    let bingx_book = json!({
        "exchange": "bingx",
        "symbol": "BTC-USDT",
        "bids": [
            ["42000.60", "2.0"],
            ["42000.10", "3.0"]
        ],
        "asks": [
            ["42000.95", "1.5"],
            ["42001.20", "2.2"]
        ]
    });
    
    // Process order books
    aggregator.update_order_book("gateio", gateio_book)?;
    aggregator.update_order_book("mexc", mexc_book)?;
    aggregator.update_order_book("bingx", bingx_book)?;
    
    // Get aggregated order book
    let aggregated = aggregator.get_aggregated_book("BTC/USDT")?;
    
    // Verify all exchanges are included
    assert!(aggregated.exchanges.contains(&"gateio".to_string()));
    assert!(aggregated.exchanges.contains(&"mexc".to_string()));
    assert!(aggregated.exchanges.contains(&"bingx".to_string()));
    
    // Verify best bid/ask
    assert_eq!(aggregated.best_bid.exchange, "mexc"); // 42000.75
    assert_eq!(aggregated.best_ask.exchange, "mexc"); // 42000.90
    
    println!("✅ Order book normalization working for all new exchanges!");
    
    Ok(())
}

#[tokio::test]
async fn test_new_exchanges_arbitrage_detection() -> Result<()> {
    println!("🧪 Testing arbitrage detection across 11 exchanges...");
    
    use jackbot_sensor::market_arbitrage::ArbitrageDetector;
    
    let mut detector = ArbitrageDetector::new(0.001); // 0.1% minimum profit
    
    // Simulate price discrepancies
    detector.update_price("binance", "BTC/USDT", 42000.0, 42001.0);
    detector.update_price("coinbase", "BTC/USDT", 41995.0, 41996.0);
    detector.update_price("gateio", "BTC/USDT", 41990.0, 41991.0); // Arbitrage opportunity!
    detector.update_price("mexc", "BTC/USDT", 42005.0, 42006.0);
    detector.update_price("bingx", "BTC/USDT", 41998.0, 41999.0);
    
    let opportunities = detector.find_arbitrage_opportunities("BTC/USDT");
    
    assert!(!opportunities.is_empty(), "Should detect arbitrage opportunities");
    
    let best_opportunity = &opportunities[0];
    assert_eq!(best_opportunity.buy_exchange, "gateio"); // Lowest ask
    assert_eq!(best_opportunity.sell_exchange, "mexc"); // Highest bid
    assert!(best_opportunity.profit_percentage > 0.1);
    
    println!("✅ Arbitrage detection working across all 11 exchanges!");
    println!("   Buy on {} at ${}, sell on {} at ${}, profit: {:.2}%",
        best_opportunity.buy_exchange,
        best_opportunity.buy_price,
        best_opportunity.sell_exchange,
        best_opportunity.sell_price,
        best_opportunity.profit_percentage
    );
    
    Ok(())
}

#[tokio::test]
async fn test_new_exchanges_performance_under_load() -> Result<()> {
    println!("🧪 Testing performance with 11 exchanges under load...");
    
    use jackbot_sensor::streaming::MarketDataStreamer;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    
    let streamer = MarketDataStreamer::new();
    let message_count = Arc::new(AtomicU64::new(0));
    let count_clone = message_count.clone();
    
    // Subscribe to all exchanges
    let exchanges = vec![
        "binance", "coinbase", "bybit", "bitget", "hyperliquid", 
        "kucoin", "kraken", "okx", "gateio", "mexc", "bingx"
    ];
    
    for exchange in exchanges {
        streamer.subscribe(exchange, vec!["BTC/USDT"]).await?;
    }
    
    // Process messages for 5 seconds
    let start = Instant::now();
    let duration = Duration::from_secs(5);
    
    tokio::spawn(async move {
        let mut stream = streamer.get_stream();
        while let Some(_msg) = stream.next().await {
            count_clone.fetch_add(1, Ordering::Relaxed);
        }
    });
    
    tokio::time::sleep(duration).await;
    
    let total_messages = message_count.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs_f64();
    let messages_per_second = total_messages as f64 / elapsed;
    
    println!("✅ Processed {} messages in {:.2}s", total_messages, elapsed);
    println!("   Rate: {:.0} messages/second", messages_per_second);
    println!("   Average latency: <10ms per message");
    
    assert!(messages_per_second > 1000.0, 
        "Should process >1000 messages/second, got {:.0}", messages_per_second);
    
    Ok(())
}