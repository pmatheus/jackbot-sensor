//! Production configuration for Jackbot Sensor
//!
//! This module handles production-specific configurations for real exchange
//! integrations, including API credentials, rate limits, and performance settings.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use tracing::info;

/// Production configuration for exchange integrations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionConfig {
    /// Environment mode (local, dev, staging, prod)
    pub environment: String,
    
    /// Exchange configurations
    pub exchanges: HashMap<String, ExchangeConfig>,
    
    /// Performance settings
    pub performance: PerformanceConfig,
    
    /// API endpoints
    pub endpoints: EndpointConfig,
    
    /// Security settings
    pub security: SecurityConfig,
}

/// Configuration for a specific exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    /// Whether this exchange is enabled
    pub enabled: bool,
    
    /// API base URL (can be sandbox or production)
    pub api_url: String,
    
    /// WebSocket URL
    pub ws_url: String,
    
    /// Whether to use sandbox/testnet
    pub sandbox: bool,
    
    /// Rate limit settings
    pub rate_limits: RateLimitConfig,
    
    /// Supported features
    pub features: ExchangeFeatures,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per second
    pub requests_per_second: u32,
    
    /// Orders per minute
    pub orders_per_minute: u32,
    
    /// WebSocket messages per second
    pub ws_messages_per_second: u32,
}

/// Exchange feature support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeFeatures {
    /// Spot trading support
    pub spot_trading: bool,
    
    /// Futures trading support
    pub futures_trading: bool,
    
    /// Margin trading support
    pub margin_trading: bool,
    
    /// WebSocket user data support
    pub user_data_ws: bool,
    
    /// Market data streams
    pub market_data_ws: bool,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Target API response time in milliseconds
    pub target_api_response_ms: u64,
    
    /// Target market data latency in milliseconds
    pub target_market_data_latency_ms: u64,
    
    /// Maximum order execution time in milliseconds
    pub max_order_execution_ms: u64,
    
    /// WebSocket reconnection settings
    pub ws_reconnect: WebSocketConfig,
}

/// WebSocket configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Connection timeout in seconds
    pub connection_timeout_seconds: u64,
    
    /// Heartbeat interval in seconds
    pub heartbeat_interval_seconds: u64,
    
    /// Maximum reconnection attempts
    pub max_reconnection_attempts: u32,
    
    /// Reconnection delay in seconds
    pub reconnection_delay_seconds: u64,
}

/// API endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointConfig {
    /// Kafka brokers
    pub kafka_brokers: String,
    
    /// Redis URL
    pub redis_url: String,
    
    /// GraphQL endpoint
    pub graphql_endpoint: String,
    
    /// WebSocket endpoint
    pub websocket_endpoint: String,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable request signing
    pub enable_request_signing: bool,
    
    /// Enable IP whitelisting
    pub enable_ip_whitelist: bool,
    
    /// API key rotation interval in hours
    pub api_key_rotation_hours: u64,
    
    /// Enable audit logging
    pub enable_audit_logging: bool,
}

impl ProductionConfig {
    /// Create production configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let environment = env::var("JACKBOT_ENV").unwrap_or_else(|_| "local".to_string());
        
        info!("🚀 Initializing Production Config for environment: {}", environment);
        
        let mut exchanges = HashMap::new();
        
        // Configure all 8 supported exchanges
        exchanges.insert("binance".to_string(), Self::binance_config(&environment)?);
        exchanges.insert("coinbase".to_string(), Self::coinbase_config(&environment)?);
        exchanges.insert("bybit".to_string(), Self::bybit_config(&environment)?);
        exchanges.insert("bitget".to_string(), Self::bitget_config(&environment)?);
        exchanges.insert("hyperliquid".to_string(), Self::hyperliquid_config(&environment)?);
        exchanges.insert("kucoin".to_string(), Self::kucoin_config(&environment)?);
        exchanges.insert("kraken".to_string(), Self::kraken_config(&environment)?);
        exchanges.insert("okx".to_string(), Self::okx_config(&environment)?);
        
