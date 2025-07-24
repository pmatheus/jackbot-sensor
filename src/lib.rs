// Infinite Agent Mode - Minimal Library Implementation
// This version provides minimal functionality to support the infinite agent binary

pub mod api;
pub mod auth;
pub mod security;
pub mod config;
pub mod sensor;
pub mod connector;
pub mod streaming;
pub mod validation;
pub mod rate_limit;
pub mod order_processor;
pub mod monitor;
pub mod discovery;
pub mod distribution;
pub mod smart_routing;
pub mod order_book_aggregator;
pub mod performance_benchmarks;
pub mod circuit_breaker;
pub mod performance;
pub mod exchange_protection;
pub mod production_config;
pub mod connectors;
pub mod exchange_websocket_config;
pub mod websocket_connection_pool;
pub mod network_resilience;
pub mod streaming_real;
pub mod order_book_aggregator_ultra;
pub mod market_arbitrage;
pub mod strategy_execution;
pub mod connection_pool;
pub mod zero_copy_parser;
pub mod latency_monitor;
pub mod performance_test_integration;
pub mod security_validation_test;
pub mod kafka_subscriber;
// pub mod defi; // Temporarily disabled due to missing dependencies

// Minimal placeholder for library functionality
pub struct JackbotSensor;

impl Default for JackbotSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl JackbotSensor {
    pub fn new() -> Self {
        Self
    }
}
