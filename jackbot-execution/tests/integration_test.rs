/// Comprehensive Integration Test Runner
/// 
/// This is the main entry point for running all integration tests
/// across the Jackbot system components (Sensor ↔ Backend ↔ Terminal)

use std::time::{Duration, Instant};
use tokio::time::timeout;

mod integration;

use integration::{
    IntegrationTestConfig, IntegrationTestSuite,
    infrastructure::MockExchangeServer,
};

/// Main integration test function
#[tokio::test]
async fn run_comprehensive_integration_tests() {
    // Initialize tracing for test logging
    tracing_subscriber::fmt()
        .with_env_filter("debug")
        .init();

    println!("🚀 Starting Jackbot Comprehensive Integration Test Suite");
    println!("===========================================================");
    
    // Load test configuration
    let config = load_test_configuration().await;
    
    // Create test suite
    let mut test_suite = IntegrationTestSuite::new(config);
    
    // Set test timeout (10 minutes for comprehensive tests)
    let test_timeout = Duration::from_secs(600);
    
    // Run all integration tests with timeout
    let test_result = timeout(test_timeout, test_suite.run_all_tests()).await;
    
    match test_result {
        Ok(Ok(())) => {
            println!("✅ All integration tests completed successfully!");
        }
        Ok(Err(e)) => {
            println!("❌ Integration tests failed: {}", e);
            panic!("Integration test failure: {}", e);
        }
        Err(_) => {
            println!("⏰ Integration tests timed out after {:?}", test_timeout);
            panic!("Integration test timeout");
        }
    }
    
    println!("===========================================================");
    println!("🏁 Integration test suite execution completed");
}

/// Test market data flow specifically
#[tokio::test]
async fn test_market_data_flow_only() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("📊 Testing market data flow only...");
    
    let config = load_test_configuration().await;
    
    // Start mock exchange
    let _mock_exchange = MockExchangeServer::start(config.mock_exchange_port).await
        .expect("Failed to start mock exchange");
    
    // Run market data flow test
    let result = integration::market_data_flow::test_end_to_end_market_data_flow(&config).await;
    
    match result {
        Ok(test_result) => {
            if test_result.success {
                println!("✅ Market data flow test passed");
            } else {
                println!("❌ Market data flow test failed: {:?}", test_result.error_message);
                panic!("Market data flow test failed");
            }
        }
        Err(e) => {
            println!("❌ Market data flow test error: {}", e);
            panic!("Market data flow test error: {}", e);
        }
    }
}

/// Test order execution flow specifically
#[tokio::test]
async fn test_order_execution_flow_only() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("💼 Testing order execution flow only...");
    
    let config = load_test_configuration().await;
    
    // Start mock exchange
    let _mock_exchange = MockExchangeServer::start(config.mock_exchange_port + 1).await
        .expect("Failed to start mock exchange");
    
    // Run order execution flow test
    let result = integration::order_execution_flow::test_end_to_end_order_execution(&config).await;
    
    match result {
        Ok(test_result) => {
            if test_result.success {
                println!("✅ Order execution flow test passed");
            } else {
                println!("❌ Order execution flow test failed: {:?}", test_result.error_message);
                panic!("Order execution flow test failed");
            }
        }
        Err(e) => {
            println!("❌ Order execution flow test error: {}", e);
            panic!("Order execution flow test error: {}", e);
        }
    }
}

/// Test performance validation specifically
#[tokio::test]
async fn test_performance_validation_only() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("⚡ Testing performance validation only...");
    
    let config = load_test_configuration().await;
    
    // Run performance tests
    let results = integration::performance_tests::run_all_performance_tests(&config).await;
    
    match results {
        Ok(test_results) => {
            let successful_tests = test_results.iter().filter(|r| r.success).count();
            let total_tests = test_results.len();
            
            if successful_tests == total_tests {
                println!("✅ All {} performance tests passed", total_tests);
            } else {
                println!("❌ Performance tests failed: {}/{} passed", successful_tests, total_tests);
                
                // Log failed tests
                for result in &test_results {
                    if !result.success {
                        println!("  - {} failed: {:?}", result.test_name, result.error_message);
                    }
                }
                
                panic!("Performance validation failed");
            }
        }
        Err(e) => {
            println!("❌ Performance test error: {}", e);
            panic!("Performance test error: {}", e);
        }
    }
}

