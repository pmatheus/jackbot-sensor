use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorConfig {
    pub deployment: DeploymentConfig,
    pub exchanges: HashMap<String, ExchangeConfig>,
    pub monitoring: MonitoringConfig,
    pub scaling: ScalingConfig,
    pub discovery: DiscoveryConfig,
    pub api: ApiConfig,
    pub data: DataConfig,
    pub risk: RiskConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    pub region: String,
    pub instance_type: String,
    pub max_instances: usize,
    pub min_instances: usize,
    pub pairs_per_instance: usize,
    pub auto_scaling: bool,
    pub health_check_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub api_passphrase: Option<String>,
    pub testnet: bool,
    pub rate_limit_buffer: f64,
    pub max_connections: usize,
    pub reconnect_interval: u64,
    pub supported_markets: Vec<String>, // spot, futures, options
    pub priority_pairs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub prometheus_port: u16,
    pub health_port: u16,
    pub metrics_interval: u64,
    pub datadog_api_key: Option<String>,
    pub sentry_dsn: Option<String>,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingConfig {
    pub cpu_threshold: f64,
    pub memory_threshold: f64,
    pub pairs_threshold: usize,
    pub scale_up_cooldown: u64,
    pub scale_down_cooldown: u64,
    pub max_scale_up_instances: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    pub check_interval: u64,
    pub new_pair_alert_channels: Vec<String>,
    pub coverage_threshold: f64,
    pub priority_assets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub rest_port: u16,
    pub websocket_port: u16,
    pub admin_port: u16,
    pub enable_cors: bool,
    pub max_connections: usize,
    pub rate_limit_per_minute: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataConfig {
    pub redis_url: String,
    pub message_broker: MessageBrokerConfig,
    pub kinesis_stream_prefix: String,
    pub s3_bucket: String,
    pub batch_size: usize,
    pub flush_interval: u64,
    pub data_lake_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBrokerConfig {
    pub brokers: String,
    pub consumer_group: String,
    pub topic_prefix: String,
    pub compression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub max_daily_loss_usd: f64,
    pub max_position_size_usd: f64,
    pub max_position_risk_percent: f64,
    pub circuit_breaker_threshold: usize,
    pub enable_trading: bool,
    pub enable_margin_trading: bool,
    pub enable_futures_trading: bool,
    pub default_leverage: f64,
}

impl SensorConfig {
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .context("Failed to read config file")?;
        
        let config: SensorConfig = toml::from_str(&content)
            .context("Failed to parse config file")?;
        
        config.validate()?;
        Ok(config)
    }
    
    pub fn validate(&self) -> Result<()> {
        // Validate deployment config
        if self.deployment.max_instances <= self.deployment.min_instances {
            anyhow::bail!("max_instances must be greater than min_instances");
        }
        
        if self.deployment.pairs_per_instance == 0 {
            anyhow::bail!("pairs_per_instance must be greater than 0");
        }
        
        // Validate at least one exchange is enabled
        if !self.exchanges.values().any(|e| e.enabled) {
            anyhow::bail!("At least one exchange must be enabled");
        }
        
        // Validate monitoring ports don't conflict
        let ports = vec![
            self.monitoring.prometheus_port,
            self.monitoring.health_port,
            self.api.rest_port,
            self.api.websocket_port,
            self.api.admin_port,
        ];
        
        let unique_ports: std::collections::HashSet<_> = ports.iter().collect();
        if unique_ports.len() != ports.len() {
            anyhow::bail!("Port conflicts detected in configuration");
        }
        
        // Validate risk limits
        if self.risk.max_daily_loss_usd <= 0.0 {
            anyhow::bail!("max_daily_loss_usd must be positive");
        }
        
        if self.risk.default_leverage <= 0.0 {
            anyhow::bail!("default_leverage must be positive");
        }
        
        Ok(())
    }
    
    pub fn get_enabled_exchanges(&self) -> Vec<String> {
        self.exchanges
            .iter()
            .filter_map(|(name, config)| {
                if config.enabled {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    pub fn get_total_pairs_capacity(&self) -> usize {
        self.deployment.max_instances * self.deployment.pairs_per_instance
    }
    
    pub fn calculate_required_instances(&self, total_pairs: usize) -> usize {
        let required = (total_pairs as f64 / self.deployment.pairs_per_instance as f64).ceil() as usize;
        required.max(self.deployment.min_instances).min(self.deployment.max_instances)
    }
}

impl Default for SensorConfig {
    fn default() -> Self {
        let mut exchanges = HashMap::new();
        
        // Default exchange configurations
        exchanges.insert("binance".to_string(), ExchangeConfig {
            enabled: true,
            api_key: None,
            api_secret: None,
            api_passphrase: None,
            testnet: false,
            rate_limit_buffer: 0.8,
            max_connections: 10,
            reconnect_interval: 5000,
            supported_markets: vec!["spot".to_string(), "futures".to_string()],
            priority_pairs: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
        });
        
        exchanges.insert("coinbase".to_string(), ExchangeConfig {
            enabled: true,
            api_key: None,
            api_secret: None,
            api_passphrase: None,
            testnet: false,
            rate_limit_buffer: 0.8,
            max_connections: 5,
            reconnect_interval: 5000,
            supported_markets: vec!["spot".to_string()],
            priority_pairs: vec!["BTC/USD".to_string(), "ETH/USD".to_string()],
        });
        
        Self {
            deployment: DeploymentConfig {
                region: "us-east-1".to_string(),
                instance_type: "t4g.nano".to_string(),
                max_instances: 100,
                min_instances: 10,
                pairs_per_instance: 75,
                auto_scaling: true,
                health_check_interval: 30,
            },
            exchanges,
            monitoring: MonitoringConfig {
                prometheus_port: 9090,
                health_port: 8080,
                metrics_interval: 60,
                datadog_api_key: None,
                sentry_dsn: None,
                log_level: "info".to_string(),
            },
            scaling: ScalingConfig {
                cpu_threshold: 70.0,
                memory_threshold: 80.0,
                pairs_threshold: 90,
                scale_up_cooldown: 300,
                scale_down_cooldown: 600,
                max_scale_up_instances: 20,
            },
            discovery: DiscoveryConfig {
                check_interval: 300,
                new_pair_alert_channels: vec!["kinesis".to_string(), "sns".to_string()],
                coverage_threshold: 99.0,
                priority_assets: vec![
                    "BTC".to_string(),
                    "ETH".to_string(),
                    "BNB".to_string(),
                    "SOL".to_string(),
                ],
            },
            api: ApiConfig {
                rest_port: 8080,
                websocket_port: 8081,
                admin_port: 9090,
                enable_cors: true,
                max_connections: 1000,
                rate_limit_per_minute: 1000,
            },
            data: DataConfig {
                message_broker: MessageBrokerConfig {
                    brokers: "localhost:9092".to_string(),
                    consumer_group: "jackbot-sensors".to_string(),
                    topic_prefix: "jackbot".to_string(),
                    compression: "snappy".to_string(),
                },
                kinesis_stream_prefix: "jackbot-market-data-".to_string(),
                s3_bucket: "jackbot-data-lake".to_string(),
                batch_size: 1000,
                flush_interval: 5000,
                data_lake_enabled: true,
            },
            risk: RiskConfig {
                max_daily_loss_usd: 10000.0,
                max_position_size_usd: 100000.0,
                max_position_risk_percent: 2.0,
                circuit_breaker_threshold: 5,
                enable_trading: true,
                enable_margin_trading: false,
                enable_futures_trading: true,
                default_leverage: 1.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;
    
    #[tokio::test]
    async fn test_config_load_and_validate() {
        let config = SensorConfig::default();
        let toml_content = toml::to_string(&config).unwrap();
        
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();
        
        let loaded_config = SensorConfig::load(temp_file.path()).await.unwrap();
        assert_eq!(loaded_config.deployment.region, "us-east-1");
        assert!(loaded_config.exchanges.contains_key("binance"));
        assert!(loaded_config.get_enabled_exchanges().contains(&"binance".to_string()));
    }
    
    #[test]
    fn test_calculate_required_instances() {
        let config = SensorConfig::default();
        
        // Test with exact multiple
        assert_eq!(config.calculate_required_instances(750), 10); // 750 / 75 = 10
        
        // Test with remainder
        assert_eq!(config.calculate_required_instances(800), 11); // 800 / 75 = 10.67 -> 11
        
        // Test below minimum
        assert_eq!(config.calculate_required_instances(100), 10); // Below min_instances
        
        // Test above maximum (simulated)
        let mut config = SensorConfig::default();
        config.deployment.max_instances = 5;
        assert_eq!(config.calculate_required_instances(1000), 5); // Capped at max
    }
}