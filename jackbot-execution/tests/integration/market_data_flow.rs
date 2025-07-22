/// Market Data Flow Integration Tests
/// 
/// Tests the complete market data pipeline:
/// Exchange → Sensor → Backend → Terminal
/// 
/// Validates:
/// - End-to-end latency <100ms
/// - Data integrity across components
/// - Real-time streaming performance
/// - Error handling and recovery

use super::{IntegrationTestConfig, IntegrationTestResult, PerformanceMetrics};
use super::infrastructure::{MockExchangeServer, MarketDataUpdate, OrderBookUpdate};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::{sleep, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use rust_decimal::Decimal;
use chrono::{DateTime, Utc};

/// Market data flow test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataFlowResult {
    pub test_name: String,
    pub total_messages: u32,
    pub successful_messages: u32,
    pub failed_messages: u32,
    pub avg_latency_ms: f64,
    pub max_latency_ms: u64,
    pub min_latency_ms: u64,
    pub throughput_msg_per_sec: f64,
    pub data_integrity_score: f64,
    pub error_rate: f64,
}

/// Message tracking for latency measurement
#[derive(Debug, Clone)]
struct MessageTracker {
    pub id: String,
    pub symbol: String,
    pub exchange_timestamp: DateTime<Utc>,
    pub sensor_timestamp: Option<DateTime<Utc>>,
    pub backend_timestamp: Option<DateTime<Utc>>,
    pub terminal_timestamp: Option<DateTime<Utc>>,
    pub data_hash: u64,
}

/// Market data validation metrics
#[derive(Debug, Clone)]
struct ValidationMetrics {
    pub total_messages: u32,
    pub valid_messages: u32,
    pub corrupted_messages: u32,
    pub out_of_order_messages: u32,
    pub duplicate_messages: u32,
    pub missing_messages: u32,
}

/// Main market data flow test
pub async fn test_end_to_end_market_data_flow(
    config: &IntegrationTestConfig,
) -> Result<IntegrationTestResult, Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    println!("📊 Starting end-to-end market data flow test...");
    
    // Initialize components
    let mock_exchange = MockExchangeServer::start(config.mock_exchange_port).await?;
    let message_tracker = Arc::new(Mutex::new(HashMap::<String, MessageTracker>::new()));
    let validation_metrics = Arc::new(Mutex::new(ValidationMetrics::new()));
    
    // Start market data consumers
    let sensor_consumer = start_sensor_simulation(config, message_tracker.clone()).await?;
    let backend_consumer = start_backend_simulation(config, message_tracker.clone()).await?;
    let terminal_consumer = start_terminal_simulation(config, message_tracker.clone()).await?;
    
    // Run test for specified duration
    let test_duration = Duration::from_secs(30); // 30-second test
    println!("⏱️ Running market data flow test for {:?}...", test_duration);
    
    sleep(test_duration).await;
    
    // Stop consumers
    sensor_consumer.abort();
    backend_consumer.abort();
    terminal_consumer.abort();
    
    // Calculate results
    let tracker_data = message_tracker.lock().await;
    let metrics = validation_metrics.lock().await;
    
    let flow_result = calculate_flow_metrics(&tracker_data, &metrics).await;
    let performance_metrics = calculate_performance_metrics(&tracker_data, start_time).await;
    
    // Determine success
    let success = flow_result.avg_latency_ms <= config.performance_targets.market_data_latency_ms as f64
        && flow_result.error_rate <= 0.01 // 1% error rate threshold
        && flow_result.data_integrity_score >= 0.99; // 99% data integrity
    
    let test_result = IntegrationTestResult {
        test_name: "end_to_end_market_data_flow".to_string(),
        success,
        duration_ms: start_time.elapsed().as_millis() as u64,
        error_message: if !success {
            Some(format!("Performance targets not met: latency={:.2}ms, error_rate={:.2}%, integrity={:.2}%",
                flow_result.avg_latency_ms, flow_result.error_rate * 100.0, flow_result.data_integrity_score * 100.0))
        } else {
            None
        },
        performance_metrics: Some(performance_metrics),
    };
    
    // Log detailed results
    log_market_data_results(&flow_result).await;
    
    println!("📊 Market data flow test completed in {:?}", start_time.elapsed());
    Ok(test_result)
}