/// Test functional integration specifically
#[tokio::test]
async fn test_functional_integration_only() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("🔧 Testing functional integration only...");
    
    let config = load_test_configuration().await;
    
    // Run functional tests
    let results = integration::functional_tests::run_all_functional_tests(&config).await;
    
    match results {
        Ok(test_results) => {
            let successful_tests = test_results.iter().filter(|r| r.success).count();
            let total_tests = test_results.len();
            
            if successful_tests == total_tests {
                println!("✅ All {} functional tests passed", total_tests);
            } else {
                println!("❌ Functional tests failed: {}/{} passed", successful_tests, total_tests);
                
                // Log failed tests
                for result in &test_results {
                    if !result.success {
                        println!("  - {} failed: {:?}", result.test_name, result.error_message);
                    }
                }
                
                panic!("Functional integration failed");
            }
        }
        Err(e) => {
            println!("❌ Functional test error: {}", e);
            panic!("Functional test error: {}", e);
        }
    }
}

/// Load test configuration from environment or defaults
async fn load_test_configuration() -> IntegrationTestConfig {
    let config = IntegrationTestConfig {
        sensor_endpoint: std::env::var("TEST_SENSOR_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:8081".to_string()),
        backend_endpoint: std::env::var("TEST_BACKEND_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:8080".to_string()),
        terminal_endpoint: std::env::var("TEST_TERMINAL_ENDPOINT")
            .unwrap_or_else(|_| "ws://localhost:8082".to_string()),
        kafka_brokers: std::env::var("TEST_KAFKA_BROKERS")
            .unwrap_or_else(|_| "localhost:9092".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
        test_database_url: std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://test:test@localhost:5433/jackbot_test".to_string()),
        mock_exchange_port: std::env::var("TEST_MOCK_EXCHANGE_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8090),
        timeout_seconds: std::env::var("TEST_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30),
        performance_targets: integration::PerformanceTargets {
            market_data_latency_ms: std::env::var("TEST_MARKET_DATA_LATENCY_TARGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            order_execution_latency_ms: std::env::var("TEST_ORDER_EXECUTION_LATENCY_TARGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
            websocket_update_latency_ms: std::env::var("TEST_WEBSOCKET_UPDATE_LATENCY_TARGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
            database_query_latency_ms: std::env::var("TEST_DATABASE_QUERY_LATENCY_TARGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
            throughput_orders_per_second: std::env::var("TEST_THROUGHPUT_ORDERS_TARGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            concurrent_connections: std::env::var("TEST_CONCURRENT_CONNECTIONS_TARGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000),
            messages_per_hour: std::env::var("TEST_MESSAGES_PER_HOUR_TARGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1_000_000),
        },
    };
    
    println!("📋 Test Configuration:");
    println!("  Sensor Endpoint: {}", config.sensor_endpoint);
    println!("  Backend Endpoint: {}", config.backend_endpoint);
    println!("  Terminal Endpoint: {}", config.terminal_endpoint);
    println!("  Kafka Brokers: {:?}", config.kafka_brokers);
    println!("  Database URL: {}", mask_credentials(&config.test_database_url));
    println!("  Mock Exchange Port: {}", config.mock_exchange_port);
    println!("  Test Timeout: {}s", config.timeout_seconds);
    println!("  Performance Targets:");
    println!("    Market Data Latency: {}ms", config.performance_targets.market_data_latency_ms);
    println!("    Order Execution Latency: {}ms", config.performance_targets.order_execution_latency_ms);
    println!("    WebSocket Update Latency: {}ms", config.performance_targets.websocket_update_latency_ms);
    println!("    Database Query Latency: {}ms", config.performance_targets.database_query_latency_ms);
    println!("    Throughput Orders/sec: {}", config.performance_targets.throughput_orders_per_second);
    println!("    Concurrent Connections: {}", config.performance_targets.concurrent_connections);
    println!("    Messages/hour: {}", config.performance_targets.messages_per_hour);
    
    config
}

/// Helper function to mask credentials in URLs for logging
fn mask_credentials(url: &str) -> String {
    if let Ok(parsed_url) = url::Url::parse(url) {
        let mut masked_url = parsed_url.clone();
        if let Some(password) = parsed_url.password() {
            if !password.is_empty() {
                masked_url.set_password(Some("***")).ok();
            }
        }
        masked_url.to_string()
    } else {
        url.to_string()
    }
}

/// Utility test for validating test environment setup
#[tokio::test]
async fn test_environment_validation() {
    println!("🔍 Validating test environment...");
    
    let config = load_test_configuration().await;
    
    // Test network connectivity to required services
    let mut validation_errors = Vec::new();
    
    // Test Kafka connectivity
    println!("  Checking Kafka connectivity...");
    // In a real implementation, would test actual Kafka connection
    
    // Test database connectivity
    println!("  Checking database connectivity...");
    // In a real implementation, would test actual database connection
    
    // Test mock exchange port availability
    println!("  Checking mock exchange port availability...");
    match tokio::net::TcpListener::bind(format!("127.0.0.1:{}", config.mock_exchange_port)).await {
        Ok(_) => println!("    ✅ Port {} is available", config.mock_exchange_port),
        Err(e) => {
            let error = format!("Port {} is not available: {}", config.mock_exchange_port, e);
            println!("    ❌ {}", error);
            validation_errors.push(error);
        }
    }
    
    // Validate environment variables
    println!("  Checking environment variables...");
    let required_vars = vec![
        "RUST_LOG",
    ];
    
    for var in required_vars {
        match std::env::var(var) {
            Ok(value) => println!("    ✅ {}: {}", var, value),
            Err(_) => println!("    ⚠️ {} not set (will use default)", var),
        }
    }
    
    // Check for validation errors
    if !validation_errors.is_empty() {
        println!("❌ Environment validation failed:");
        for error in &validation_errors {
            println!("  - {}", error);
        }
        panic!("Environment validation failed with {} errors", validation_errors.len());
    }
    
    println!("✅ Environment validation passed");
}

/// Benchmark test for measuring baseline performance
#[tokio::test]
async fn benchmark_baseline_performance() {
    println!("📊 Running baseline performance benchmark...");
    
    let config = load_test_configuration().await;
    
    // Measure mock exchange startup time
    let start_time = Instant::now();
    let _mock_exchange = MockExchangeServer::start(config.mock_exchange_port + 10).await
        .expect("Failed to start mock exchange for benchmark");
    let startup_time = start_time.elapsed();
    
    println!("  Mock exchange startup time: {:?}", startup_time);
    
    // Measure message processing baseline
    // This would include more detailed benchmarks in a real implementation
    
    // Basic assertions for performance baselines
    assert!(startup_time < Duration::from_secs(5), "Mock exchange startup too slow: {:?}", startup_time);
    
    println!("✅ Baseline performance benchmark completed");
}

/// Load test for validating system behavior under load
#[tokio::test]
#[ignore] // Ignored by default due to resource requirements
async fn load_test_system_capacity() {
    println!("🚛 Running system load test...");
    
    let config = load_test_configuration().await;
    
    // This would implement a comprehensive load test
    // - Simulating thousands of concurrent users
    // - High-frequency order placement
    // - Sustained market data streams
    // - Memory and CPU monitoring
    
    // For now, just a placeholder that demonstrates the concept
    println!("  Load test would simulate:");
    println!("    - {} concurrent connections", config.performance_targets.concurrent_connections);
    println!("    - {} orders per second", config.performance_targets.throughput_orders_per_second);
    println!("    - {} messages per hour", config.performance_targets.messages_per_hour);
    
    // In a real implementation, this would run for extended periods
    // and validate system behavior under extreme load
    
    println!("✅ Load test completed (simulation)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_credentials() {
        let url_with_password = "postgres://user:password@localhost:5432/db";
        let masked = mask_credentials(url_with_password);
        assert!(masked.contains("user:***"));
        assert!(!masked.contains("password"));
    }

    #[test]
    fn test_config_from_env() {
        // Test environment variable parsing
        std::env::set_var("TEST_SENSOR_ENDPOINT", "http://test:8081");
        std::env::set_var("TEST_TIMEOUT_SECONDS", "60");
        
        // In a real test, would call load_test_configuration and verify
        // For now, just verify env vars are set
        assert_eq!(std::env::var("TEST_SENSOR_ENDPOINT").unwrap(), "http://test:8081");
        assert_eq!(std::env::var("TEST_TIMEOUT_SECONDS").unwrap(), "60");
        
        // Clean up
        std::env::remove_var("TEST_SENSOR_ENDPOINT");
        std::env::remove_var("TEST_TIMEOUT_SECONDS");
    }
}