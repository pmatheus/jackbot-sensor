//! Test to verify new modules compile correctly

// Import all new modules to ensure they compile
use jackbot_sensor::exchange_websocket_config::{ExchangeWebSocketConfig, ExchangeWebSocketEndpoint};
use jackbot_sensor::websocket_connection_pool::{WebSocketConnectionPool, ConnectionHealth};
use jackbot_sensor::network_resilience::{
    CircuitBreaker, CircuitBreakerConfig, ExponentialBackoff, 
    FailoverManager, ResilientWebSocketConnection
};

#[test]
fn test_modules_compile() {
    // This test just ensures the modules compile
    println!("✅ All new modules compile successfully!");
    
    // Test ExchangeWebSocketConfig
    let config = ExchangeWebSocketConfig::production();
    assert!(config.has_exchange("binance"));
    assert!(config.has_exchange("coinbase"));
    
    // Test testnet config
    let testnet_config = ExchangeWebSocketConfig::testnet();
    assert!(testnet_config.has_exchange("binance"));
}

#[test]
fn test_no_localhost_in_production() {
    let config = ExchangeWebSocketConfig::production();
    
    for exchange in config.exchanges() {
        let endpoint = config.get_endpoint(exchange).unwrap();
        
        // Ensure no localhost in production URLs
        assert!(!endpoint.primary_url.contains("localhost"));
        assert!(!endpoint.primary_url.contains("127.0.0.1"));
        assert!(!endpoint.primary_url.contains(":8082"));
        
        // Verify real WebSocket URLs
        assert!(endpoint.primary_url.starts_with("wss://") || 
                endpoint.primary_url.starts_with("ws://"));
    }
}

#[test]
fn test_circuit_breaker_creation() {
    let config = CircuitBreakerConfig::default();
    let cb = CircuitBreaker::new(config);
    // Circuit breaker created successfully
}

#[test]
fn test_exchange_endpoints() {
    let config = ExchangeWebSocketConfig::production();
    
    // Test all 8 exchanges are configured
    let exchanges = vec![
        "binance", "coinbase", "bybit", "bitget",
        "hyperliquid", "kucoin", "kraken", "okx"
    ];
    
    for exchange in exchanges {
        assert!(
            config.has_exchange(exchange),
            "Exchange {} not configured", 
            exchange
        );
        
        let endpoint = config.get_endpoint(exchange).unwrap();
        println!("{}: {}", exchange, endpoint.primary_url);
    }
}