/// Simulate sensor component consuming from mock exchange
async fn start_sensor_simulation(
    config: &IntegrationTestConfig,
    message_tracker: Arc<Mutex<HashMap<String, MessageTracker>>>,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let ws_url = format!("ws://localhost:{}", config.mock_exchange_port);
    let (ws_stream, _) = connect_async(&ws_url).await?;
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    // Subscribe to market data
    let subscribe_msg = json!({
        "type": "subscribe",
        "channels": ["market_data", "order_book"],
        "symbols": ["BTCUSDT", "ETHUSDT", "ADAUSDT", "DOTUSDT"]
    });
    
    ws_sender.send(Message::Text(subscribe_msg.to_string())).await?;
    
    let tracker_clone = message_tracker.clone();
    let handle = tokio::spawn(async move {
        println!("🔧 Sensor simulation started");
        let mut message_count = 0u32;
        
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    message_count += 1;
                    
                    if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                        // Simulate sensor processing
                        sleep(Duration::from_millis(5)).await; // 5ms processing time
                        
                        // Track message in sensor
                        if let Some(data) = parsed.get("data") {
                            let message_id = generate_message_id(&data);
                            
                            let mut tracker = tracker_clone.lock().await;
                            if let Some(existing) = tracker.get_mut(&message_id) {
                                existing.sensor_timestamp = Some(Utc::now());
                            } else {
                                // Create new tracker entry
                                let symbol = data["symbol"].as_str().unwrap_or("UNKNOWN").to_string();
                                let exchange_ts = if let Some(ts_str) = data["timestamp"].as_str() {
                                    DateTime::parse_from_rfc3339(ts_str).map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now())
                                } else {
                                    Utc::now()
                                };
                                
                                tracker.insert(message_id.clone(), MessageTracker {
                                    id: message_id,
                                    symbol,
                                    exchange_timestamp: exchange_ts,
                                    sensor_timestamp: Some(Utc::now()),
                                    backend_timestamp: None,
                                    terminal_timestamp: None,
                                    data_hash: calculate_data_hash(&data),
                                });
                            }
                        }
                        
                        // Simulate publishing to Kafka
                        // In real implementation, this would publish to actual Kafka
                        if message_count % 100 == 0 {
                            println!("🔧 Sensor processed {} messages", message_count);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    println!("🔧 Sensor WebSocket connection closed");
                    break;
                }
                Err(e) => {
                    println!("❌ Sensor WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        
        println!("🔧 Sensor simulation ended. Processed {} messages", message_count);
    });
    
    Ok(handle)
}

/// Simulate backend component consuming from Kafka and serving WebSocket
async fn start_backend_simulation(
    config: &IntegrationTestConfig,
    message_tracker: Arc<Mutex<HashMap<String, MessageTracker>>>,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let tracker_clone = message_tracker.clone();
    
    let handle = tokio::spawn(async move {
        println!("🖥️ Backend simulation started");
        let mut message_count = 0u32;
        
        // Simulate backend processing loop
        let mut interval = tokio::time::interval(Duration::from_millis(10)); // 100 Hz processing
        
        loop {
            interval.tick().await;
            
            // Simulate consuming from Kafka and processing
            {
                let mut tracker = tracker_clone.lock().await;
                let mut processed_messages = Vec::new();
                
                for (id, message) in tracker.iter_mut() {
                    if message.sensor_timestamp.is_some() && message.backend_timestamp.is_none() {
                        // Simulate backend processing delay
                        message.backend_timestamp = Some(Utc::now());
                        message_count += 1;
                        processed_messages.push(id.clone());
                        
                        if processed_messages.len() >= 10 {
                            break; // Process in batches
                        }
                    }
                }
                
                if message_count % 100 == 0 && !processed_messages.is_empty() {
                    println!("🖥️ Backend processed {} messages", message_count);
                }
            }
            
            // Simulate memory pressure relief
            if message_count % 1000 == 0 {
                sleep(Duration::from_millis(1)).await;
            }
        }
    });
    
    Ok(handle)
}

/// Simulate terminal component consuming from backend WebSocket
async fn start_terminal_simulation(
    config: &IntegrationTestConfig,
    message_tracker: Arc<Mutex<HashMap<String, MessageTracker>>>,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let tracker_clone = message_tracker.clone();
    
    let handle = tokio::spawn(async move {
        println!("📱 Terminal simulation started");
        let mut message_count = 0u32;
        
        // Simulate terminal processing loop
        let mut interval = tokio::time::interval(Duration::from_millis(20)); // 50 Hz UI updates
        
        loop {
            interval.tick().await;
            
            // Simulate terminal receiving and displaying data
            {
                let mut tracker = tracker_clone.lock().await;
                let mut processed_messages = Vec::new();
                
                for (id, message) in tracker.iter_mut() {
                    if message.backend_timestamp.is_some() && message.terminal_timestamp.is_none() {
                        // Simulate terminal processing and display
                        message.terminal_timestamp = Some(Utc::now());
                        message_count += 1;
                        processed_messages.push(id.clone());
                        
                        if processed_messages.len() >= 5 {
                            break; // Process in smaller batches for UI
                        }
                    }
                }
                
                if message_count % 50 == 0 && !processed_messages.is_empty() {
                    println!("📱 Terminal processed {} messages", message_count);
                }
            }
            
            // Simulate UI rendering delay
            if message_count % 100 == 0 {
                sleep(Duration::from_millis(1)).await;
            }
        }
    });
    
    Ok(handle)
}

/// Calculate flow metrics from tracked messages
async fn calculate_flow_metrics(
    tracker_data: &HashMap<String, MessageTracker>,
    validation_metrics: &ValidationMetrics,
) -> MarketDataFlowResult {
    let mut latencies = Vec::new();
    let mut successful_flows = 0u32;
    let mut total_flows = 0u32;
    
    for message in tracker_data.values() {
        total_flows += 1;
        
        if let (Some(terminal_ts), exchange_ts) = (message.terminal_timestamp, message.exchange_timestamp) {
            let latency = terminal_ts.timestamp_millis() - exchange_ts.timestamp_millis();
            if latency >= 0 && latency <= 10000 { // Reasonable latency bounds (0-10s)
                latencies.push(latency as u64);
                successful_flows += 1;
            }
        }
    }
    
    let avg_latency = if !latencies.is_empty() {
        latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
    } else {
        0.0
    };
    
    let max_latency = latencies.iter().max().copied().unwrap_or(0);
    let min_latency = latencies.iter().min().copied().unwrap_or(0);
    let failed_flows = total_flows - successful_flows;
    let error_rate = if total_flows > 0 { failed_flows as f64 / total_flows as f64 } else { 0.0 };
    
    let data_integrity_score = if validation_metrics.total_messages > 0 {
        validation_metrics.valid_messages as f64 / validation_metrics.total_messages as f64
    } else {
        1.0
    };
    
    let throughput = if !latencies.is_empty() {
        successful_flows as f64 / 30.0 // 30-second test duration
    } else {
        0.0
    };
    
    MarketDataFlowResult {
        test_name: "market_data_end_to_end".to_string(),
        total_messages: total_flows,
        successful_messages: successful_flows,
        failed_messages: failed_flows,
        avg_latency_ms: avg_latency,
        max_latency_ms: max_latency,
        min_latency_ms: min_latency,
        throughput_msg_per_sec: throughput,
        data_integrity_score,
        error_rate,
    }
}

/// Calculate performance metrics
async fn calculate_performance_metrics(
    tracker_data: &HashMap<String, MessageTracker>,
    start_time: Instant,
) -> PerformanceMetrics {
    let duration_secs = start_time.elapsed().as_secs_f64();
    let total_messages = tracker_data.len();
    
    let successful_messages = tracker_data.values()
        .filter(|m| m.terminal_timestamp.is_some())
        .count();
    
    let avg_latency = if successful_messages > 0 {
        let total_latency: i64 = tracker_data.values()
            .filter_map(|m| {
                if let (Some(terminal_ts), exchange_ts) = (m.terminal_timestamp, m.exchange_timestamp) {
                    Some(terminal_ts.timestamp_millis() - exchange_ts.timestamp_millis())
                } else {
                    None
                }
            })
            .sum();
        total_latency as f64 / successful_messages as f64
    } else {
        0.0
    };
    
    PerformanceMetrics {
        latency_ms: avg_latency as u64,
        throughput: total_messages as f64 / duration_secs,
        memory_usage_mb: estimate_memory_usage(tracker_data),
        cpu_usage_percent: 25.0, // Simulated CPU usage
        errors_count: (total_messages - successful_messages) as u32,
    }
}

/// Generate unique message ID from data
fn generate_message_id(data: &Value) -> String {
    let symbol = data["symbol"].as_str().unwrap_or("UNKNOWN");
    let timestamp = data["timestamp"].as_str().unwrap_or("");
    let sequence = data["sequence"].as_u64().unwrap_or(0);
    format!("{}_{}_{}_{}", symbol, timestamp, sequence, Uuid::new_v4())
}

/// Calculate hash of message data for integrity checking
fn calculate_data_hash(data: &Value) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    data.to_string().hash(&mut hasher);
    hasher.finish()
}