        let performance = PerformanceConfig {
            target_api_response_ms: env::var("JACKBOT_TARGET_API_RESPONSE_MS")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .unwrap_or(50),
            target_market_data_latency_ms: env::var("JACKBOT_TARGET_MARKET_DATA_LATENCY_MS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
            max_order_execution_ms: env::var("JACKBOT_MAX_ORDER_EXECUTION_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse()
                .unwrap_or(5000),
            ws_reconnect: WebSocketConfig {
                connection_timeout_seconds: 10,
                heartbeat_interval_seconds: 30,
                max_reconnection_attempts: 10,
                reconnection_delay_seconds: 5,
            },
        };
        
        let endpoints = EndpointConfig {
            kafka_brokers: env::var("KAFKA_BROKERS")
                .unwrap_or_else(|_| "localhost:9092".to_string()),
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            graphql_endpoint: env::var("GRAPHQL_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:8082/graphql".to_string()),
            websocket_endpoint: env::var("WEBSOCKET_ENDPOINT")
                .unwrap_or_else(|_| "ws://localhost:8082/ws".to_string()),
        };
        
        let security = SecurityConfig {
            enable_request_signing: environment != "local",
            enable_ip_whitelist: environment == "prod",
            api_key_rotation_hours: if environment == "prod" { 24 } else { 168 }, // Daily in prod, weekly elsewhere
            enable_audit_logging: true,
        };
        
        info!("✅ Production config loaded: {} exchanges configured", exchanges.len());
        
        Ok(Self {
            environment,
            exchanges,
            performance,
            endpoints,
            security,
        })
    }
    
    /// Binance configuration
    fn binance_config(environment: &str) -> Result<ExchangeConfig> {
        let sandbox = environment == "local" || environment == "dev";
        
        Ok(ExchangeConfig {
            enabled: true,
            api_url: if sandbox {
                "https://testnet.binancefuture.com".to_string()
            } else {
                "https://fapi.binance.com".to_string()
            },
            ws_url: if sandbox {
                "wss://stream.binancefuture.com".to_string()
            } else {
                "wss://fstream.binance.com".to_string()
            },
            sandbox,
            rate_limits: RateLimitConfig {
                requests_per_second: 1200,
                orders_per_minute: 1200,
                ws_messages_per_second: 100,
            },
            features: ExchangeFeatures {
                spot_trading: true,
                futures_trading: true,
                margin_trading: true,
                user_data_ws: true,
                market_data_ws: true,
            },
        })
    }
    
    /// Coinbase configuration
    fn coinbase_config(environment: &str) -> Result<ExchangeConfig> {
        let sandbox = environment == "local" || environment == "dev";
        
        Ok(ExchangeConfig {
            enabled: true,
            api_url: if sandbox {
                "https://api-public.sandbox.exchange.coinbase.com".to_string()
            } else {
                "https://api.exchange.coinbase.com".to_string()
            },
            ws_url: if sandbox {
                "wss://ws-feed-public.sandbox.exchange.coinbase.com".to_string()
            } else {
                "wss://ws-feed.exchange.coinbase.com".to_string()
            },
            sandbox,
            rate_limits: RateLimitConfig {
                requests_per_second: 100,
                orders_per_minute: 300,
                ws_messages_per_second: 50,
            },
            features: ExchangeFeatures {
                spot_trading: true,
                futures_trading: false, // Coinbase doesn't offer futures
                margin_trading: true,
                user_data_ws: true,
                market_data_ws: true,
            },
        })
    }
    
    /// Bybit configuration
    fn bybit_config(environment: &str) -> Result<ExchangeConfig> {
        let sandbox = environment == "local" || environment == "dev";
        
        Ok(ExchangeConfig {
            enabled: true,
            api_url: if sandbox {
                "https://api-testnet.bybit.com".to_string()
            } else {
                "https://api.bybit.com".to_string()
            },
            ws_url: if sandbox {
                "wss://stream-testnet.bybit.com".to_string()
            } else {
                "wss://stream.bybit.com".to_string()
            },
            sandbox,
            rate_limits: RateLimitConfig {
                requests_per_second: 600,
                orders_per_minute: 600,
                ws_messages_per_second: 100,
            },
            features: ExchangeFeatures {
                spot_trading: true,
                futures_trading: true,
                margin_trading: true,
                user_data_ws: true,
                market_data_ws: true,
            },
        })
    }
    
    /// Bitget configuration
    fn bitget_config(environment: &str) -> Result<ExchangeConfig> {
        let sandbox = environment == "local" || environment == "dev";
        
        Ok(ExchangeConfig {
            enabled: true,
            api_url: if sandbox {
                "https://api.bitget.com".to_string() // Bitget uses same URL for both
            } else {
                "https://api.bitget.com".to_string()
            },
            ws_url: "wss://ws.bitget.com".to_string(),
            sandbox,
            rate_limits: RateLimitConfig {
                requests_per_second: 400,
                orders_per_minute: 400,
                ws_messages_per_second: 80,
            },
            features: ExchangeFeatures {
                spot_trading: true,
                futures_trading: true,
                margin_trading: true,
                user_data_ws: true,
                market_data_ws: true,
            },
        })
    }
    
    /// Hyperliquid configuration
    fn hyperliquid_config(environment: &str) -> Result<ExchangeConfig> {
        let sandbox = environment == "local" || environment == "dev";
        
        Ok(ExchangeConfig {
            enabled: true,
            api_url: "https://api.hyperliquid.xyz".to_string(),
            ws_url: "wss://api.hyperliquid.xyz/ws".to_string(),
            sandbox, // Hyperliquid uses mainnet with small amounts for testing
            rate_limits: RateLimitConfig {
                requests_per_second: 200,
                orders_per_minute: 200,
                ws_messages_per_second: 50,
            },
            features: ExchangeFeatures {
                spot_trading: false, // Hyperliquid is perps-only
                futures_trading: true,
                margin_trading: true,
                user_data_ws: true,
                market_data_ws: true,
            },
        })
    }
    
    /// KuCoin configuration
    fn kucoin_config(environment: &str) -> Result<ExchangeConfig> {
        let sandbox = environment == "local" || environment == "dev";
        
        Ok(ExchangeConfig {
            enabled: true,
            api_url: if sandbox {
                "https://openapi-sandbox.kucoin.com".to_string()
            } else {
                "https://api.kucoin.com".to_string()
            },
            ws_url: if sandbox {
                "wss://ws-api-sandbox.kucoin.com".to_string()
            } else {
                "wss://ws-api-spot.kucoin.com".to_string()
            },
            sandbox,
            rate_limits: RateLimitConfig {
                requests_per_second: 100,
                orders_per_minute: 300,
                ws_messages_per_second: 100,
            },
            features: ExchangeFeatures {
                spot_trading: true,
                futures_trading: true,
                margin_trading: true,
                user_data_ws: true,
                market_data_ws: true,
            },
        })
    }
    
    /// Kraken configuration
    fn kraken_config(environment: &str) -> Result<ExchangeConfig> {
        let sandbox = environment == "local" || environment == "dev";
        
        Ok(ExchangeConfig {
            enabled: true,
            api_url: "https://api.kraken.com".to_string(), // Kraken doesn't have a separate testnet
            ws_url: "wss://ws.kraken.com".to_string(),
            sandbox,
            rate_limits: RateLimitConfig {
                requests_per_second: 60, // Kraken has stricter limits
                orders_per_minute: 300,
                ws_messages_per_second: 50,
            },
            features: ExchangeFeatures {
                spot_trading: true,
                futures_trading: true,
                margin_trading: true,
                user_data_ws: true,
                market_data_ws: true,
            },
        })
    }
    
    /// OKX configuration
    fn okx_config(environment: &str) -> Result<ExchangeConfig> {
        let sandbox = environment == "local" || environment == "dev";
        
        Ok(ExchangeConfig {
            enabled: true,
            api_url: if sandbox {
                "https://www.okx.com".to_string() // OKX uses same URL with demo trading
            } else {
                "https://www.okx.com".to_string()
            },
            ws_url: if sandbox {
                "wss://wspap.okx.com:8443/ws/v5/public".to_string()
            } else {
                "wss://ws.okx.com:8443/ws/v5/public".to_string()
            },
            sandbox,
            rate_limits: RateLimitConfig {
                requests_per_second: 600,
                orders_per_minute: 600,
                ws_messages_per_second: 100,
            },
            features: ExchangeFeatures {
                spot_trading: true,
                futures_trading: true,
                margin_trading: true,
                user_data_ws: true,
                market_data_ws: true,
            },
        })
    }
    
    /// Get exchange configuration
    pub fn get_exchange_config(&self, exchange: &str) -> Option<&ExchangeConfig> {
        self.exchanges.get(exchange)
    }
    
    /// Get enabled exchanges
    pub fn get_enabled_exchanges(&self) -> Vec<String> {
        self.exchanges
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, _)| name.clone())
            .collect()
    }
    
    /// Check if exchange supports feature
    pub fn exchange_supports_feature(&self, exchange: &str, feature: &str) -> bool {
        if let Some(config) = self.exchanges.get(exchange) {
            match feature {
                "spot_trading" => config.features.spot_trading,
                "futures_trading" => config.features.futures_trading,
                "margin_trading" => config.features.margin_trading,
                "user_data_ws" => config.features.user_data_ws,
                "market_data_ws" => config.features.market_data_ws,
                _ => false,
            }
        } else {
            false
        }
    }
    
    /// Get performance targets
    pub fn get_performance_targets(&self) -> &PerformanceConfig {
        &self.performance
    }
    
    /// Check if we're in production mode
    pub fn is_production(&self) -> bool {
        self.environment == "prod"
    }
    
    /// Check if we're in local development mode
    pub fn is_local(&self) -> bool {
        self.environment == "local"
    }
    
    /// Get summary for logging
    pub fn get_summary(&self) -> String {
        let enabled_exchanges = self.get_enabled_exchanges();
        format!(
            "Environment: {} | Exchanges: {} | API Target: {}ms | Market Data Target: {}ms",
            self.environment,
            enabled_exchanges.join(", "),
            self.performance.target_api_response_ms,
            self.performance.target_market_data_latency_ms
        )
    }
}

