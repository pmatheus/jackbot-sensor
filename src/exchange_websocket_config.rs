//! Real Exchange WebSocket Configuration
//!
//! This module provides REAL production WebSocket endpoints for all supported exchanges.
//! Replaces the localhost:8082 mock service with actual exchange connections.
//! Designed for <10ms latency Bloomberg Terminal competition.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Exchange WebSocket endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeWebSocketEndpoint {
    /// Primary WebSocket URL
    pub primary_url: String,
    
    /// Backup/failover WebSocket URLs (different regions)
    pub backup_urls: Vec<String>,
    
    /// Regional endpoints for latency optimization
    pub regional_endpoints: HashMap<String, String>,
    
    /// Whether this is a testnet endpoint
    pub is_testnet: bool,
    
    /// Connection timeout
    pub connection_timeout: Duration,
    
    /// Heartbeat interval (ping/pong)
    pub heartbeat_interval: Duration,
    
    /// Maximum message size in bytes
    pub max_message_size: usize,
    
    /// Rate limit per connection
    pub rate_limit_per_second: u32,
}

/// Complete exchange WebSocket configuration
#[derive(Debug, Clone)]
pub struct ExchangeWebSocketConfig {
    endpoints: HashMap<String, ExchangeWebSocketEndpoint>,
}