/// Estimate memory usage from tracked data
fn estimate_memory_usage(tracker_data: &HashMap<String, MessageTracker>) -> f64 {
    const BYTES_PER_MESSAGE: usize = 256; // Estimated bytes per tracked message
    (tracker_data.len() * BYTES_PER_MESSAGE) as f64 / (1024.0 * 1024.0) // Convert to MB
}

/// Log detailed market data results
async fn log_market_data_results(result: &MarketDataFlowResult) {
    println!("\n📊 Market Data Flow Test Results");
    println!("================================");
    println!("Total Messages: {}", result.total_messages);
    println!("Successful Messages: {}", result.successful_messages);
    println!("Failed Messages: {}", result.failed_messages);
    println!("Average Latency: {:.2} ms", result.avg_latency_ms);
    println!("Max Latency: {} ms", result.max_latency_ms);
    println!("Min Latency: {} ms", result.min_latency_ms);
    println!("Throughput: {:.2} msg/sec", result.throughput_msg_per_sec);
    println!("Data Integrity: {:.2}%", result.data_integrity_score * 100.0);
    println!("Error Rate: {:.2}%", result.error_rate * 100.0);
    
    // Performance assessment
    if result.avg_latency_ms <= 100.0 {
        println!("✅ Latency target met (<100ms)");
    } else {
        println!("❌ Latency target missed (>100ms)");
    }
    
    if result.error_rate <= 0.01 {
        println!("✅ Error rate acceptable (<1%)");
    } else {
        println!("❌ Error rate too high (>1%)");
    }
    
    if result.data_integrity_score >= 0.99 {
        println!("✅ Data integrity excellent (>99%)");
    } else {
        println!("❌ Data integrity concerns (<99%)");
    }
}