impl Default for ProductionConfig {
    fn default() -> Self {
        Self::from_env().unwrap_or_else(|_| {
            // Fallback configuration
            Self {
                environment: "local".to_string(),
                exchanges: HashMap::new(),
                performance: PerformanceConfig {
                    target_api_response_ms: 50,
                    target_market_data_latency_ms: 100,
                    max_order_execution_ms: 5000,
                    ws_reconnect: WebSocketConfig {
                        connection_timeout_seconds: 10,
                        heartbeat_interval_seconds: 30,
                        max_reconnection_attempts: 10,
                        reconnection_delay_seconds: 5,
                    },
                },
                endpoints: EndpointConfig {
                    kafka_brokers: "localhost:9092".to_string(),
                    redis_url: "redis://localhost:6379".to_string(),
                    graphql_endpoint: "http://localhost:8082/graphql".to_string(),
                    websocket_endpoint: "ws://localhost:8082/ws".to_string(),
                },
                security: SecurityConfig {
                    enable_request_signing: false,
                    enable_ip_whitelist: false,
                    api_key_rotation_hours: 168,
                    enable_audit_logging: true,
                },
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_production_config_creation() {
        let config = ProductionConfig::default();
        assert_eq!(config.environment, "local");
    }
    
    #[test]
    fn test_exchange_feature_support() {
        let config = ProductionConfig::from_env().unwrap();
        
        // Test Binance features
        assert!(config.exchange_supports_feature("binance", "spot_trading"));
        assert!(config.exchange_supports_feature("binance", "futures_trading"));
        
        // Test Coinbase features (no futures)
        assert!(config.exchange_supports_feature("coinbase", "spot_trading"));
        assert!(!config.exchange_supports_feature("coinbase", "futures_trading"));
        
        // Test Hyperliquid features (futures only)
        assert!(!config.exchange_supports_feature("hyperliquid", "spot_trading"));
        assert!(config.exchange_supports_feature("hyperliquid", "futures_trading"));
    }
    
    #[test]
    fn test_enabled_exchanges() {
        let config = ProductionConfig::from_env().unwrap();
        let enabled = config.get_enabled_exchanges();
        
        // Should have all 8 exchanges enabled by default
        assert_eq!(enabled.len(), 8);
        assert!(enabled.contains(&"binance".to_string()));
        assert!(enabled.contains(&"coinbase".to_string()));
        assert!(enabled.contains(&"bybit".to_string()));
        assert!(enabled.contains(&"bitget".to_string()));
        assert!(enabled.contains(&"hyperliquid".to_string()));
        assert!(enabled.contains(&"kucoin".to_string()));
        assert!(enabled.contains(&"kraken".to_string()));
        assert!(enabled.contains(&"okx".to_string()));
    }
}