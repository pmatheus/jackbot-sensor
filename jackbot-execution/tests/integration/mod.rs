/// Integration Test Suite for Jackbot Cross-Component Integration
/// 
/// This module provides comprehensive integration testing across:
/// - Sensor ↔ Backend ↔ Terminal components
/// - Market data flow validation
/// - Order execution end-to-end testing
/// - Performance and reliability validation

pub mod infrastructure;
pub mod market_data_flow;
pub mod order_execution_flow;
pub mod performance_tests;
pub mod functional_tests;
pub mod mock_services;

use std::time::Duration;
use tokio::time::timeout;
use serde::{Deserialize, Serialize};

/// Integration test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationTestConfig {
    pub sensor_endpoint: String,
    pub backend_endpoint: String,
    pub terminal_endpoint: String,
    pub kafka_brokers: Vec<String>,
    pub test_database_url: String,
    pub mock_exchange_port: u16,
    pub timeout_seconds: u64,
    pub performance_targets: PerformanceTargets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTargets {
    pub market_data_latency_ms: u64,  // <100ms end-to-end
    pub order_execution_latency_ms: u64,  // <1000ms total
    pub websocket_update_latency_ms: u64,  // <50ms
    pub database_query_latency_ms: u64,  // <50ms
    pub throughput_orders_per_second: u32,  // 100 orders/second
    pub concurrent_connections: u32,  // 10,000 connections
    pub messages_per_hour: u64,  // 1M Kafka messages/hour
}

impl Default for IntegrationTestConfig {
    fn default() -> Self {
        Self {
            sensor_endpoint: "http://localhost:8081".to_string(),
            backend_endpoint: "http://localhost:8080".to_string(),
            terminal_endpoint: "ws://localhost:8082".to_string(),
            kafka_brokers: vec!["localhost:9092".to_string()],
            test_database_url: "postgres://test:test@localhost:5433/jackbot_test".to_string(),
            mock_exchange_port: 8090,
            timeout_seconds: 30,
            performance_targets: PerformanceTargets {
                market_data_latency_ms: 100,
                order_execution_latency_ms: 1000,
                websocket_update_latency_ms: 50,
                database_query_latency_ms: 50,
                throughput_orders_per_second: 100,
                concurrent_connections: 10000,
                messages_per_hour: 1_000_000,
            },
        }
    }
}

/// Integration test result
#[derive(Debug, Serialize, Deserialize)]
pub struct IntegrationTestResult {
    pub test_name: String,
    pub success: bool,
    pub duration_ms: u64,
    pub error_message: Option<String>,
    pub performance_metrics: Option<PerformanceMetrics>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub latency_ms: u64,
    pub throughput: f64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    pub errors_count: u32,
}

/// Test suite manager
pub struct IntegrationTestSuite {
    config: IntegrationTestConfig,
    results: Vec<IntegrationTestResult>,
}

impl IntegrationTestSuite {
    pub fn new(config: IntegrationTestConfig) -> Self {
        Self {
            config,
            results: Vec::new(),
        }
    }

    pub async fn run_all_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting Jackbot Integration Test Suite");
        
        // Start infrastructure
        self.setup_test_infrastructure().await?;
        
        // Run test categories
        self.run_market_data_flow_tests().await?;
        self.run_order_execution_tests().await?;
        self.run_performance_tests().await?;
        self.run_functional_tests().await?;
        
        // Generate report
        self.generate_test_report().await?;
        
        // Cleanup
        self.cleanup_test_infrastructure().await?;
        
        Ok(())
    }

    async fn setup_test_infrastructure(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📦 Setting up test infrastructure...");
        
        // Start mock exchange server
        infrastructure::MockExchangeServer::start(self.config.mock_exchange_port).await?;
        
        // Setup test Kafka environment
        infrastructure::TestKafkaEnvironment::setup(&self.config.kafka_brokers).await?;
        
        // Initialize test database
        infrastructure::TestDatabase::initialize(&self.config.test_database_url).await?;
        
        println!("✅ Test infrastructure ready");
        Ok(())
    }

    async fn run_market_data_flow_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📊 Running market data flow tests...");
        
        let test_result = timeout(
            Duration::from_secs(self.config.timeout_seconds),
            market_data_flow::test_end_to_end_market_data_flow(&self.config)
        ).await??;
        
        self.results.push(test_result);
        Ok(())
    }

    async fn run_order_execution_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("💼 Running order execution tests...");
        
        let test_result = timeout(
            Duration::from_secs(self.config.timeout_seconds),
            order_execution_flow::test_end_to_end_order_execution(&self.config)
        ).await??;
        
        self.results.push(test_result);
        Ok(())
    }

    async fn run_performance_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("⚡ Running performance tests...");
        
        let test_results = timeout(
            Duration::from_secs(self.config.timeout_seconds * 2), // Performance tests take longer
            performance_tests::run_all_performance_tests(&self.config)
        ).await??;
        
        self.results.extend(test_results);
        Ok(())
    }

    async fn run_functional_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Running functional integration tests...");
        
        let test_results = timeout(
            Duration::from_secs(self.config.timeout_seconds),
            functional_tests::run_all_functional_tests(&self.config)
        ).await??;
        
        self.results.extend(test_results);
        Ok(())
    }

    async fn generate_test_report(&self) -> Result<(), Box<dyn std::error::Error>> {
        let total_tests = self.results.len();
        let passed_tests = self.results.iter().filter(|r| r.success).count();
        let failed_tests = total_tests - passed_tests;
        
        println!("\n📋 Integration Test Report");
        println!("========================");
        println!("Total Tests: {}", total_tests);
        println!("Passed: {} ✅", passed_tests);
        println!("Failed: {} ❌", failed_tests);
        println!("Success Rate: {:.1}%", (passed_tests as f64 / total_tests as f64) * 100.0);
        
        if failed_tests > 0 {
            println!("\n❌ Failed Tests:");
            for result in &self.results {
                if !result.success {
                    println!("  - {}: {}", result.test_name, 
                        result.error_message.as_deref().unwrap_or("Unknown error"));
                }
            }
        }
        
        // Write detailed report to file
        let report_json = serde_json::to_string_pretty(&self.results)?;
        tokio::fs::write("integration_test_report.json", report_json).await?;
        println!("\n📄 Detailed report saved to: integration_test_report.json");
        
        Ok(())
    }

    async fn cleanup_test_infrastructure(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🧹 Cleaning up test infrastructure...");
        
        // Stop mock services
        infrastructure::MockExchangeServer::stop().await?;
        infrastructure::TestKafkaEnvironment::cleanup().await?;
        infrastructure::TestDatabase::cleanup().await?;
        
        println!("✅ Cleanup complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_integration_suite_initialization() {
        let config = IntegrationTestConfig::default();
        let suite = IntegrationTestSuite::new(config);
        assert_eq!(suite.results.len(), 0);
    }
}