impl ExchangeWebSocketConfig {
    /// Create production WebSocket configuration for all exchanges
    pub fn production() -> Self {
        let mut endpoints = HashMap::new();
        
        // Binance - World's largest exchange by volume
        endpoints.insert("binance".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://stream.binance.com:9443/ws".to_string(),
            backup_urls: vec![
                "wss://stream.binance.com:443/ws".to_string(),
                "wss://stream1.binance.com:9443/ws".to_string(),
                "wss://stream2.binance.com:9443/ws".to_string(),
                "wss://stream3.binance.com:9443/ws".to_string(),
            ],
            regional_endpoints: HashMap::from([
                ("us".to_string(), "wss://stream.binance.us:9443/ws".to_string()),
                ("eu".to_string(), "wss://stream.binance.com:9443/ws".to_string()),
                ("asia".to_string(), "wss://stream-asia.binance.com:9443/ws".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 10, // Per connection limit
        });
        
        // Coinbase - US regulated exchange
        endpoints.insert("coinbase".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://ws-feed.exchange.coinbase.com".to_string(),
            backup_urls: vec![
                "wss://ws-feed.prime.coinbase.com".to_string(),
                "wss://ws-feed-public.exchange.coinbase.com".to_string(),
            ],
            regional_endpoints: HashMap::from([
                ("us".to_string(), "wss://ws-feed.exchange.coinbase.com".to_string()),
                ("eu".to_string(), "wss://ws-feed-eu.exchange.coinbase.com".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 10,
        });
        
        // Bybit - Major derivatives exchange
        endpoints.insert("bybit".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://stream.bybit.com/v5/public/spot".to_string(),
            backup_urls: vec![
                "wss://stream.bybit.com/v5/public/linear".to_string(),
                "wss://stream-backup.bybit.com/v5/public/spot".to_string(),
            ],
            regional_endpoints: HashMap::from([
                ("global".to_string(), "wss://stream.bybit.com/v5/public/spot".to_string()),
                ("asia".to_string(), "wss://stream-asia.bybit.com/v5/public/spot".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(20),
            max_message_size: 65536,
            rate_limit_per_second: 20,
        });
        
        // Bitget - Growing exchange with good liquidity
        endpoints.insert("bitget".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://ws.bitget.com/v2/ws/public".to_string(),
            backup_urls: vec![
                "wss://ws-api.bitget.com/v2/ws/public".to_string(),
            ],
            regional_endpoints: HashMap::from([
                ("global".to_string(), "wss://ws.bitget.com/v2/ws/public".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 20,
        });
        
        // Hyperliquid - Decentralized perpetuals exchange
        endpoints.insert("hyperliquid".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://api.hyperliquid.xyz/ws".to_string(),
            backup_urls: vec![],
            regional_endpoints: HashMap::from([
                ("global".to_string(), "wss://api.hyperliquid.xyz/ws".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 20,
        });
        
        // KuCoin - Popular altcoin exchange
        endpoints.insert("kucoin".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://ws-api-spot.kucoin.com".to_string(),
            backup_urls: vec![
                "wss://push1-v2.kucoin.com/endpoint".to_string(),
            ],
            regional_endpoints: HashMap::from([
                ("global".to_string(), "wss://ws-api-spot.kucoin.com".to_string()),
                ("futures".to_string(), "wss://ws-api-futures.kucoin.com".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 100, // KuCoin allows high throughput
        });
        
        // Kraken - Established US exchange
        endpoints.insert("kraken".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://ws.kraken.com".to_string(),
            backup_urls: vec![
                "wss://ws-auth.kraken.com".to_string(), // For authenticated streams
            ],
            regional_endpoints: HashMap::from([
                ("public".to_string(), "wss://ws.kraken.com".to_string()),
                ("private".to_string(), "wss://ws-auth.kraken.com".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 50,
        });
        
        // OKX (formerly OKEx) - Major global exchange
        endpoints.insert("okx".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://ws.okx.com:8443/ws/v5/public".to_string(),
            backup_urls: vec![
                "wss://wsaws.okx.com:8443/ws/v5/public".to_string(), // AWS endpoint
            ],
            regional_endpoints: HashMap::from([
                ("public".to_string(), "wss://ws.okx.com:8443/ws/v5/public".to_string()),
                ("private".to_string(), "wss://ws.okx.com:8443/ws/v5/private".to_string()),
                ("business".to_string(), "wss://ws.okx.com:8443/ws/v5/business".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 20,
        });
        
        // Gate.io - Top 10 global exchange with good liquidity
        endpoints.insert("gateio".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://api.gateio.ws/ws/4".to_string(),
            backup_urls: vec![
                "wss://ws.gate.io/v4".to_string(),
                "wss://api.gateio.hk/ws/4".to_string(), // Hong Kong endpoint
            ],
            regional_endpoints: HashMap::from([
                ("global".to_string(), "wss://api.gateio.ws/ws/4".to_string()),
                ("asia".to_string(), "wss://api.gateio.hk/ws/4".to_string()),
                ("futures".to_string(), "wss://fx-ws.gateio.ws/v4/ws/usdt".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 100, // Gate.io has generous rate limits
        });
        
        // MEXC - Fast-growing exchange with deep liquidity
        endpoints.insert("mexc".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://wbs.mexc.com/ws".to_string(),
            backup_urls: vec![
                "wss://wbs.mexc.com/raw/ws".to_string(),
                "wss://contract.mexc.com/ws".to_string(), // Futures endpoint
            ],
            regional_endpoints: HashMap::from([
                ("spot".to_string(), "wss://wbs.mexc.com/ws".to_string()),
                ("futures".to_string(), "wss://contract.mexc.com/ws".to_string()),
                ("global".to_string(), "wss://wbs.mexc.com/ws".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 50,
        });
        
        // BingX - Rising exchange with competitive fees
        endpoints.insert("bingx".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://open-api-ws.bingx.com/market".to_string(),
            backup_urls: vec![
                "wss://open-api-ws.bingx.com/swap".to_string(), // Swap trading
            ],
            regional_endpoints: HashMap::from([
                ("spot".to_string(), "wss://open-api-ws.bingx.com/market".to_string()),
                ("swap".to_string(), "wss://open-api-ws.bingx.com/swap".to_string()),
                ("global".to_string(), "wss://open-api-ws.bingx.com/market".to_string()),
            ]),
            is_testnet: false,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 100, // BingX allows high throughput
        });
        
        Self { endpoints }
    }
    
    /// Create testnet/sandbox WebSocket configuration
    pub fn testnet() -> Self {
        let mut endpoints = HashMap::new();
        
        // Binance Testnet
        endpoints.insert("binance".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://testnet.binance.vision/ws".to_string(),
            backup_urls: vec![
                "wss://stream.binancefuture.com/ws".to_string(), // Futures testnet
            ],
            regional_endpoints: HashMap::new(),
            is_testnet: true,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 20,
        });
        
        // Coinbase Sandbox
        endpoints.insert("coinbase".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://ws-feed-public.sandbox.exchange.coinbase.com".to_string(),
            backup_urls: vec![],
            regional_endpoints: HashMap::new(),
            is_testnet: true,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 10,
        });
        
        // Bybit Testnet
        endpoints.insert("bybit".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://stream-testnet.bybit.com/v5/public/spot".to_string(),
            backup_urls: vec![],
            regional_endpoints: HashMap::new(),
            is_testnet: true,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(20),
            max_message_size: 65536,
            rate_limit_per_second: 20,
        });
        
        // KuCoin Sandbox
        endpoints.insert("kucoin".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://openapi-ws-sandbox.kucoin.com/endpoint".to_string(),
            backup_urls: vec![],
            regional_endpoints: HashMap::new(),
            is_testnet: true,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 100,
        });
        
        // OKX Demo Trading
        endpoints.insert("okx".to_string(), ExchangeWebSocketEndpoint {
            primary_url: "wss://wspap.okx.com:8443/ws/v5/public?brokerId=9999".to_string(),
            backup_urls: vec![],
            regional_endpoints: HashMap::new(),
            is_testnet: true,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            max_message_size: 65536,
            rate_limit_per_second: 20,
        });
        
        // Note: Not all exchanges provide testnet WebSocket endpoints
        // Bitget, Hyperliquid, and Kraken don't have public testnet WebSockets
        
        Self { endpoints }
    }
    
    /// Get WebSocket endpoint for an exchange
    pub fn get_endpoint(&self, exchange: &str) -> Option<&ExchangeWebSocketEndpoint> {
        self.endpoints.get(exchange)
    }
    
    /// Get the best WebSocket URL based on latency testing
    pub async fn get_optimal_url(&self, exchange: &str, region: Option<&str>) -> Result<String> {
        let endpoint = self.get_endpoint(exchange)
            .ok_or_else(|| anyhow::anyhow!("Unknown exchange: {}", exchange))?;
        
        // If region is specified, try to use regional endpoint
        if let Some(region) = region {
            if let Some(regional_url) = endpoint.regional_endpoints.get(region) {
                return Ok(regional_url.clone());
            }
        }
        
        // Latency testing for endpoint optimization - see LATENCY_TESTING_SPEC.md
        // For now, return primary URL
        Ok(endpoint.primary_url.clone())
    }
    
    /// Get all configured exchanges
    pub fn exchanges(&self) -> Vec<&str> {
        self.endpoints.keys().map(|s| s.as_str()).collect()
    }
    
    /// Check if an exchange is configured
    pub fn has_exchange(&self, exchange: &str) -> bool {
        self.endpoints.contains_key(exchange)
    }
}

/// WebSocket connection health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketHealthMetrics {
    pub exchange: String,
    pub url: String,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_message_at: chrono::DateTime<chrono::Utc>,
    pub messages_received: u64,
    pub messages_sent: u64,
    pub reconnect_count: u32,
    pub average_latency_ms: f64,
    pub is_healthy: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_production_config_has_all_exchanges() {
        let config = ExchangeWebSocketConfig::production();
        let exchanges = config.exchanges();
        
        assert_eq!(exchanges.len(), 11);
        assert!(exchanges.contains(&"binance"));
        assert!(exchanges.contains(&"coinbase"));
        assert!(exchanges.contains(&"bybit"));
        assert!(exchanges.contains(&"bitget"));
        assert!(exchanges.contains(&"hyperliquid"));
        assert!(exchanges.contains(&"kucoin"));
        assert!(exchanges.contains(&"kraken"));
        assert!(exchanges.contains(&"okx"));
        assert!(exchanges.contains(&"gateio"));
        assert!(exchanges.contains(&"mexc"));
        assert!(exchanges.contains(&"bingx"));
    }
    
    #[test]
    fn test_no_localhost_in_production() {
        let config = ExchangeWebSocketConfig::production();
        
        for exchange in config.exchanges() {
            let endpoint = config.get_endpoint(exchange).unwrap();
            assert!(!endpoint.primary_url.contains("localhost"));
            assert!(!endpoint.primary_url.contains("127.0.0.1"));
            
            for backup_url in &endpoint.backup_urls {
                assert!(!backup_url.contains("localhost"));
                assert!(!backup_url.contains("127.0.0.1"));
            }
        }
    }
    
    #[tokio::test]
    async fn test_get_optimal_url() {
        let config = ExchangeWebSocketConfig::production();
        
        // Test primary URL
        let url = config.get_optimal_url("binance", None).await.unwrap();
        assert_eq!(url, "wss://stream.binance.com:9443/ws");
        
        // Test regional URL
        let url = config.get_optimal_url("binance", Some("us")).await.unwrap();
        assert_eq!(url, "wss://stream.binance.us:9443/ws");
    }
}