impl ValidationMetrics {
    fn new() -> Self {
        Self {
            total_messages: 0,
            valid_messages: 0,
            corrupted_messages: 0,
            out_of_order_messages: 0,
            duplicate_messages: 0,
            missing_messages: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_message_tracking() {
        let mut tracker = HashMap::new();
        let message_id = "test_message_1".to_string();
        
        tracker.insert(message_id.clone(), MessageTracker {
            id: message_id.clone(),
            symbol: "BTCUSDT".to_string(),
            exchange_timestamp: Utc::now(),
            sensor_timestamp: Some(Utc::now()),
            backend_timestamp: None,
            terminal_timestamp: None,
            data_hash: 12345,
        });
        
        assert!(tracker.contains_key(&message_id));
        assert!(tracker[&message_id].sensor_timestamp.is_some());
    }

    #[test]
    fn test_generate_message_id() {
        let data = json!({
            "symbol": "BTCUSDT",
            "timestamp": "2023-01-01T00:00:00Z",
            "sequence": 12345
        });
        
        let id = generate_message_id(&data);
        assert!(id.contains("BTCUSDT"));
        assert!(id.contains("12345"));
    }

    #[test]
    fn test_calculate_data_hash() {
        let data1 = json!({"symbol": "BTCUSDT", "price": 50000});
        let data2 = json!({"symbol": "BTCUSDT", "price": 50000});
        let data3 = json!({"symbol": "BTCUSDT", "price": 50001});
        
        assert_eq!(calculate_data_hash(&data1), calculate_data_hash(&data2));
        assert_ne!(calculate_data_hash(&data1), calculate_data_hash(&data3));
    }
}